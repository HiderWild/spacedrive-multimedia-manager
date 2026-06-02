//! DB-aware tag resolution for the rule evaluator (task E-01, finish).
//!
//! Proves that a [`Condition::Tag`] evaluated against a target built with
//! [`resolve_rule_target`] matches the *effective* tag set: a tag attached
//! directly, inherited from an ancestor folder, or implied through a parent
//! relation. The pure `evaluate` is unchanged; only the target is pre-resolved.

use sd_core::domain::content_identity::ContentKind;
use sd_core::domain::tag::TagInheritanceSource;
use sd_core::infra::db::entities::{
	entry, entry_closure, tag, tag_parent, user_metadata, user_metadata_tag,
};
use sd_core::infra::db::Database;
use sd_core::ops::rules::{evaluate, resolve_rule_target, Condition, Rule, RuleTarget};
use sea_orm::{ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, QueryFilter, Set};
use tempfile::TempDir;
use uuid::Uuid;

/// Minimal rule target with no own tags; effective tags come from resolution.
struct TestEntry {
	path: String,
}

impl RuleTarget for TestEntry {
	fn path(&self) -> String {
		self.path.clone()
	}
	fn extension(&self) -> Option<&str> {
		None
	}
	fn size(&self) -> u64 {
		0
	}
	fn kind(&self) -> ContentKind {
		ContentKind::Unknown
	}
	fn width(&self) -> Option<u32> {
		None
	}
	fn height(&self) -> Option<u32> {
		None
	}
	fn duration_seconds(&self) -> Option<f64> {
		None
	}
	fn tag_names(&self) -> Vec<String> {
		Vec::new()
	}
}

async fn fresh_db() -> (TempDir, Database) {
	let temp = TempDir::new().unwrap();
	let db_path = temp.path().join("rules_tag_resolution.db");
	let db = Database::create(&db_path).await.expect("create db");
	db.migrate().await.expect("migrate");
	(temp, db)
}

async fn insert_closure(db: &DbConn, ancestor_id: i32, descendant_id: i32, depth: i32) {
	let closure = entry_closure::ActiveModel {
		ancestor_id: Set(ancestor_id),
		descendant_id: Set(descendant_id),
		depth: Set(depth),
	};
	closure.insert(db).await.expect("insert entry closure");
}

/// Insert an entry (folder/file) wiring its closure rows. Returns (db id, uuid).
async fn insert_entry(db: &DbConn, name: &str, parent_id: Option<i32>) -> (i32, Uuid) {
	let uuid = Uuid::new_v4();
	let now = chrono::Utc::now();
	let model = entry::ActiveModel {
		uuid: Set(Some(uuid)),
		name: Set(name.to_string()),
		kind: Set(1),
		size: Set(0),
		aggregate_size: Set(0),
		child_count: Set(0),
		file_count: Set(0),
		created_at: Set(now),
		modified_at: Set(now),
		parent_id: Set(parent_id),
		..Default::default()
	};
	let inserted = model.insert(db).await.expect("insert entry");
	insert_closure(db, inserted.id, inserted.id, 0).await;
	if let Some(parent_id) = parent_id {
		let parent_closures = entry_closure::Entity::find()
			.filter(entry_closure::Column::DescendantId.eq(parent_id))
			.all(db)
			.await
			.expect("load parent closures");
		for parent_closure in parent_closures {
			insert_closure(
				db,
				parent_closure.ancestor_id,
				inserted.id,
				parent_closure.depth + 1,
			)
			.await;
		}
	}
	(inserted.id, uuid)
}

/// Insert a tag and return its db id.
async fn insert_tag(db: &DbConn, name: &str) -> i32 {
	let now = chrono::Utc::now();
	let model = tag::ActiveModel {
		uuid: Set(Uuid::new_v4()),
		canonical_name: Set(name.to_string()),
		tag_type: Set("standard".to_string()),
		is_organizational_anchor: Set(false),
		privacy_level: Set("normal".to_string()),
		search_weight: Set(100),
		created_at: Set(now),
		updated_at: Set(now),
		..Default::default()
	};
	model.insert(db).await.expect("insert tag").id
}

/// Insert a parent edge: `child` implies `parent`.
async fn add_parent(db: &DbConn, child_id: i32, parent_id: i32) {
	let model = tag_parent::ActiveModel {
		child_tag_id: Set(child_id),
		parent_tag_id: Set(parent_id),
		created_at: Set(chrono::Utc::now()),
	};
	model.insert(db).await.expect("insert parent edge");
}

