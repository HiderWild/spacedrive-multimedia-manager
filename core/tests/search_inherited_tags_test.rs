//! Search integration tests for inherited tag matching (task A-06).
//!
//! The search tag filter optionally matches files that inherit a tag from an
//! ancestor folder, not just files the tag is applied to directly. These tests
//! pin the three required behaviours:
//! (a) `include_inherited = false` matches only directly-tagged entries,
//! (b) `include_inherited = true` also matches inheriting descendants,
//! (c) a descendant that overrides/clears the tag is excluded from inherited
//!     matches (and the exclusion propagates to its subtree).

use sd_core::domain::tag::TagInheritanceSource;
use sd_core::infra::db::entities::{entry, entry_closure, tag, user_metadata, user_metadata_tag};
use sd_core::infra::db::Database;
use sd_core::ops::search::inherited::find_entry_ids_for_tag_with_inheritance;
use sea_orm::{ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, QueryFilter, Set};
use std::collections::HashSet;
use tempfile::TempDir;
use uuid::Uuid;

/// Build a migrated, empty database in a temp dir.
async fn fresh_db() -> (TempDir, Database) {
	let temp = TempDir::new().unwrap();
	let db_path = temp.path().join("search_inherited_tags.db");
	let db = Database::create(&db_path).await.expect("create db");
	db.migrate().await.expect("migrate");
	(temp, db)
}

/// Insert an entry (folder/file), maintaining the closure table, returning (db id, uuid).
async fn insert_entry(db: &DbConn, name: &str, parent_id: Option<i32>) -> (i32, Uuid) {
	let uuid = Uuid::new_v4();
	let now = chrono::Utc::now();
	let model = entry::ActiveModel {
		uuid: Set(Some(uuid)),
		name: Set(name.to_string()),
		kind: Set(1), // Directory
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

async fn insert_closure(db: &DbConn, ancestor_id: i32, descendant_id: i32, depth: i32) {
	let closure = entry_closure::ActiveModel {
		ancestor_id: Set(ancestor_id),
		descendant_id: Set(descendant_id),
		depth: Set(depth),
	};
	closure.insert(db).await.expect("insert entry closure");
}

/// Insert a tag and return its (db id, uuid).
async fn insert_tag(db: &DbConn, name: &str) -> (i32, Uuid) {
	let uuid = Uuid::new_v4();
	let now = chrono::Utc::now();
	let model = tag::ActiveModel {
		uuid: Set(uuid),
		canonical_name: Set(name.to_string()),
		tag_type: Set("standard".to_string()),
		is_organizational_anchor: Set(false),
		privacy_level: Set("normal".to_string()),
		search_weight: Set(100),
		created_at: Set(now),
		updated_at: Set(now),
		..Default::default()
	};
	let id = model.insert(db).await.expect("insert tag").id;
	(id, uuid)
}

/// Apply a tag to an entry (creates entry-scoped metadata + junction row).
async fn apply_tag(db: &DbConn, entry_uuid: Uuid, tag_id: i32, source: TagInheritanceSource) {
	let now = chrono::Utc::now();
	let metadata_id = user_metadata::ActiveModel {
		uuid: Set(Uuid::new_v4()),
		entry_uuid: Set(Some(entry_uuid)),
		content_identity_uuid: Set(None),
		favorite: Set(false),
		hidden: Set(false),
		custom_data: Set(serde_json::json!({}).into()),
		created_at: Set(now),
		updated_at: Set(now),
		..Default::default()
	}
	.insert(db)
	.await
	.expect("insert metadata")
	.id;

	user_metadata_tag::ActiveModel {
		user_metadata_id: Set(metadata_id),
		tag_id: Set(tag_id),
		confidence: Set(1.0),
		source: Set("user".to_string()),
		inheritance_source: Set(source.as_str().to_string()),
		created_at: Set(now),
		updated_at: Set(now),
		device_uuid: Set(Uuid::new_v4()),
		uuid: Set(Uuid::new_v4()),
		version: Set(1),
		..Default::default()
	}
	.insert(db)
	.await
	.expect("insert tag link");
}

async fn matches(db: &DbConn, tag_uuid: Uuid, include_inherited: bool) -> HashSet<i32> {
	find_entry_ids_for_tag_with_inheritance(db, tag_uuid, include_inherited)
		.await
		.expect("resolve inherited tag matches")
		.into_iter()
		.collect()
}

/// (a) Direct-only matching returns just the tagged folder, not its children.
#[tokio::test]
async fn direct_only_matches_tagged_entry() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (folder_id, folder_uuid) = insert_entry(conn, "folder", None).await;
	let (child_id, _child_uuid) = insert_entry(conn, "child.txt", Some(folder_id)).await;

	let (tag_id, tag_uuid) = insert_tag(conn, "Project").await;
	apply_tag(conn, folder_uuid, tag_id, TagInheritanceSource::Direct).await;

	let direct = matches(conn, tag_uuid, false).await;
	assert_eq!(
		direct,
		HashSet::from([folder_id]),
		"direct-only must match only the tagged folder"
	);
	assert!(
		!direct.contains(&child_id),
		"child must not match under direct-only"
	);
}

/// (b) Inherited matching returns the tagged folder plus its inheriting children.
#[tokio::test]
async fn inherited_matches_descendants() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (folder_id, folder_uuid) = insert_entry(conn, "folder", None).await;
	let (c1_id, _c1) = insert_entry(conn, "a.txt", Some(folder_id)).await;
	let (c2_id, _c2) = insert_entry(conn, "b.txt", Some(folder_id)).await;
	// Grandchild under a subfolder inherits transitively.
	let (sub_id, _sub) = insert_entry(conn, "sub", Some(folder_id)).await;
	let (g_id, _g) = insert_entry(conn, "deep.txt", Some(sub_id)).await;

	// Unrelated file with a different tag must never match.
	let (other_id, other_uuid) = insert_entry(conn, "other.txt", None).await;
	let (other_tag, _) = insert_tag(conn, "Unrelated").await;
	apply_tag(conn, other_uuid, other_tag, TagInheritanceSource::Direct).await;

	let (tag_id, tag_uuid) = insert_tag(conn, "Project").await;
	apply_tag(conn, folder_uuid, tag_id, TagInheritanceSource::Direct).await;

	let inherited = matches(conn, tag_uuid, true).await;
	assert_eq!(
		inherited,
		HashSet::from([folder_id, c1_id, c2_id, sub_id, g_id]),
		"inherited must match the folder and all descendants"
	);
	assert!(
		!inherited.contains(&other_id),
		"unrelated file must not match"
	);
}

