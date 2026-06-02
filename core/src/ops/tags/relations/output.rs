//! Tag relation outputs (A-04).

use crate::domain::tag::Tag;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AddParentTagOutput {
	pub child_tag_id: Uuid,
	pub parent_tag_id: Uuid,
	pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RemoveParentTagOutput {
	pub child_tag_id: Uuid,
	pub parent_tag_id: Uuid,
	/// Number of edges removed (0 if none existed).
	pub removed: usize,
	pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AddSiblingTagOutput {
	pub tag_id: Uuid,
	pub ideal_tag_id: Uuid,
	pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RemoveSiblingTagOutput {
	pub tag_id: Uuid,
	/// Number of alias rows removed (0 if none existed).
	pub removed: usize,
	pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ResolveImpliedTagsOutput {
	/// The expanded, canonicalized set of implied tags.
	pub tags: Vec<Tag>,
}