/// Attach a tag to an entry directly (creates metadata + link).
async fn apply_direct_tag(db: &DbConn, entry_uuid: Uuid, tag_id: i32) {
	let now = chrono::Utc::now();
	let metadata = user_metadata::ActiveModel {
		uuid: Set(Uuid::new_v4()),
		entry_uuid: Set(Some(entry_uuid)),
		content_identity_uuid: Set(None),
		favorite: Set(false),
		hidden: Set(false),
		custom_data: Set(serde_json::json!({}).into()),
		created_at: Set(now),
		updated_at: Set(now),
		..Default::default()
	};
	let metadata_id = metadata.insert(db).await.expect("insert metadata").id;

	let link = user_metadata_tag::ActiveModel {
		user_metadata_id: Set(metadata_id),
		tag_id: Set(tag_id),
		confidence: Set(1.0),
		source: Set("user".to_string()),
		inheritance_source: Set(TagInheritanceSource::Direct.as_str().to_string()),
		created_at: Set(now),
		updated_at: Set(now),
		device_uuid: Set(Uuid::new_v4()),
		uuid: Set(Uuid::new_v4()),
		version: Set(1),
		..Default::default()
	};
	link.insert(db).await.expect("insert tag link");
}

fn tag_rule(name: &str) -> Rule {
	Rule {
		name: format!("match {name}"),
		condition: Condition::Tag {
			name: name.to_string(),
		},
		actions: Vec::new(),
	}
}

/// A child file with direct tag `car` (car -> vehicle) matches `Tag { vehicle }`
/// because the parent relation implies `vehicle`, while the direct `car` still
/// matches and an unrelated `boat` does not.
#[tokio::test]
async fn tag_condition_matches_implied_parent() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (folder_id, _folder_uuid) = insert_entry(conn, "garage", None).await;
	let (_child_id, child_uuid) = insert_entry(conn, "mycar.jpg", Some(folder_id)).await;

	let car = insert_tag(conn, "car").await;
	let vehicle = insert_tag(conn, "vehicle").await;
	let _boat = insert_tag(conn, "boat").await;
	add_parent(conn, car, vehicle).await;

	apply_direct_tag(conn, child_uuid, car).await;

	let target = resolve_rule_target(
		conn,
		child_uuid,
		TestEntry {
			path: "garage/mycar.jpg".into(),
		},
	)
	.await
	.expect("resolve target");

	assert!(
		evaluate(&tag_rule("vehicle"), &target),
		"vehicle is implied by direct car -> vehicle parent edge"
	);
	assert!(
		evaluate(&tag_rule("car"), &target),
		"directly-attached car still matches"
	);
	assert!(
		!evaluate(&tag_rule("boat"), &target),
		"unrelated boat must not match"
	);
}

/// A folder tagged `vehicle` is inherited by its child file, so the child
/// matches `Tag { vehicle }` even without a direct or implied `vehicle`.
#[tokio::test]
async fn tag_condition_matches_inherited_folder_tag() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (folder_id, folder_uuid) = insert_entry(conn, "vehicles", None).await;
	let (_child_id, child_uuid) = insert_entry(conn, "photo.jpg", Some(folder_id)).await;

	let vehicle = insert_tag(conn, "vehicle").await;
	apply_direct_tag(conn, folder_uuid, vehicle).await;

	let target = resolve_rule_target(
		conn,
		child_uuid,
		TestEntry {
			path: "vehicles/photo.jpg".into(),
		},
	)
	.await
	.expect("resolve target");

	assert!(
		evaluate(&tag_rule("vehicle"), &target),
		"vehicle is inherited from the parent folder"
	);
	assert!(
		!evaluate(&tag_rule("car"), &target),
		"no car tag anywhere, must not match"
	);
}

/// An entry with no tags resolves to an empty set and matches no tag condition.
#[tokio::test]
async fn tag_condition_no_match_without_tags() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (_id, entry_uuid) = insert_entry(conn, "untagged.txt", None).await;

	let target = resolve_rule_target(
		conn,
		entry_uuid,
		TestEntry {
			path: "untagged.txt".into(),
		},
	)
	.await
	.expect("resolve target");

	assert!(!evaluate(&tag_rule("vehicle"), &target));
	assert!(target.effective_tag_names().is_empty());
}
