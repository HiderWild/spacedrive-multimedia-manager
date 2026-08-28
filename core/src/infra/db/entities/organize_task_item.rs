//! SeaORM entity for one row in a recursive-organize task manifest.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "organize_task_items")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub id: i32,
	pub uuid: Uuid,
	pub task_id: Uuid,
	pub parent_id: Option<i32>,
	pub entry_uuid: Option<Uuid>,
	pub relative_path: String,
	pub relative_path_key: String,
	pub name: String,
	pub extension: Option<String>,
	pub kind: String,
	pub size_bytes: i64,
	pub aggregate_size_bytes: i64,
	pub modified_at_100ns: i64,
	pub metadata_signature: String,
	pub tree_start: Option<i64>,
	pub tree_end: Option<i64>,
	pub unit_count: Option<i64>,
	pub membership_state: String,
	pub external_state: String,
	pub decision_kind: Option<String>,
	pub move_destination: Option<String>,
	pub operation_state: String,
	pub last_error: Option<String>,
	pub applied_at: Option<DateTimeUtc>,
	pub created_at: DateTimeUtc,
	pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
	#[sea_orm(
		belongs_to = "super::organize_task::Entity",
		from = "Column::TaskId",
		to = "super::organize_task::Column::Id",
		on_delete = "Cascade"
	)]
	Task,
	#[sea_orm(
		belongs_to = "super::organize_task_item::Entity",
		from = "Column::ParentId",
		to = "super::organize_task_item::Column::Id",
		on_delete = "Cascade"
	)]
	Parent,
	#[sea_orm(has_many = "super::organize_task_item::Entity")]
	Children,
}

impl Related<super::organize_task::Entity> for Entity {
	fn to() -> RelationDef {
		Relation::Task.def()
	}
}

impl Related<super::organize_task_item::Entity> for Entity {
	fn to() -> RelationDef {
		Relation::Parent.def()
	}
}

impl ActiveModelBehavior for ActiveModel {}
