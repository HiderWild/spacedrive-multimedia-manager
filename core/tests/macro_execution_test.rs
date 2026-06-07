//! E-02 macro execution: dry-run preview, real run, per-item skip, resumability.
//!
//! Drives the rule executor end to end against a throwaway library database. A
//! test [`MacroDispatcher`] applies `tags.apply` actions straight to the DB (the
//! same rows the production path writes) and rejects everything else, so we can
//! assert dispatch outcomes without standing up a full `Library`/`CoreContext`.
//!
//! Coverage:
//! - dry-run reports the planned actions for every matched file and mutates nothing;
//! - a real run performs the actions (the tag becomes effective on the entry);
//! - a failing action on a file is logged and skipped without aborting the batch.

use async_trait::async_trait;
use sd_core::infra::db::entities::{entry, entry_closure, tag, user_metadata, user_metadata_tag};
use sd_core::infra::db::Database;
use sd_core::ops::rules::{run_macro, ActionRef, Condition, MacroDispatcher, Rule};
use sd_core::ops::tags::effective::resolve_effective_tags;
use sea_orm::{ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, QueryFilter, Set};
use serde_json::{json, Value};
use tempfile::TempDir;
use uuid::Uuid;

async fn fresh_db() -> (TempDir, Database) {
	let temp = TempDir::new().unwrap();
	let db_path = temp.path().join("macro_execution.db");
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

/// Insert a file entry (kind 0) with an extension, wiring its closure rows.
async fn insert_file(
	db: &DbConn,
	name: &str,
	extension: Option<&str>,
	parent_id: Option<i32>,
) -> (i32, Uuid) {
	let uuid = Uuid::new_v4();
	let now = chrono::Utc::now();
	let model = entry::ActiveModel {
		uuid: Set(Some(uuid)),
		name: Set(name.to_string()),
		kind: Set(0),
		extension: Set(extension.map(|e| e.to_string())),
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

/// Insert a directory entry (kind 1) wiring its self-closure row.
async fn insert_dir(db: &DbConn, name: &str, parent_id: Option<i32>) -> (i32, Uuid) {
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
	let inserted = model.insert(db).await.expect("insert dir");
	insert_closure(db, inserted.id, inserted.id, 0).await;
	(inserted.id, uuid)
}

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

/// Attach a tag to an entry directly (creates metadata + link), mirroring the
/// rows the production tag path writes.
async fn apply_tag_row(db: &DbConn, entry_uuid: Uuid, tag_id: i32) {
	let now = chrono::Utc::now();
	let metadata = user_metadata::ActiveModel {
		uuid: Set(Uuid::new_v4()),
		entry_uuid: Set(Some(entry_uuid)),
		content_identity_uuid: Set(None),
		favorite: Set(false),
		hidden: Set(false),
		custom_data: Set(json!({}).into()),
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
		inheritance_source: Set("Direct".to_string()),
		created_at: Set(now),
		updated_at: Set(now),
		device_uuid: Set(Uuid::new_v4()),
		uuid: Set(Uuid::new_v4()),
		version: Set(1),
		..Default::default()
	};
	link.insert(db).await.expect("insert tag link");
}

/// Test dispatcher: applies `tags.apply` straight to the DB, rejects the rest.
struct DbTagDispatcher {
	conn: DbConn,
}

#[async_trait]
impl MacroDispatcher for DbTagDispatcher {
	async fn dispatch(&self, action: &str, entry_uuid: Uuid, params: &Value) -> Result<(), String> {
		if action != "tags.apply" {
			return Err(format!("unsupported action: {action}"));
		}
		let names: Vec<String> = params
			.get("tags")
			.and_then(Value::as_array)
			.map(|a| {
				a.iter()
					.filter_map(|v| v.as_str().map(String::from))
					.collect()
			})
			.unwrap_or_default();
		for name in names {
			let found = tag::Entity::find()
				.filter(tag::Column::CanonicalName.eq(name.clone()))
				.one(&self.conn)
				.await
				.map_err(|e| e.to_string())?
				.ok_or_else(|| format!("tag not found: {name}"))?;
			apply_tag_row(&self.conn, entry_uuid, found.id).await;
		}
		Ok(())
	}
}

fn tag_video_rule() -> Rule {
	Rule {
		name: "tag mp4 as video".to_string(),
		condition: Condition::Extension {
			value: "mp4".to_string(),
		},
		actions: vec![ActionRef {
			action: "tags.apply".to_string(),
			params: json!({ "tags": ["video"] }),
		}],
	}
}

async fn has_effective_tag(conn: &DbConn, entry_uuid: Uuid, name: &str) -> bool {
	resolve_effective_tags(conn, entry_uuid)
		.await
		.expect("resolve effective tags")
		.iter()
		.any(|t| t.tag.canonical_name == name)
}

/// Dry-run lists the planned action for every matched file and mutates nothing.
#[tokio::test]
async fn dry_run_reports_plan_without_mutating() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (folder_id, _) = insert_dir(conn, "clips", None).await;
	let (_, a_uuid) = insert_file(conn, "a.mp4", Some("mp4"), Some(folder_id)).await;
	let (_, b_uuid) = insert_file(conn, "b.mp4", Some("mp4"), Some(folder_id)).await;
	let (_, c_uuid) = insert_file(conn, "c.txt", Some("txt"), Some(folder_id)).await;
	let _ = insert_tag(conn, "video").await;

	let dispatcher = DbTagDispatcher { conn: conn.clone() };
	let result = run_macro(conn, &dispatcher, &[tag_video_rule()], true)
		.await
		.expect("run dry-run");

	assert!(result.dry_run, "result should be flagged dry-run");
	assert_eq!(result.matched_files, 2, "only the two mp4 files match");
	assert_eq!(
		result.planned.len(),
		2,
		"one planned action per matched file"
	);
	assert!(result.planned.iter().all(|p| p.action == "tags.apply"));
	assert_eq!(result.succeeded, 0, "dry-run performs nothing");
	assert_eq!(result.failed, 0);

	// Nothing was mutated.
	assert!(!has_effective_tag(conn, a_uuid, "video").await);
	assert!(!has_effective_tag(conn, b_uuid, "video").await);
	assert!(!has_effective_tag(conn, c_uuid, "video").await);
}

/// A real run performs the actions; the tag becomes effective on matched files.
#[tokio::test]
async fn real_run_applies_actions() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (folder_id, _) = insert_dir(conn, "clips", None).await;
	let (_, a_uuid) = insert_file(conn, "a.mp4", Some("mp4"), Some(folder_id)).await;
	let (_, b_uuid) = insert_file(conn, "b.mp4", Some("mp4"), Some(folder_id)).await;
	let (_, c_uuid) = insert_file(conn, "c.txt", Some("txt"), Some(folder_id)).await;
	let _ = insert_tag(conn, "video").await;

	let dispatcher = DbTagDispatcher { conn: conn.clone() };
	let result = run_macro(conn, &dispatcher, &[tag_video_rule()], false)
		.await
		.expect("run real");

	assert!(!result.dry_run);
	assert_eq!(result.matched_files, 2);
	assert_eq!(result.succeeded, 2, "tag applied to both mp4 files");
	assert_eq!(result.failed, 0);
	assert!(result.planned.is_empty(), "real run records no plan items");

	assert!(has_effective_tag(conn, a_uuid, "video").await);
	assert!(has_effective_tag(conn, b_uuid, "video").await);
	assert!(!has_effective_tag(conn, c_uuid, "video").await);
}

/// A failing action is logged and skipped; sibling actions and other files in
/// the batch still run to completion.
#[tokio::test]
async fn failing_action_is_logged_and_skipped() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (folder_id, _) = insert_dir(conn, "clips", None).await;
	let (_, a_uuid) = insert_file(conn, "a.mp4", Some("mp4"), Some(folder_id)).await;
	let (_, b_uuid) = insert_file(conn, "b.mp4", Some("mp4"), Some(folder_id)).await;
	let _ = insert_tag(conn, "video").await;

	let rule = Rule {
		name: "tag then a bogus op".to_string(),
		condition: Condition::Extension {
			value: "mp4".to_string(),
		},
		actions: vec![
			ActionRef {
				action: "tags.apply".to_string(),
				params: json!({ "tags": ["video"] }),
			},
			ActionRef {
				action: "bogus.op".to_string(),
				params: json!({}),
			},
		],
	};

	let dispatcher = DbTagDispatcher { conn: conn.clone() };
	let result = run_macro(conn, &dispatcher, &[rule], false)
		.await
		.expect("run real");

	assert_eq!(result.matched_files, 2);
	// One success (tags.apply) and one failure (bogus.op) per matched file.
	assert_eq!(result.succeeded, 2, "tags.apply succeeded on both files");
	assert_eq!(result.failed, 2, "bogus.op failed on both files");
	assert_eq!(result.failures.len(), 2, "each failure is logged");

	// The failing action did not abort the batch: the tag is applied to both.
	assert!(has_effective_tag(conn, a_uuid, "video").await);
	assert!(has_effective_tag(conn, b_uuid, "video").await);
}
