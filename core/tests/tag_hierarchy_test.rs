//! Tag hierarchy resolution tests (task A-04).
//!
//! Verifies relationships *between tags*: parent edges make one tag imply
//! another transitively, sibling edges canonicalize aliases to an ideal tag,
//! and the resolver expands an applied set into its full implied form while
//! staying loop-safe. Also covers cycle/self-loop rejection and the migration
//! up/down round-trip for the two new tables.

use sd_core::infra::db::entities::{tag, tag_parent, tag_sibling};
use sd_core::infra::db::migration::Migrator;
use sd_core::infra::db::Database;
use sd_core::ops::tags::relations::input::{AddParentTagInput, AddSiblingTagInput};
use sd_core::ops::tags::relations::resolver::{resolve_implied_tags, would_create_cycle};
use sea_orm::{ActiveModelTrait, DbConn, Set};
use sea_orm_migration::MigratorTrait;
use tempfile::TempDir;
use uuid::Uuid;

/// Build a migrated, empty database in a temp dir.
async fn fresh_db() -> (TempDir, Database) {
	let temp = TempDir::new().unwrap();
	let db_path = temp.path().join("tag_hierarchy.db");
	let db = Database::create(&db_path).await.expect("create db");
	db.migrate().await.expect("migrate");
	(temp, db)
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

/// Insert a parent edge: `child` implies `parent`.
async fn add_parent(db: &DbConn, child_id: i32, parent_id: i32) {
	let model = tag_parent::ActiveModel {
		child_tag_id: Set(child_id),
		parent_tag_id: Set(parent_id),
		created_at: Set(chrono::Utc::now()),
	};
	model.insert(db).await.expect("insert parent edge");
}

/// Insert a sibling edge: `tag` is an alias of canonical `ideal`.
async fn add_sibling(db: &DbConn, tag_id: i32, ideal_id: i32) {
	let model = tag_sibling::ActiveModel {
		tag_id: Set(tag_id),
		ideal_tag_id: Set(ideal_id),
		created_at: Set(chrono::Utc::now()),
	};
	model.insert(db).await.expect("insert sibling edge");
}

/// Collect canonical names from a resolved tag set for easy assertions.
fn names(tags: &[sd_core::domain::tag::Tag]) -> Vec<String> {
	let mut out: Vec<String> = tags.iter().map(|t| t.canonical_name.clone()).collect();
	out.sort();
	out
}

#[tokio::test]
async fn parent_implication_expands_applied_tag() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (car, car_uuid) = insert_tag(conn, "car").await;
	let (vehicle, _) = insert_tag(conn, "vehicle").await;
	add_parent(conn, car, vehicle).await;

	let resolved = resolve_implied_tags(conn, &[car_uuid])
		.await
		.expect("resolve implied");
	assert_eq!(names(&resolved), vec!["car", "vehicle"]);
}

#[tokio::test]
async fn parent_implication_is_transitive() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (car, car_uuid) = insert_tag(conn, "car").await;
	let (vehicle, _) = insert_tag(conn, "vehicle").await;
	let (object, _) = insert_tag(conn, "object").await;
	add_parent(conn, car, vehicle).await;
	add_parent(conn, vehicle, object).await;

	let resolved = resolve_implied_tags(conn, &[car_uuid])
		.await
		.expect("resolve implied");
	assert_eq!(names(&resolved), vec!["car", "object", "vehicle"]);
}

#[tokio::test]
async fn sibling_alias_canonicalizes_to_ideal() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (car, _) = insert_tag(conn, "car").await;
	let (automobile, automobile_uuid) = insert_tag(conn, "automobile").await;
	add_sibling(conn, automobile, car).await;

	// Applying the alias resolves to the canonical ideal, not the alias.
	let resolved = resolve_implied_tags(conn, &[automobile_uuid])
		.await
		.expect("resolve implied");
	assert_eq!(names(&resolved), vec!["car"]);
}

