//! Tag override / restore action tests (task A-03, write side).
//!
//! Proves the full round-trip against A-02 resolution: a parent's direct tag is
//! inherited by a child; the override action suppresses it; the remove-override
//! action restores inheritance; and a direct application takes precedence.

use sd_core::domain::tag::TagInheritanceSource;
use sd_core::infra::action::LibraryAction;
use sd_core::infra::db::entities::{entry, entry_closure, tag, user_metadata, user_metadata_tag};
use sd_core::ops::tags::apply::{action::ApplyTagsAction, input::ApplyTagsInput};
use sd_core::ops::tags::effective::resolve_effective_tags;
use sd_core::ops::tags::overrides::{
	action::{OverrideTagAction, RemoveTagOverrideAction},
	input::{OverrideTagInput, RemoveTagOverrideInput},
};
use sd_core::Core;
use sea_orm::{ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, QueryFilter, Set};
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

/// Insert an entry (folder/file), wiring up its closure rows. Returns (db id, uuid).
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

/// Insert a tag, returning its (db id, uuid).
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
	let inserted = model.insert(db).await.expect("insert tag");
	(inserted.id, uuid)
}

/// Create entry-scoped user_metadata and return its id.
async fn insert_metadata(db: &DbConn, entry_uuid: Uuid) -> i32 {
	let now = chrono::Utc::now();
	let model = user_metadata::ActiveModel {
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
	model.insert(db).await.expect("insert metadata").id
}

/// Directly apply a tag to an entry with a given inheritance source (raw row).
async fn apply_tag(db: &DbConn, entry_uuid: Uuid, tag_id: i32, source: TagInheritanceSource) {
	let metadata_id = insert_metadata(db, entry_uuid).await;
	let now = chrono::Utc::now();
	let model = user_metadata_tag::ActiveModel {
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
	};
	model.insert(db).await.expect("insert tag link");
}

#[tokio::test]
async fn override_round_trip_with_a02_resolution() {
	// Build a real Core + library so we can dispatch library actions.
	let temp_data = TempDir::new().unwrap();
	let data_dir = temp_data.path().join("core_data");
	std::fs::create_dir_all(&data_dir).unwrap();

	let core = Arc::new(Core::new(data_dir.clone()).await.unwrap());
	let library = core
		.libraries
		.create_library("Tag Override Test Library", None, core.context.clone())
		.await
		.unwrap();
	let library_id = library.id();
	let action_manager = core.context.get_action_manager().await.unwrap();

	let conn = library.db().conn();

	// parent/ -> child.txt ; tag T applied directly on the parent.
	let (parent_id, parent_uuid) = insert_entry(conn, "parent", None).await;
	let (child_id, child_uuid) = insert_entry(conn, "child.txt", Some(parent_id)).await;
	let (tag_db_id, tag_uuid) = insert_tag(conn, "Important").await;
	apply_tag(conn, parent_uuid, tag_db_id, TagInheritanceSource::Direct).await;

	// (a) Child inherits T from the parent.
	let effective = resolve_effective_tags(conn, child_uuid).await.unwrap();
	assert_eq!(effective.len(), 1, "child should inherit one tag");
	assert_eq!(effective[0].tag.canonical_name, "Important");
	assert_eq!(effective[0].source, TagInheritanceSource::Inherited);
	assert_eq!(effective[0].source_entry_id, Some(parent_uuid));

	// (b) Override T on the child: it should disappear from the effective set.
	let override_action = OverrideTagAction::from_input(OverrideTagInput {
		entry_id: child_uuid,
		tag_id: tag_uuid,
		source_ancestor_id: Some(parent_uuid),
	})
	.unwrap();
	let override_out = action_manager
		.dispatch_library(Some(library_id), override_action)
		.await
		.unwrap();
	assert_eq!(override_out.overridden_from, Some(parent_uuid));

	let effective = resolve_effective_tags(conn, child_uuid).await.unwrap();
	assert!(
		effective.is_empty(),
		"override must suppress the inherited tag, got {:?}",
		effective
			.iter()
			.map(|e| &e.tag.canonical_name)
			.collect::<Vec<_>>()
	);

	// Sanity: the suppression row is stored as "overridden" with provenance.
	let override_rows = user_metadata_tag::Entity::find()
		.filter(user_metadata_tag::Column::TagId.eq(tag_db_id))
		.filter(user_metadata_tag::Column::InheritanceSource.eq("overridden"))
		.all(conn)
		.await
		.unwrap();
	assert_eq!(override_rows.len(), 1, "exactly one override row");
	assert_eq!(override_rows[0].overridden_from_entry_id, Some(parent_id));

	// (c) Remove the override: the child inherits T again.
	let remove_action = RemoveTagOverrideAction::from_input(RemoveTagOverrideInput {
		entry_id: child_uuid,
		tag_id: tag_uuid,
	})
	.unwrap();
	let remove_out = action_manager
		.dispatch_library(Some(library_id), remove_action)
		.await
		.unwrap();
	assert_eq!(remove_out.overrides_removed, 1, "one override removed");

	let effective = resolve_effective_tags(conn, child_uuid).await.unwrap();
	assert_eq!(effective.len(), 1, "inheritance restored");
	assert_eq!(effective[0].source, TagInheritanceSource::Inherited);
	assert_eq!(effective[0].source_entry_id, Some(parent_uuid));

	// (d) Apply T directly on the child: direct wins over inherited.
	let apply_action = ApplyTagsAction::from_input(ApplyTagsInput::user_tags_entry(
		vec![child_id],
		vec![tag_uuid],
	))
	.unwrap();
	action_manager
		.dispatch_library(Some(library_id), apply_action)
		.await
		.unwrap();

	let effective = resolve_effective_tags(conn, child_uuid).await.unwrap();
	assert_eq!(effective.len(), 1, "tag resolves once, closest wins");
	assert_eq!(effective[0].tag.canonical_name, "Important");
	assert_eq!(effective[0].source, TagInheritanceSource::Direct);
	assert_eq!(effective[0].source_entry_id, Some(child_uuid));
	assert_eq!(effective[0].depth, 0);
}
