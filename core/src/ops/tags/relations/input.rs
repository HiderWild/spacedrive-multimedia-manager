//! Tag relation inputs (A-04).
//!
//! Inputs for the parent-implication and sibling-alias write actions, plus the
//! read-side implied-tag resolver query.

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Add a parent implication: `child_tag_id` implies `parent_tag_id`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AddParentTagInput {
	/// Tag UUID that implies the parent.
	pub child_tag_id: Uuid,
	/// Tag UUID implied by the child.
	pub parent_tag_id: Uuid,
}

impl AddParentTagInput {
	pub fn validate(&self) -> Result<(), String> {
		if self.child_tag_id.is_nil() || self.parent_tag_id.is_nil() {
			return Err("tag ids cannot be nil".to_string());
		}
		if self.child_tag_id == self.parent_tag_id {
			return Err("a tag cannot be its own parent".to_string());
		}
		Ok(())
	}
}

/// Remove a parent implication edge.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RemoveParentTagInput {
	pub child_tag_id: Uuid,
	pub parent_tag_id: Uuid,
}

impl RemoveParentTagInput {
	pub fn validate(&self) -> Result<(), String> {
		if self.child_tag_id.is_nil() || self.parent_tag_id.is_nil() {
			return Err("tag ids cannot be nil".to_string());
		}
		Ok(())
	}
}

/// Add a sibling alias: `tag_id` is an alias of the canonical `ideal_tag_id`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AddSiblingTagInput {
	/// Alias tag UUID that collapses to the ideal.
	pub tag_id: Uuid,
	/// Canonical tag UUID the alias resolves to.
	pub ideal_tag_id: Uuid,
}

impl AddSiblingTagInput {
	pub fn validate(&self) -> Result<(), String> {
		if self.tag_id.is_nil() || self.ideal_tag_id.is_nil() {
			return Err("tag ids cannot be nil".to_string());
		}
		if self.tag_id == self.ideal_tag_id {
			return Err("a tag cannot be its own sibling".to_string());
		}
		Ok(())
	}
}

/// Remove a sibling alias for a tag, restoring it to a standalone tag.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RemoveSiblingTagInput {
	pub tag_id: Uuid,
}

impl RemoveSiblingTagInput {
	pub fn validate(&self) -> Result<(), String> {
		if self.tag_id.is_nil() {
			return Err("tag_id cannot be nil".to_string());
		}
		Ok(())
	}
}

/// Resolve the implied, canonicalized tag set for a set of applied tags.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ResolveImpliedTagsInput {
	/// Applied tag UUIDs to expand via parents and canonicalize via siblings.
	pub tag_ids: Vec<Uuid>,
}

impl ResolveImpliedTagsInput {
	pub fn validate(&self) -> Result<(), String> {
		if self.tag_ids.iter().any(|id| id.is_nil()) {
			return Err("tag_ids cannot contain a nil uuid".to_string());
		}
		Ok(())
	}
}
