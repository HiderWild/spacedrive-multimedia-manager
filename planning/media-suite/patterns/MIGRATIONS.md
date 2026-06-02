# Adding a Database Migration

Canonical, repo-verified steps for adding a SeaORM migration to Spacedrive core.
Established by task **G-01** so every later media-suite epic (tag inheritance,
rules engine, new sidecar kinds) adds migrations the same way.

## What the system is

- **ORM:** SeaORM over SQLite, one database embedded per library.
- **Migration runner:** `sea_orm_migration`. The `Migrator` lives in
  [core/src/infra/db/migration/mod.rs](../../../core/src/infra/db/migration/mod.rs).
- **Where migrations run:** `Database::migrate()` calls `Migrator::up(&conn, None)`
  during library init — see
  [core/src/infra/db/mod.rs](../../../core/src/infra/db/mod.rs).
- **Tracking:** SeaORM records applied migrations in the `seaql_migrations` table,
  so each migration runs exactly once.
- **Template:** copy
  [m20260531_000001_media_suite_scaffold_template.rs](../../../core/src/infra/db/migration/m20260531_000001_media_suite_scaffold_template.rs).

## Steps

### 1. Create the migration file

Add a file under `core/src/infra/db/migration/` named
`m<YYYYMMDD>_<NNNNNN>_<short_name>.rs`. The date prefix must sort **after** the
latest existing migration (migrations apply in the order listed in `mod.rs`).

Minimal additive example (add then drop a column):

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(Entries::Table)
					.add_column(
						ColumnDef::new(Entries::ExampleColumn)
							.big_integer()
							.not_null()
							.default(0),
					)
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(Entries::Table)
					.drop_column(Entries::ExampleColumn)
					.to_owned(),
			)
			.await
	}
}

#[derive(DeriveIden)]
enum Entries {
	Table,
	ExampleColumn,
}
```

For partial or expression indexes that the builder cannot express, use raw SQL via
`manager.get_connection().execute_unprepared(...)` with `IF NOT EXISTS` /
`IF EXISTS` guards (see
[m20260417_000001_add_entries_sync_cursor_index.rs](../../../core/src/infra/db/migration/m20260417_000001_add_entries_sync_cursor_index.rs)).

**Rules:**
- `down` must be a clean inverse of `up`.
- Explain *why* the change exists in a `//!` module doc, per repo conventions.
- Tabs for indentation; run `cargo fmt`.

### 2. Register it in `mod.rs`

Two edits in
[core/src/infra/db/migration/mod.rs](../../../core/src/infra/db/migration/mod.rs):

```rust
// 1. Declare the module (with the other `mod` lines, in date order)
mod m20260531_000001_media_suite_scaffold_template;

// 2. Add to the migrations() vec (last entry)
Box::new(m20260531_000001_media_suite_scaffold_template::Migration),
```

### 3. (Optional) Regenerate entities / TS types

If the migration changes types exposed to the frontend, update the SeaORM entity
in `core/src/data/` (or relevant domain struct) and regenerate:

```bash
cargo run --bin generate_typescript_types
```

### 4. Verify

```bash
cargo check -p sd-core
cargo test -p sd-core --test database_migration_test
```

`test_latest_migration_applies_and_reverses_cleanly` in
[core/tests/database_migration_test.rs](../../../core/tests/database_migration_test.rs)
reverses the newest migration one step and re-applies it, proving `up`/`down`
round-trip cleanly. Keep that test green for every new migration.