/// (c) An override on a child excludes it (and its subtree) from inherited matches.
#[tokio::test]
async fn override_excludes_descendant_from_inherited() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (folder_id, folder_uuid) = insert_entry(conn, "folder", None).await;
	let (kept_id, _kept) = insert_entry(conn, "kept.txt", Some(folder_id)).await;
	let (over_id, over_uuid) = insert_entry(conn, "cleared", Some(folder_id)).await;
	// A file beneath the overriding folder must also be excluded.
	let (over_child_id, _occ) = insert_entry(conn, "under_cleared.txt", Some(over_id)).await;

	let (tag_id, tag_uuid) = insert_tag(conn, "Project").await;
	apply_tag(conn, folder_uuid, tag_id, TagInheritanceSource::Direct).await;
	apply_tag(conn, over_uuid, tag_id, TagInheritanceSource::Overridden).await;

	// Direct-only is unaffected by inheritance/overrides: only the folder.
	let direct = matches(conn, tag_uuid, false).await;
	assert_eq!(direct, HashSet::from([folder_id]));

	let inherited = matches(conn, tag_uuid, true).await;
	assert_eq!(
		inherited,
		HashSet::from([folder_id, kept_id]),
		"inherited must include the kept child but exclude the override and its subtree"
	);
	assert!(
		!inherited.contains(&over_id),
		"overriding entry must be excluded from inherited matches"
	);
	assert!(
		!inherited.contains(&over_child_id),
		"subtree under an override must be excluded"
	);
}

/// A direct re-application below an override wins again at that entry.
#[tokio::test]
async fn direct_readd_below_override_matches() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (root_id, root_uuid) = insert_entry(conn, "root", None).await;
	let (folder_id, folder_uuid) = insert_entry(conn, "folder", Some(root_id)).await;
	let (leaf_id, leaf_uuid) = insert_entry(conn, "leaf.txt", Some(folder_id)).await;

	let (tag_id, tag_uuid) = insert_tag(conn, "Project").await;
	apply_tag(conn, root_uuid, tag_id, TagInheritanceSource::Direct).await;
	apply_tag(conn, folder_uuid, tag_id, TagInheritanceSource::Overridden).await;
	apply_tag(conn, leaf_uuid, tag_id, TagInheritanceSource::Direct).await;

	let inherited = matches(conn, tag_uuid, true).await;
	assert_eq!(
		inherited,
		HashSet::from([root_id, leaf_id]),
		"root (direct) and leaf (re-added direct) match; the overriding folder does not"
	);
	assert!(
		!inherited.contains(&folder_id),
		"the overriding folder itself must not match"
	);
}
