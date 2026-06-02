//! Tag inheritance resolution tests (task A-02).
//!
//! Verifies that effective tags resolve correctly up the entry/folder hierarchy:
//! ancestor folder tags are inherited by descendants, direct tags take
//! precedence, and overrides suppress otherwise-inherited tags. Closest level
//! always wins across a multi-level chain.

use sd_core::domain::tag::TagInheritanceSource;
use sd_core::infra::db::entities::{entry, entry_closure, tag, user_metadata, user_metadata_tag};
use sd_core::infra::db::Database;
use sd_core::ops::tags::effective::resolve_effective_tags;
use sea_orm::{ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, QueryFilter, Set};
use tempfile::TempDir;
use uuid::Uuid;

/// Build a migrated, empty database in a temp dir.
async fn fresh_db() -> (TempDir, Database) {
	let temp = TempDir::new().unwrap();
	let db_path = temp.path().join("tag_inheritance.db");
	let db = Database::create(&db_path).await.expect("create db");
	db.migrate().await.expect("migrate");
	(temp, db)
}

/// Insert an entry (folder/file) and return its (db id, uuid).
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

async fn insert_entry_without_parent(db: &DbConn, name: &str) -> (i32, Uuid) {
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
		parent_id: Set(None),
		..Default::default()
	};
	let inserted = model.insert(db).await.expect("insert entry");
	insert_closure(db, inserted.id, inserted.id, 0).await;
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

/// Create entry-scoped user_metadata for an entry and return its id.
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

/// Link a tag to an entry's metadata with a given inheritance source.
async fn link_tag(db: &DbConn, metadata_id: i32, tag_id: i32, source: TagInheritanceSource) {
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

/// Apply a tag to an entry (creates metadata + link). Returns metadata id.
async fn apply_tag(
	db: &DbConn,
	entry_uuid: Uuid,
	tag_id: i32,
	source: TagInheritanceSource,
) -> i32 {
	let metadata_id = insert_metadata(db, entry_uuid).await;
	link_tag(db, metadata_id, tag_id, source).await;
	metadata_id
}

/// (a) A tag on a parent folder is inherited by a child file.
#[tokio::test]
async fn parent_tag_inherited_by_child() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (parent_id, parent_uuid) = insert_entry(conn, "parent", None).await;
	let (_child_id, child_uuid) = insert_entry(conn, "child.txt", Some(parent_id)).await;

	let tag_id = insert_tag(conn, "Important").await;
	apply_tag(conn, parent_uuid, tag_id, TagInheritanceSource::Direct).await;

	let effective = resolve_effective_tags(conn, child_uuid).await.unwrap();

	assert_eq!(effective.len(), 1, "child should inherit one tag");
	let resolved = &effective[0];
	assert_eq!(resolved.tag.canonical_name, "Important");
	assert_eq!(resolved.source, TagInheritanceSource::Inherited);
	assert_eq!(resolved.source_entry_id, Some(parent_uuid));
	assert_eq!(resolved.depth, 1, "tag comes from one level up");
}

#[tokio::test]
async fn direct_only_tag_resolves_as_direct() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (_entry_id, entry_uuid) = insert_entry(conn, "file.txt", None).await;

	let tag_id = insert_tag(conn, "DirectOnly").await;
	apply_tag(conn, entry_uuid, tag_id, TagInheritanceSource::Direct).await;

	let effective = resolve_effective_tags(conn, entry_uuid).await.unwrap();

	assert_eq!(effective.len(), 1);
	assert_eq!(effective[0].tag.canonical_name, "DirectOnly");
	assert_eq!(effective[0].source, TagInheritanceSource::Direct);
	assert_eq!(effective[0].source_entry_id, Some(entry_uuid));
	assert_eq!(effective[0].depth, 0);
}

