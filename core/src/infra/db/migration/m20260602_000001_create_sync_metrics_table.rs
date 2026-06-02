use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.create_table(
				Table::create()
					.table(SyncMetricsSnapshot::Table)
					.if_not_exists()
					.col(
						ColumnDef::new(SyncMetricsSnapshot::Id)
							.integer()
							.not_null()
							.auto_increment()
							.primary_key(),
					)
					.col(
						ColumnDef::new(SyncMetricsSnapshot::LibraryId)
							.uuid()
							.not_null(),
					)
					.col(
						ColumnDef::new(SyncMetricsSnapshot::SnapshotJson)
							.text()
							.not_null(),
					)
					.col(
						ColumnDef::new(SyncMetricsSnapshot::CreatedAt)
							.timestamp_with_time_zone()
							.not_null(),
					)
					.to_owned(),
			)
			.await?;

		// Index on (library_id, created_at) for efficient time-range queries and cleanup
		manager
			.create_index(
				Index::create()
					.name("idx_sync_metrics_snapshot_library_time")
					.table(SyncMetricsSnapshot::Table)
					.col(SyncMetricsSnapshot::LibraryId)
					.col(SyncMetricsSnapshot::CreatedAt)
					.to_owned(),
			)
			.await?;

		Ok(())
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(Table::drop().table(SyncMetricsSnapshot::Table).to_owned())
			.await?;

		Ok(())
	}
}

#[derive(DeriveIden)]
enum SyncMetricsSnapshot {
	Table,
	Id,
	LibraryId,
	SnapshotJson,
	CreatedAt,
}
