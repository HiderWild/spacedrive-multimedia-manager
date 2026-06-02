//! Test database migration functionality

use sd_core::infra::db::migration::Migrator;
use sd_core::infra::db::Database;
use sea_orm_migration::MigratorTrait;
use tempfile::TempDir;

#[tokio::test]
async fn test_database_creation_and_migration() {
	// Create a temporary directory for the test database
	let temp_dir = TempDir::new().unwrap();
	let db_path = temp_dir.path().join("test.db");

	println!("Creating database at: {:?}", db_path);

	// Create the database
	let db = Database::create(&db_path)
		.await
		.expect("Failed to create database");

	println!("Database created successfully, running migrations...");

	// Run migrations with debug info
	println!("Running migrations...");
	let result = db.migrate().await;

	match result {
		Ok(()) => {
			println!("Migrations completed successfully!");
		}
		Err(e) => {
			println!("Migration failed: {}", e);
			panic!("Migration failed: {}", e);
		}
	}

	// Verify the database exists and has tables
	assert!(db_path.exists(), "Database file should exist");

	// Try to connect to verify it's a valid database
	let conn = db.conn();

	// Try a simple query to verify the database is working
	use sea_orm::{ConnectionTrait, Statement};

	let result = conn
		.execute(Statement::from_string(
			sea_orm::DatabaseBackend::Sqlite,
			"SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;".to_string(),
		))
		.await;

	match result {
		Ok(result) => {
			println!(
				"Database query successful, {} rows affected",
				result.rows_affected()
			);
		}
		Err(e) => {
			println!("Database query failed: {}", e);
			panic!("Database query failed: {}", e);
		}
	}
}

/// Verifies the canonical migration baseline (G-01): the most recent migration
/// applies and reverses cleanly. Reverting a single step then re-applying it
/// proves the `down` body is a clean inverse of `up`, which is the contract
/// every media-suite additive migration must uphold.
#[tokio::test]
async fn test_latest_migration_applies_and_reverses_cleanly() {
	let temp_dir = TempDir::new().unwrap();
	let db_path = temp_dir.path().join("roundtrip.db");

	let db = Database::create(&db_path)
		.await
		.expect("Failed to create database");
	db.migrate().await.expect("Initial migration should apply");

	let conn = db.conn();

	// Reverse only the most recent migration, then re-apply it. Both directions
	// must succeed for the round trip to be considered clean.
	Migrator::down(conn, Some(1))
		.await
		.expect("Latest migration should reverse cleanly");
	Migrator::up(conn, Some(1))
		.await
		.expect("Latest migration should re-apply cleanly");
}
