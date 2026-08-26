//! AI album exclusion entity
//!
//! Per-album flags that exclude all member images from face/scene recognition.
//! Extension writes rows when album exclusion is toggled; core reads during
//! derivative queue checks. Absent row = no exclusion for that album.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "ai_album_exclusion")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub id: i32,
	pub uuid: Uuid,
	pub album_id: String,
	pub library_id: Uuid,
	pub exclude_face: bool,
	pub exclude_scene: bool,
	pub created_at: DateTimeUtc,
	pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
