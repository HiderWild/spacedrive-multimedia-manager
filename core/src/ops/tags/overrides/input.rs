//! Inputs for the tag override / restore actions (task A-03).

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Mark a tag as overridden (suppressed) on a specific entry so that the entry
/// stops inheriting that tag from its ancestor folders.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OverrideTagInput {
	/// Entry UUID (File.id) to override the tag on.
	pub entry_id: Uuid,

	/// Tag UUID to suppress on this entry.
	pub tag_id: Uuid,

	/// Optional ancestor entry UUID whose inherited tag is being overridden.
	/// Stored for provenance; resolution does not require it.
	pub source_ancestor_id: Option<Uuid>,
}

impl OverrideTagInput {
	pub fn validate(&self) -> Result<(), String> {
		if self.entry_id.is_nil() {
			return Err("entry_id cannot be nil".to_string());
		}
		if self.tag_id.is_nil() {
			return Err("tag_id cannot be nil".to_string());
		}
		Ok(())
	}
}

/// Remove an existing override on an entry so the tag is inherited again.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RemoveTagOverrideInput {
	/// Entry UUID (File.id) to restore inheritance on.
	pub entry_id: Uuid,

	/// Tag UUID whose override should be cleared.
	pub tag_id: Uuid,
}

impl RemoveTagOverrideInput {
	pub fn validate(&self) -> Result<(), String> {
		if self.entry_id.is_nil() {
			return Err("entry_id cannot be nil".to_string());
		}
		if self.tag_id.is_nil() {
			return Err("tag_id cannot be nil".to_string());
		}
		Ok(())
	}
}
