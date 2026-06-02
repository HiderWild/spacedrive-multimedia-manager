//! Sync metrics snapshot entity
//!
//! Stores periodic snapshots of sync metrics for historical analysis.
//! Each row is a JSON-serialized `SyncMetricsSnapshot` tied to a library and timestamp.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sync_metrics_snapshot")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub id: i32,

	/// Library this snapshot belongs to
	pub library_id: Uuid,

	/// JSON-serialized metrics snapshot
	pub snapshot_json: String,

	/// When this snapshot was taken
	pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
