//! Persistence layer for sync metrics
//!
//! Stores periodic metrics snapshots in the database for historical analysis
//! and provides retrieval with time-range filtering and automatic cleanup.

use crate::service::sync::metrics::snapshot::SyncMetricsSnapshot;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set};
use std::sync::Arc;
use uuid::Uuid;

/// Store a metrics snapshot in the database.
///
/// Serializes the snapshot to JSON and inserts a row into the
/// `sync_metrics_snapshot` table. The snapshot is timestamped at insertion time.
pub async fn store_metrics_snapshot(
	db: &Arc<DatabaseConnection>,
	library_id: Uuid,
	snapshot: SyncMetricsSnapshot,
) -> Result<()> {
	let snapshot_json = serde_json::to_string(&snapshot)?;

	let active_model = crate::infra::db::entities::sync_metrics_snapshot::ActiveModel {
		library_id: Set(library_id),
		snapshot_json: Set(snapshot_json),
		created_at: Set(Utc::now().into()),
		..Default::default()
	};

	active_model.insert(db.as_ref()).await?;

	tracing::debug!(
		library_id = %library_id,
		timestamp = %snapshot.timestamp,
		"Stored metrics snapshot to database"
	);

	Ok(())
}

/// Retrieve metrics snapshots from the database.
///
/// Returns snapshots for the given `library_id`, optionally filtered by a
/// `since` timestamp and capped at `limit` rows (most recent first).
pub async fn get_metrics_snapshots(
	db: &Arc<DatabaseConnection>,
	library_id: Uuid,
	since: Option<DateTime<Utc>>,
	limit: Option<u32>,
) -> Result<Vec<SyncMetricsSnapshot>> {
	use crate::infra::db::entities::sync_metrics_snapshot;

	let mut query = sync_metrics_snapshot::Entity::find()
		.filter(sync_metrics_snapshot::Column::LibraryId.eq(library_id))
		.order_by_desc(sync_metrics_snapshot::Column::CreatedAt);

	if let Some(since) = since {
		query = query.filter(sync_metrics_snapshot::Column::CreatedAt.gte(since));
	}

	let limit_val = limit.unwrap_or(100);
	query = query.limit(limit_val as u64);

	let results = query.all(db.as_ref()).await?;

	let snapshots: Vec<SyncMetricsSnapshot> = results
		.into_iter()
		.filter_map(|row| match serde_json::from_str(&row.snapshot_json) {
			Ok(snapshot) => Some(snapshot),
			Err(e) => {
				tracing::warn!(
					error = %e,
					id = row.id,
					"Failed to deserialize metrics snapshot, skipping"
				);
				None
			}
		})
		.collect();

	Ok(snapshots)
}

/// Clean up old metrics snapshots.
///
/// Deletes all snapshots for the given `library_id` that were created before
/// `older_than`. Returns the number of rows deleted.
pub async fn cleanup_old_metrics(
	db: &Arc<DatabaseConnection>,
	library_id: Uuid,
	older_than: DateTime<Utc>,
) -> Result<usize> {
	use crate::infra::db::entities::sync_metrics_snapshot;

	let result = sync_metrics_snapshot::Entity::delete_many()
		.filter(sync_metrics_snapshot::Column::LibraryId.eq(library_id))
		.filter(sync_metrics_snapshot::Column::CreatedAt.lt(older_than))
		.exec(db.as_ref())
		.await?;

	if result.rows_affected > 0 {
		tracing::debug!(
			library_id = %library_id,
			deleted = result.rows_affected,
			cutoff = %older_than,
			"Cleaned up old metrics snapshots"
		);
	}

	Ok(result.rows_affected as usize)
}
