//! Effective-tag cache tests (task A-05).
//!
//! Proves the cache layered on A-02 inheritance resolution: first lookup
//! computes and matches the direct resolver, a second lookup serves from cache
//! (demonstrated via the hit counter and a stale-read-then-invalidate cycle),
//! `invalidate_subtree` drops a folder and its descendants, `invalidate_all`
//! clears everything, and an A-03 override write invalidates the affected
//! subtree so the child recomputes to the suppressed set.

use sd_core::domain::tag::TagInheritanceSource;
use sd_core::infra::action::LibraryAction;
use sd_core::infra::db::entities::{entry, entry_closure, tag, user_metadata, user_metadata_tag};
use sd_core::ops::tags::effective::resolve_effective_tags;
use sd_core::ops::tags::overrides::{action::OverrideTagAction, input::OverrideTagInput};
use sd_core::Core;
use sea_orm::{ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, QueryFilter, Set};
use std::collections::HashSet;
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

/// Set of canonical tag names from an effective tag slice.
fn names(tags: &[sd_core::ops::tags::effective::EffectiveTag]) -> HashSet<String> {
	tags.iter().map(|t| t.tag.canonical_name.clone()).collect()
}

async fn make_core() -> (Arc<Core>, TempDir) {
	let temp_data = TempDir::new().unwrap();
	let data_dir = temp_data.path().join("core_data");
	std::fs::create_dir_all(&data_dir).unwrap();
	let core = Arc::new(Core::new(data_dir).await.unwrap());
	(core, temp_data)
}

/// (a) First get computes and matches the A-02 resolver; second get is a cache
/// hit (hit counter increments). Mutating the DB without invalidating returns
/// the stale cached value; invalidating then returns the fresh value.
#[tokio::test]
async fn cache_hit_and_stale_then_invalidate() {
	let (core, _tmp) = make_core().await;
	let library = core
		.libraries
		.create_library("A05 Cache Hit", None, core.context.clone())
		.await
		.unwrap();
	let conn = library.db().conn();
	let cache = library.tag_cache();

	let (parent_id, parent_uuid) = insert_entry(conn, "parent", None).await;
	let (child_id, child_uuid) = insert_entry(conn, "child.txt", Some(parent_id)).await;
	let _ = child_id;
	let (tag_a, _tag_a_uuid) = insert_tag(conn, "Important").await;
	apply_tag(conn, parent_uuid, tag_a, TagInheritanceSource::Direct).await;

	// First get: a miss that matches the direct resolver exactly.
	let direct = resolve_effective_tags(conn, child_uuid).await.unwrap();
	let cached = cache.get_or_compute(conn, child_uuid).await.unwrap();
	assert_eq!(names(&direct), names(&cached), "first get must match A-02");
	assert_eq!(cache.misses(), 1, "first lookup is a miss");
	assert_eq!(cache.hits(), 0);

	// Second get: served from cache.
	let again = cache.get_or_compute(conn, child_uuid).await.unwrap();
	assert_eq!(names(&again), names(&cached));
	assert_eq!(cache.hits(), 1, "second lookup is a hit");
	assert_eq!(cache.misses(), 1, "no recompute on hit");

	// Mutate the DB underneath WITHOUT invalidating: cache stays stale.
	let (tag_b, _tag_b_uuid) = insert_tag(conn, "Archive").await;
	apply_tag(conn, parent_uuid, tag_b, TagInheritanceSource::Direct).await;
	let stale = cache.get_or_compute(conn, child_uuid).await.unwrap();
	assert_eq!(
		names(&stale),
		names(&cached),
		"without invalidation the stale single-tag set is returned"
	);
	assert_eq!(stale.len(), 1);

	// Invalidate the parent's subtree (covers the child) and recompute fresh.
	cache.invalidate_subtree(conn, parent_uuid).await.unwrap();
	let fresh = cache.get_or_compute(conn, child_uuid).await.unwrap();
	let fresh_names = names(&fresh);
	assert!(fresh_names.contains("Important"));
	assert!(
		fresh_names.contains("Archive"),
		"after invalidation the new tag appears: {:?}",
		fresh_names
	);
	assert_eq!(fresh.len(), 2);
}

