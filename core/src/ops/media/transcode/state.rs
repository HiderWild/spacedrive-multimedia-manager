//! Transcode job state structures

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Job execution phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TranscodePhase {
	Discovery,
	Processing,
	Complete,
}

/// Resumable state for a batch transcode job.
///
/// `entries` is populated during discovery, then `processed` advances one video
/// at a time. Because the job checkpoints after every file, a resume picks up at
/// the next unprocessed entry without redoing completed work.
#[derive(Debug, Serialize, Deserialize)]
pub struct TranscodeState {
	pub phase: TranscodePhase,
	/// Entries to process: (entry_id, source_path, content_uuid).
	pub entries: Vec<(i32, PathBuf, Option<uuid::Uuid>)>,
	pub processed: usize,
	pub success_count: usize,
	pub error_count: usize,
	pub total_encoding_time_secs: u64,
}

impl TranscodeState {
	pub fn new() -> Self {
		Self {
			phase: TranscodePhase::Discovery,
			entries: Vec::new(),
			processed: 0,
			success_count: 0,
			error_count: 0,
			total_encoding_time_secs: 0,
		}
	}
}

impl Default for TranscodeState {
	fn default() -> Self {
		Self::new()
	}
}