/// The resolver must use entry_closure, not a recursive parent_id walk.
#[tokio::test]
async fn inherited_tags_resolve_from_closure_table() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (parent_id, parent_uuid) = insert_entry_without_parent(conn, "parent").await;
	let (child_id, child_uuid) = insert_entry_without_parent(conn, "child.txt").await;
	insert_closure(conn, parent_id, child_id, 1).await;

	let tag_id = insert_tag(conn, "ClosureOnly").await;
	apply_tag(conn, parent_uuid, tag_id, TagInheritanceSource::Direct).await;

	let effective = resolve_effective_tags(conn, child_uuid).await.unwrap();

	assert_eq!(effective.len(), 1);
	assert_eq!(effective[0].tag.canonical_name, "ClosureOnly");
	assert_eq!(effective[0].source, TagInheritanceSource::Inherited);
	assert_eq!(effective[0].source_entry_id, Some(parent_uuid));
	assert_eq!(effective[0].depth, 1);
}

/// (b) A direct tag on the child takes precedence over inheritance.
#[tokio::test]
async fn direct_tag_takes_precedence() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (parent_id, parent_uuid) = insert_entry(conn, "parent", None).await;
	let (_child_id, child_uuid) = insert_entry(conn, "child.txt", Some(parent_id)).await;

	let tag_id = insert_tag(conn, "Shared").await;
	// Same tag applied both on the parent and directly on the child.
	apply_tag(conn, parent_uuid, tag_id, TagInheritanceSource::Direct).await;
	apply_tag(conn, child_uuid, tag_id, TagInheritanceSource::Direct).await;

	let effective = resolve_effective_tags(conn, child_uuid).await.unwrap();

	assert_eq!(effective.len(), 1, "tag resolves once, closest wins");
	let resolved = &effective[0];
	assert_eq!(resolved.tag.canonical_name, "Shared");
	assert_eq!(resolved.source, TagInheritanceSource::Direct);
	assert_eq!(resolved.source_entry_id, Some(child_uuid));
	assert_eq!(resolved.depth, 0, "direct tag is on the entry itself");
}

/// (c) An override on the child suppresses the inherited tag.
#[tokio::test]
async fn override_suppresses_inherited_tag() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (parent_id, parent_uuid) = insert_entry(conn, "parent", None).await;
	let (_child_id, child_uuid) = insert_entry(conn, "child.txt", Some(parent_id)).await;

	let tag_id = insert_tag(conn, "Suppressed").await;
	// Parent has the tag; child explicitly overrides (suppresses) it.
	apply_tag(conn, parent_uuid, tag_id, TagInheritanceSource::Direct).await;
	apply_tag(conn, child_uuid, tag_id, TagInheritanceSource::Overridden).await;

	let effective = resolve_effective_tags(conn, child_uuid).await.unwrap();

	assert!(
		effective.is_empty(),
		"override on child must suppress the inherited tag, got {:?}",
		effective
			.iter()
			.map(|e| &e.tag.canonical_name)
			.collect::<Vec<_>>()
	);
	// The parent itself still resolves the tag directly.
	let parent_effective = resolve_effective_tags(conn, parent_uuid).await.unwrap();
	assert_eq!(parent_effective.len(), 1);
	assert_eq!(parent_effective[0].source, TagInheritanceSource::Direct);
}

#[tokio::test]
async fn override_mid_tree_suppresses_farther_ancestor_for_subtree() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (root_id, root_uuid) = insert_entry(conn, "root", None).await;
	let (folder_id, folder_uuid) = insert_entry(conn, "folder", Some(root_id)).await;
	let (_leaf_id, leaf_uuid) = insert_entry(conn, "leaf.txt", Some(folder_id)).await;

	let tag_id = insert_tag(conn, "RootOnly").await;
	apply_tag(conn, root_uuid, tag_id, TagInheritanceSource::Direct).await;
	apply_tag(conn, folder_uuid, tag_id, TagInheritanceSource::Overridden).await;

	let folder_effective = resolve_effective_tags(conn, folder_uuid).await.unwrap();
	let leaf_effective = resolve_effective_tags(conn, leaf_uuid).await.unwrap();
	let root_effective = resolve_effective_tags(conn, root_uuid).await.unwrap();

	assert!(folder_effective.is_empty());
	assert!(leaf_effective.is_empty());
	assert_eq!(root_effective.len(), 1);
	assert_eq!(root_effective[0].source, TagInheritanceSource::Direct);
}