#[tokio::test]
async fn sibling_alias_combines_with_parent_implication() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (car, _) = insert_tag(conn, "car").await;
	let (vehicle, _) = insert_tag(conn, "vehicle").await;
	let (automobile, automobile_uuid) = insert_tag(conn, "automobile").await;
	add_parent(conn, car, vehicle).await;
	add_sibling(conn, automobile, car).await;

	// alias -> car -> vehicle
	let resolved = resolve_implied_tags(conn, &[automobile_uuid])
		.await
		.expect("resolve implied");
	assert_eq!(names(&resolved), vec!["car", "vehicle"]);
}

#[tokio::test]
async fn resolver_is_loop_safe() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	// Manually craft a parent cycle a -> b -> a (bypassing the action guard)
	// to prove the resolver terminates instead of looping forever.
	let (a, a_uuid) = insert_tag(conn, "a").await;
	let (b, _) = insert_tag(conn, "b").await;
	add_parent(conn, a, b).await;
	add_parent(conn, b, a).await;

	let resolved = resolve_implied_tags(conn, &[a_uuid])
		.await
		.expect("resolve implied");
	assert_eq!(names(&resolved), vec!["a", "b"]);
}

#[tokio::test]
async fn self_loop_input_is_rejected() {
	let id = Uuid::new_v4();
	let input = AddParentTagInput {
		child_tag_id: id,
		parent_tag_id: id,
	};
	assert!(input.validate().is_err(), "self parent must be rejected");

	let sib = AddSiblingTagInput {
		tag_id: id,
		ideal_tag_id: id,
	};
	assert!(sib.validate().is_err(), "self sibling must be rejected");
}

#[tokio::test]
async fn cycle_detection_blocks_back_edge() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (car, _) = insert_tag(conn, "car").await;
	let (vehicle, _) = insert_tag(conn, "vehicle").await;
	let (object, _) = insert_tag(conn, "object").await;
	add_parent(conn, car, vehicle).await;
	add_parent(conn, vehicle, object).await;

	// Adding object -> car would close the cycle car -> vehicle -> object -> car.
	let creates_cycle = would_create_cycle(conn, object, car)
		.await
		.expect("cycle check");
	assert!(creates_cycle, "back edge must be detected as a cycle");

	// A self edge is also a cycle.
	let self_cycle = would_create_cycle(conn, car, car)
		.await
		.expect("cycle check");
	assert!(self_cycle, "self edge must be detected as a cycle");

	// A forward edge that does not close a loop is fine: car already implies
	// object transitively, so adding a direct car -> object edge is acyclic.
	let safe = would_create_cycle(conn, car, object)
		.await
		.expect("cycle check");
	assert!(!safe, "non-cyclic edge must be allowed");
}

#[tokio::test]
async fn removing_parent_restores_resolution() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	let (car, car_uuid) = insert_tag(conn, "car").await;
	let (vehicle, _) = insert_tag(conn, "vehicle").await;
	add_parent(conn, car, vehicle).await;

	use sea_orm::EntityTrait;
	tag_parent::Entity::delete_by_id((car, vehicle))
		.exec(conn)
		.await
		.expect("delete parent edge");

	let resolved = resolve_implied_tags(conn, &[car_uuid])
		.await
		.expect("resolve implied");
	assert_eq!(names(&resolved), vec!["car"]);
}

#[tokio::test]
async fn migration_down_up_round_trips_new_tables() {
	let (_tmp, db) = fresh_db().await;
	let conn = db.conn();

	// Reverse the most recent migration (the A-04 tag relations tables).
	Migrator::down(conn, Some(1)).await.expect("migrate down");

	// Inserting into the dropped table must now fail.
	let (car, _) = insert_tag(conn, "car").await;
	let (vehicle, _) = insert_tag(conn, "vehicle").await;
	let orphaned = tag_parent::ActiveModel {
		child_tag_id: Set(car),
		parent_tag_id: Set(vehicle),
		created_at: Set(chrono::Utc::now()),
	}
	.insert(conn)
	.await;
	assert!(
		orphaned.is_err(),
		"tag_parent table should be gone after down"
	);

	// Re-apply and confirm the table is usable again.
	Migrator::up(conn, Some(1)).await.expect("migrate up");
	add_parent(conn, car, vehicle).await;
}
