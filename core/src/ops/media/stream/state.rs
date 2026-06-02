//! Streaming-package job state structures.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Job execution phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamPhase {
	Discovery,
	Processing,
	Complete,
}

/// Resumable state for a batch streaming job.
///
/// `entries` is populated during discovery, then `processed` advances one video
/// at a time. Because the job checkpoints after every package, a resume picks up
/// at the next unprocessed entry; per-rendition skipping inside a package is
/// handled by the generator (existing HLS variant playlists are left in place).
#[derive(Debug, Serialize, Deserialize)]
pub struct StreamState {
	pub phase: StreamPhase,
	/// Entries to process: (entry_id, source_path, content_uuid).
	pub entries: Vec<(i32, PathBuf, Option<uuid::Uuid>)>,
	pub processed: usize,
	pub success_count: usize,
	pub error_count: usize,
	/// Total media segments written across all processed packages.
	pub total_segments: usize,
}

impl StreamState {
	pub fn new() -> Self {
		Self {
			phase: StreamPhase::Discovery,
			entries: Vec::new(),
			processed: 0,
			success_count: 0,
			error_count: 0,
			total_segments: 0,
		}
	}
}

impl Default for StreamState {
	fn default() -> Self {
		Self::new()
	}
}
