//! AI album members entity
//!
//! Junction table linking albums to their member content_uuids. The photos
//! extension maintains membership; core JOINs against ai_album_exclusion to
//! determine if a content is in any excluded album.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "ai_album_members")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub id: i32,
	pub album_id: String,
	pub content_uuid: Uuid,
	pub library_id: Uuid,
	pub added_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
