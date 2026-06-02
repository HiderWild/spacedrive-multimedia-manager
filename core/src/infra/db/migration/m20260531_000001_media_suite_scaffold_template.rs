//! Media-suite scaffolding template (G-01).
//!
//! Canonical, copy-pasteable baseline for the additive migrations the media-suite
//! epics will add (tag inheritance sources, rules engine tables, new sidecar
//! kinds). It is intentionally a no-op at runtime: `up` and `down` execute a
//! harmless probe so the migration applies and reverses cleanly without altering
//! the production schema, while the doc comments below carry the real template a
//! developer copies when adding a new migration.
//!
//! ## How to add a real migration
//! 1. Copy this file to `m<YYYYMMDD>_000001_<short_name>.rs` (date must sort after
//!    the latest existing migration).
//! 2. Register it in [`super`]'s `mod.rs`: add a `mod ...;` line and a matching
//!    `Box::new(...)` entry at the end of `migrations()`.
//! 3. Replace the no-op bodies with your schema change, keeping `down` a clean
//!    inverse of `up`.
//!
//! ### Pattern A — add/drop a column (SeaORM builder)
//! ```rust,ignore
//! async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
//! 	manager
//! 		.alter_table(
//! 			Table::alter()
//! 				.table(Entries::Table)
//! 				.add_column(
//! 					ColumnDef::new(Entries::ExampleColumn)
//! 						.big_integer()
//! 						.not_null()
//! 						.default(0),
//! 				)
//! 				.to_owned(),
//! 		)
//! 		.await
//! }
//!
//! async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
//! 	manager
//! 		.alter_table(
//! 			Table::alter()
//! 				.table(Entries::Table)
//! 				.drop_column(Entries::ExampleColumn)
//! 				.to_owned(),
//! 		)
//! 		.await
//! }
//!
//! #[derive(DeriveIden)]
//! enum Entries {
//! 	Table,
//! 	ExampleColumn,
//! }
//! ```
//!
//! ### Pattern B — create/drop an index (raw SQL, for partial or expression indexes)
//! ```rust,ignore
//! manager
//! 	.get_connection()
//! 	.execute_unprepared(
//! 		"CREATE INDEX IF NOT EXISTS idx_example ON entries(example_column)",
//! 	)
//! 	.await?;
//! // down:
//! manager
//! 	.get_connection()
//! 	.execute_unprepared("DROP INDEX IF EXISTS idx_example")
//! 	.await?;
//! ```

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		// No-op probe: proves the migration runs and is recorded in
		// `seaql_migrations` without touching the schema. Replace with a real
		// additive change (see module docs) when scaffolding a new table/column.
		manager
			.get_connection()
			.execute_unprepared("SELECT 1")
			.await?;

		Ok(())
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		// Clean inverse of `up`. For a real migration this drops the column,
		// table, or index created above.
		manager
			.get_connection()
			.execute_unprepared("SELECT 1")
			.await?;

		Ok(())
	}
}
