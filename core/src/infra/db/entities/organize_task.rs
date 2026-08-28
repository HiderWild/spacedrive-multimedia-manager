//! SeaORM entity for a local recursive-organize task.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "organize_tasks")]
pub struct Model {
	#[sea_orm(primary_key, auto_increment = false)]
	pub id: Uuid,
	pub name: String,
	pub root_path: String,
	pub root_path_key: String,
	pub device_slug: String,
	pub volume_id: Option<i32>,
	pub root_entry_uuid: Option<Uuid>,
	pub status: String,
	pub revision: i64,
	pub snapshot_version: i32,
	pub total_entries: i64,
	pub total_units: i64,
	pub total_bytes: i64,
	pub scan_issue_count: i64,
	pub pending_addition_count: i64,
	pub scan_job_id: Option<Uuid>,
	pub commit_job_id: Option<Uuid>,
	pub last_error: Option<String>,
	pub created_at: DateTimeUtc,
	pub updated_at: DateTimeUtc,
	pub completed_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
	#[sea_orm(
		belongs_to = "super::volume::Entity",
		from = "Column::VolumeId",
		to = "super::volume::Column::Id",
		on_delete = "SetNull"
	)]
	Volume,
	#[sea_orm(has_many = "super::organize_task_item::Entity")]
	Items,
}

impl Related<super::volume::Entity> for Entity {
	fn to() -> RelationDef {
		Relation::Volume.def()
	}
}

impl Related<super::organize_task_item::Entity> for Entity {
	fn to() -> RelationDef {
		Relation::Items.def()
	}
}

impl ActiveModelBehavior for ActiveModel {}
