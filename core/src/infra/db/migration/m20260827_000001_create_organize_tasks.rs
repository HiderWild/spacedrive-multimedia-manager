//! Create the local recursive-organize task manifest.

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.get_connection()
			.execute(Statement::from_string(
				DatabaseBackend::Sqlite,
				organize_tasks_sql(),
			))
			.await?;

		manager
			.get_connection()
			.execute(Statement::from_string(
				DatabaseBackend::Sqlite,
				organize_task_items_sql(),
			))
			.await?;

		for statement in organize_index_sql() {
			manager
				.get_connection()
				.execute(Statement::from_string(DatabaseBackend::Sqlite, statement))
				.await?;
		}

		Ok(())
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.get_connection()
			.execute(Statement::from_string(
				DatabaseBackend::Sqlite,
				"DROP TABLE IF EXISTS organize_task_items".to_string(),
			))
			.await?;
		manager
			.get_connection()
			.execute(Statement::from_string(
				DatabaseBackend::Sqlite,
				"DROP TABLE IF EXISTS organize_tasks".to_string(),
			))
			.await?;
		Ok(())
	}
}

fn organize_tasks_sql() -> String {
	r#"CREATE TABLE IF NOT EXISTS organize_tasks (
    id BLOB NOT NULL PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    root_path TEXT NOT NULL,
    root_path_key TEXT NOT NULL,
    device_slug TEXT NOT NULL,
    volume_id INTEGER,
    root_entry_uuid BLOB,
    status TEXT NOT NULL CHECK (status IN ('scanning', 'active', 'committing', 'completed', 'failed')),
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    snapshot_version INTEGER NOT NULL DEFAULT 1 CHECK (snapshot_version > 0),
    total_entries BIGINT NOT NULL DEFAULT 0 CHECK (total_entries >= 0),
    total_units BIGINT NOT NULL DEFAULT 0 CHECK (total_units >= 0),
    total_bytes BIGINT NOT NULL DEFAULT 0 CHECK (total_bytes >= 0),
    scan_issue_count BIGINT NOT NULL DEFAULT 0 CHECK (scan_issue_count >= 0),
    pending_addition_count BIGINT NOT NULL DEFAULT 0 CHECK (pending_addition_count >= 0),
    scan_job_id BLOB,
    commit_job_id BLOB,
    last_error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    completed_at TIMESTAMP,
    FOREIGN KEY (volume_id) REFERENCES volumes(id) ON DELETE SET NULL
)"#.to_string()
}

fn organize_task_items_sql() -> String {
	r#"CREATE TABLE IF NOT EXISTS organize_task_items (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    uuid BLOB NOT NULL,
    task_id BLOB NOT NULL,
    parent_id INTEGER,
    entry_uuid BLOB,
    relative_path TEXT NOT NULL,
    relative_path_key TEXT NOT NULL,
    name TEXT NOT NULL,
    extension TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('file', 'directory', 'reparse_point', 'unreadable')),
    size_bytes BIGINT NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
    aggregate_size_bytes BIGINT NOT NULL DEFAULT 0 CHECK (aggregate_size_bytes >= 0),
    modified_at_100ns BIGINT NOT NULL DEFAULT 0,
    metadata_signature TEXT NOT NULL,
    tree_start BIGINT,
    tree_end BIGINT,
    unit_count BIGINT,
    membership_state TEXT NOT NULL DEFAULT 'included' CHECK (membership_state IN ('included', 'pending_addition')),
    external_state TEXT NOT NULL DEFAULT 'present' CHECK (external_state IN ('present', 'changed', 'missing', 'unreadable')),
    decision_kind TEXT CHECK (decision_kind IS NULL OR decision_kind IN ('keep', 'discard', 'move')),
    move_destination TEXT,
    operation_state TEXT NOT NULL DEFAULT 'none' CHECK (operation_state IN ('none', 'pending', 'running', 'applied', 'failed')),
    last_error TEXT,
    applied_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    UNIQUE (task_id, uuid),
    UNIQUE (task_id, relative_path_key),
    CHECK (
        (membership_state = 'included' AND tree_start IS NOT NULL AND tree_end IS NOT NULL AND unit_count IS NOT NULL)
        OR
        (membership_state = 'pending_addition' AND tree_start IS NULL AND tree_end IS NULL AND unit_count IS NULL)
    ),
    CHECK (
        (decision_kind = 'move' AND move_destination IS NOT NULL)
        OR
        (decision_kind IS NULL AND move_destination IS NULL)
        OR
        (decision_kind IN ('keep', 'discard') AND move_destination IS NULL)
    ),
    CHECK (
        (decision_kind IS NULL OR decision_kind = 'keep') AND operation_state = 'none'
        OR
        decision_kind IN ('discard', 'move') AND operation_state IN ('none', 'pending', 'running', 'applied', 'failed')
    ),
    CHECK (tree_start IS NULL OR tree_start >= 0),
    CHECK (tree_end IS NULL OR tree_end >= 0),
    CHECK (unit_count IS NULL OR unit_count >= 0),
    CHECK (tree_start IS NULL OR tree_end IS NOT NULL AND tree_end >= tree_start),
    FOREIGN KEY (task_id) REFERENCES organize_tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES organize_task_items(id) ON DELETE CASCADE
)"#.to_string()
}

fn organize_index_sql() -> [String; 4] {
	[
		"CREATE UNIQUE INDEX IF NOT EXISTS idx_organize_items_task_tree_start_included ON organize_task_items (task_id, tree_start) WHERE membership_state = 'included'".to_string(),
		"CREATE INDEX IF NOT EXISTS idx_organize_items_task_parent_name ON organize_task_items (task_id, parent_id, name)".to_string(),
		"CREATE INDEX IF NOT EXISTS idx_organize_items_task_decision_tree ON organize_task_items (task_id, decision_kind, tree_start)".to_string(),
		"CREATE INDEX IF NOT EXISTS idx_organize_items_task_membership_external ON organize_task_items (task_id, membership_state, external_state)".to_string(),
	]
}