/// (b) invalidate_subtree(parent) invalidates the child specifically: changing a
/// parent folder's tag then invalidating the subtree makes the child recompute.
#[tokio::test]
async fn invalidate_subtree_invalidates_child() {
	let (core, _tmp) = make_core().await;
	let library = core
		.libraries
		.create_library("A05 Subtree", None, core.context.clone())
		.await
		.unwrap();
	let conn = library.db().conn();
	let cache = library.tag_cache();

	let (grand_id, grand_uuid) = insert_entry(conn, "grand", None).await;
	let (parent_id, parent_uuid) = insert_entry(conn, "parent", Some(grand_id)).await;
	let (_child_id, child_uuid) = insert_entry(conn, "child.txt", Some(parent_id)).await;
	let (tag_a, _) = insert_tag(conn, "Root").await;
	apply_tag(conn, grand_uuid, tag_a, TagInheritanceSource::Direct).await;

	// Prime the child cache (inherits Root from grandparent).
	let primed = cache.get_or_compute(conn, child_uuid).await.unwrap();
	assert_eq!(names(&primed), HashSet::from(["Root".to_string()]));

	// Add a tag on the parent folder and invalidate from the grandparent.
	let (tag_b, _) = insert_tag(conn, "Project").await;
	apply_tag(conn, parent_uuid, tag_b, TagInheritanceSource::Direct).await;
	cache.invalidate_subtree(conn, grand_uuid).await.unwrap();

	// Child recomputes to include the parent's new tag.
	let recomputed = cache.get_or_compute(conn, child_uuid).await.unwrap();
	assert_eq!(
		names(&recomputed),
		HashSet::from(["Root".to_string(), "Project".to_string()]),
		"child must recompute after subtree invalidation"
	);
}

/// (c) invalidate_all clears every cached entry.
#[tokio::test]
async fn invalidate_all_clears_everything() {
	let (core, _tmp) = make_core().await;
	let library = core
		.libraries
		.create_library("A05 All", None, core.context.clone())
		.await
		.unwrap();
	let conn = library.db().conn();
	let cache = library.tag_cache();

	let (p_id, p_uuid) = insert_entry(conn, "p", None).await;
	let (_c1_id, c1_uuid) = insert_entry(conn, "c1", Some(p_id)).await;
	let (_c2_id, c2_uuid) = insert_entry(conn, "c2", Some(p_id)).await;
	let (tag_a, _) = insert_tag(conn, "Shared").await;
	apply_tag(conn, p_uuid, tag_a, TagInheritanceSource::Direct).await;

	cache.get_or_compute(conn, p_uuid).await.unwrap();
	cache.get_or_compute(conn, c1_uuid).await.unwrap();
	cache.get_or_compute(conn, c2_uuid).await.unwrap();
	assert_eq!(cache.len().await, 3, "three entries cached");

	cache.invalidate_all().await;
	assert!(cache.is_empty().await, "cache must be empty after clear");
}

/// (d) An A-03 override write invalidates the subtree, so the child's effective
/// set changes (the inherited tag is suppressed) on the next lookup.
#[tokio::test]
async fn override_write_invalidates_via_cache() {
	let (core, _tmp) = make_core().await;
	let library = core
		.libraries
		.create_library("A05 Override", None, core.context.clone())
		.await
		.unwrap();
	let library_id = library.id();
	let action_manager = core.context.get_action_manager().await.unwrap();
	let conn = library.db().conn();
	let cache = library.tag_cache();

	let (parent_id, parent_uuid) = insert_entry(conn, "parent", None).await;
	let (_child_id, child_uuid) = insert_entry(conn, "child.txt", Some(parent_id)).await;
	let (tag_db, tag_uuid) = insert_tag(conn, "Important").await;
	apply_tag(conn, parent_uuid, tag_db, TagInheritanceSource::Direct).await;

	// Prime: child inherits the parent's tag.
	let before = cache.get_or_compute(conn, child_uuid).await.unwrap();
	assert_eq!(names(&before), HashSet::from(["Important".to_string()]));

	// Dispatch the A-03 override on the child; the action invalidates the subtree.
	let override_action = OverrideTagAction::from_input(OverrideTagInput {
		entry_id: child_uuid,
		tag_id: tag_uuid,
		source_ancestor_id: Some(parent_uuid),
	})
	.unwrap();
	action_manager
		.dispatch_library(Some(library_id), override_action)
		.await
		.unwrap();

	// The cache was invalidated by the action, so the child recomputes to empty.
	let after = cache.get_or_compute(conn, child_uuid).await.unwrap();
	assert!(
		after.is_empty(),
		"override must drop the inherited tag from the child, got {:?}",
		names(&after)
	);
}
