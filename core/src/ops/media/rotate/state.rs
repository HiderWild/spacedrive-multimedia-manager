//! Rotate job state structures.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Job execution phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RotatePhase {
	Discovery,
	Processing,
	Complete,
}

/// Resumable state for a batch rotate job.
///
/// `entries` is populated during discovery, then `processed` advances one image
/// at a time. The job checkpoints after every file, so a resume picks up at the
/// next unprocessed entry without re-rotating completed work. `rotated_entries`
/// accumulates the entry UUIDs of successfully rewritten files so a thumbnail
/// regeneration pass can target exactly those.
#[derive(Debug, Serialize, Deserialize)]
pub struct RotateState {
	pub phase: RotatePhase,
	/// Entries to process: (entry_id, entry_uuid, source_path).
	pub entries: Vec<(i32, Option<Uuid>, PathBuf)>,
	pub processed: usize,
	pub success_count: usize,
	pub error_count: usize,
	/// UUIDs of files whose pixels were rewritten; thumbnails are now stale.
	pub rotated_entries: Vec<Uuid>,
}

impl RotateState {
	pub fn new() -> Self {
		Self {
			phase: RotatePhase::Discovery,
			entries: Vec::new(),
			processed: 0,
			success_count: 0,
			error_count: 0,
			rotated_entries: Vec::new(),
		}
	}
}

impl Default for RotateState {
	fn default() -> Self {
		Self::new()
	}
}