#[tokio::test]
async fn direct_readd_after_mid_tree_override_wins_at_leaf() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (root_id, root_uuid) = insert_entry(conn, "root", None).await;
	let (folder_id, folder_uuid) = insert_entry(conn, "folder", Some(root_id)).await;
	let (_leaf_id, leaf_uuid) = insert_entry(conn, "leaf.txt", Some(folder_id)).await;

	let tag_id = insert_tag(conn, "Readded").await;
	apply_tag(conn, root_uuid, tag_id, TagInheritanceSource::Direct).await;
	apply_tag(conn, folder_uuid, tag_id, TagInheritanceSource::Overridden).await;
	apply_tag(conn, leaf_uuid, tag_id, TagInheritanceSource::Direct).await;

	let effective = resolve_effective_tags(conn, leaf_uuid).await.unwrap();

	assert_eq!(effective.len(), 1);
	assert_eq!(effective[0].tag.canonical_name, "Readded");
	assert_eq!(effective[0].source, TagInheritanceSource::Direct);
	assert_eq!(effective[0].source_entry_id, Some(leaf_uuid));
	assert_eq!(effective[0].depth, 0);
}

/// (d) Multi-level grandparent -> parent -> child: closest level wins.
#[tokio::test]
async fn multi_level_closest_wins() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (gp_id, gp_uuid) = insert_entry(conn, "grandparent", None).await;
	let (parent_id, parent_uuid) = insert_entry(conn, "parent", Some(gp_id)).await;
	let (_child_id, child_uuid) = insert_entry(conn, "child.txt", Some(parent_id)).await;

	// "Color" applied on both grandparent and parent: parent (closer) wins.
	let color = insert_tag(conn, "Color").await;
	apply_tag(conn, gp_uuid, color, TagInheritanceSource::Direct).await;
	apply_tag(conn, parent_uuid, color, TagInheritanceSource::Direct).await;

	// "Project" only on the grandparent: inherited from two levels up.
	let project = insert_tag(conn, "Project").await;
	apply_tag(conn, gp_uuid, project, TagInheritanceSource::Direct).await;

	let mut effective = resolve_effective_tags(conn, child_uuid).await.unwrap();
	effective.sort_by(|a, b| a.tag.canonical_name.cmp(&b.tag.canonical_name));

	assert_eq!(effective.len(), 2, "two distinct effective tags");

	let color_tag = &effective[0];
	assert_eq!(color_tag.tag.canonical_name, "Color");
	assert_eq!(color_tag.source, TagInheritanceSource::Inherited);
	assert_eq!(
		color_tag.source_entry_id,
		Some(parent_uuid),
		"closer parent wins over grandparent"
	);
	assert_eq!(color_tag.depth, 1);

	let project_tag = &effective[1];
	assert_eq!(project_tag.tag.canonical_name, "Project");
	assert_eq!(project_tag.source, TagInheritanceSource::Inherited);
	assert_eq!(project_tag.source_entry_id, Some(gp_uuid));
	assert_eq!(project_tag.depth, 2, "inherited two levels up");
}

#[tokio::test]
async fn six_level_deep_file_inherits_root_tag() {
	let (_temp, db) = fresh_db().await;
	let conn = db.conn();

	let (root_id, root_uuid) = insert_entry(conn, "level-0", None).await;
	let mut parent_id = root_id;
	let mut leaf_uuid = root_uuid;
	for depth in 1..=6 {
		let (entry_id, entry_uuid) =
			insert_entry(conn, &format!("level-{}", depth), Some(parent_id)).await;
		parent_id = entry_id;
		leaf_uuid = entry_uuid;
	}

	let tag_id = insert_tag(conn, "DeepRoot").await;
	apply_tag(conn, root_uuid, tag_id, TagInheritanceSource::Direct).await;

	let effective = resolve_effective_tags(conn, leaf_uuid).await.unwrap();

	assert_eq!(effective.len(), 1);
	assert_eq!(effective[0].tag.canonical_name, "DeepRoot");
	assert_eq!(effective[0].source, TagInheritanceSource::Inherited);
	assert_eq!(effective[0].source_entry_id, Some(root_uuid));
	assert_eq!(effective[0].depth, 6);
}
