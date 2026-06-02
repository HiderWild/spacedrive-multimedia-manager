//! Tag sibling entity (A-04).
//!
//! An alias edge: `tag_id` is a sibling/alias of the canonical `ideal_tag_id`.
//! Siblings collapse to their ideal tag during resolution so the same concept
//! displays under one canonical tag (e.g. `automobile` -> `car`). Each tag can
//! alias at most one ideal, so `tag_id` is the primary key. Sibling chains are
//! followed transitively and the resolver is loop-safe against cyclic data.

use sea_orm::entity::prelude::*;
use sea_orm::Set;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tag_sibling")]
pub struct Model {
	#[sea_orm(primary_key, auto_increment = false)]
	pub tag_id: i32,
	pub ideal_tag_id: i32,
	pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
	#[sea_orm(
		belongs_to = "super::tag::Entity",
		from = "Column::TagId",
		to = "super::tag::Column::Id"
	)]
	Tag,

	#[sea_orm(
		belongs_to = "super::tag::Entity",
		from = "Column::IdealTagId",
		to = "super::tag::Column::Id"
	)]
	IdealTag,
}

impl ActiveModelBehavior for ActiveModel {
	fn new() -> Self {
		Self {
			created_at: Set(chrono::Utc::now()),
			..ActiveModelTrait::default()
		}
	}
}
