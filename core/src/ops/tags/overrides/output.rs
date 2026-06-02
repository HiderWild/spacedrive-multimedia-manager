//! Outputs for the tag override / restore actions (task A-03).

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OverrideTagOutput {
	/// Entry the override was written to.
	pub entry_id: Uuid,

	/// Tag that is now suppressed on the entry.
	pub tag_id: Uuid,

	/// Ancestor recorded as the override source, if one was provided and resolved.
	pub overridden_from: Option<Uuid>,

	pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RemoveTagOverrideOutput {
	/// Entry the override was cleared from.
	pub entry_id: Uuid,

	/// Tag whose inheritance was restored.
	pub tag_id: Uuid,

	/// Number of override rows removed (0 if none existed).
	pub overrides_removed: usize,

	pub message: String,
}
