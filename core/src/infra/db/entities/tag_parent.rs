//! Tag parent entity (A-04).
//!
//! A directed "implies" edge between two tags: applying `child_tag_id` to an
//! item means `parent_tag_id` is also effectively present. Edges are transitive
//! (`car` -> `vehicle` -> `object`) and resolved at read time by the relation
//! resolver. Self-loops and cycles are rejected by the write actions, but the
//! resolver stays loop-safe regardless of stored data.

use sea_orm::entity::prelude::*;
use sea_orm::Set;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tag_parent")]
pub struct Model {
	#[sea_orm(primary_key, auto_increment = false)]
	pub child_tag_id: i32,
	#[sea_orm(primary_key, auto_increment = false)]
	pub parent_tag_id: i32,
	pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
	#[sea_orm(
		belongs_to = "super::tag::Entity",
		from = "Column::ChildTagId",
		to = "super::tag::Column::Id"
	)]
	ChildTag,

	#[sea_orm(
		belongs_to = "super::tag::Entity",
		from = "Column::ParentTagId",
		to = "super::tag::Column::Id"
	)]
	ParentTag,
}

impl ActiveModelBehavior for ActiveModel {
	fn new() -> Self {
		Self {
			created_at: Set(chrono::Utc::now()),
			..ActiveModelTrait::default()
		}
	}
}
