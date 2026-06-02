//! Batch image rotation job.
//!
//! `RotateJob` walks every image in a library and applies a single
//! [`RotateOp`][super::config::RotateOp] to its pixels, writing each result back
//! in place. It mirrors the transcode and proxy jobs: a discovery phase loads the
//! work list, a processing phase handles one file at a time, progress is reported
//! per file, and the job checkpoints after each rotation so an interrupted run
//! resumes at the next unprocessed entry. Per-file failures are logged and
//! skipped so one bad image never aborts the batch.
//!
//! Because rotation changes the pixels, any cached thumbnails are now stale. When
//! the `ffmpeg` feature (which owns thumbnailing) is enabled, the job dispatches
//! a thumbnail regeneration pass for exactly the files it rewrote; otherwise it
//! logs that thumbnails need regeneration.

use super::{
	config::RotateJobConfig,
	state::{RotatePhase, RotateState},
	transform::rotate_file,
};
use crate::infra::job::{prelude::*, traits::DynJob};
use serde::{Deserialize, Serialize};
use specta::Type;
use tracing::warn;

/// Batch image rotation job.
#[derive(Serialize, Deserialize)]
pub struct RotateJob {
	config: RotateJobConfig,
	state: RotateState,
}

impl RotateJob {
	pub fn new(config: RotateJobConfig) -> Self {
		Self {
			config,
			state: RotateState::new(),
		}
	}

	pub fn with_defaults() -> Self {
		Self::new(RotateJobConfig::default())
	}

	async fn run_discovery(&mut self, ctx: &JobContext<'_>) -> JobResult<()> {
		use crate::infra::db::entities::{content_identity, entry};
		use sea_orm::{
			ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
		};

		ctx.log("Starting rotate discovery phase");
		let db = ctx.library_db();

		let results = entry::Entity::find()
			.filter(entry::Column::Kind.eq(0)) // Files only
			.join(JoinType::InnerJoin, entry::Relation::ContentIdentity.def())
			.filter(content_identity::Column::KindId.eq(1)) // Image kind
			.all(db)
			.await
			.map_err(|e| JobError::execution(format!("Database query failed: {}", e)))?;

		ctx.log(format!("Found {} image entries", results.len()));

		for entry_model in results {
			let path = crate::ops::indexing::PathResolver::get_full_path(db, entry_model.id)
				.await
				.map_err(|e| JobError::execution(format!("Failed to resolve path: {}", e)))?;

			self.state
				.entries
				.push((entry_model.id, entry_model.uuid, path));
		}

		ctx.log(format!(
			"Discovery complete: {} entries to process",
			self.state.entries.len()
		));

		Ok(())
	}

	/// Dispatch a thumbnail regeneration pass for the rewritten files.
	///
	/// Thumbnailing lives behind the `ffmpeg` feature. When that feature is off,
	/// the stale thumbnails are simply noted in the log so an operator can
	/// regenerate them, rather than building a parallel thumbnail path here.
	async fn regenerate_thumbnails(&self, ctx: &JobContext<'_>) {
		if !self.config.regenerate_thumbnails || self.state.rotated_entries.is_empty() {
			return;
		}

		#[cfg(feature = "ffmpeg")]
		{
			let library = ctx.library_arc();
			match library
				.generate_thumbnails(Some(self.state.rotated_entries.clone()))
				.await
			{
				Ok(_) => ctx.log(format!(
					"Dispatched thumbnail regeneration for {} rotated files",
					self.state.rotated_entries.len()
				)),
				Err(e) => ctx.log(format!(
					"WARNING: failed to dispatch thumbnail regeneration: {}",
					e
				)),
			}
		}

		#[cfg(not(feature = "ffmpeg"))]
		{
			ctx.log(format!(
				"{} rotated files need thumbnail regeneration (ffmpeg feature disabled)",
				self.state.rotated_entries.len()
			));
		}
	}
}

impl Job for RotateJob {
	const NAME: &'static str = "rotate";
	const RESUMABLE: bool = true;
	const DESCRIPTION: Option<&'static str> =
		Some("Rotate or flip images in place and normalize orientation");
}

impl DynJob for RotateJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RotateJobOutput {
	pub total_processed: usize,
	pub success_count: usize,
	pub error_count: usize,
	/// Number of files whose thumbnails are now stale and need regeneration.
	pub thumbnails_pending: usize,
}

impl From<RotateJobOutput> for JobOutput {
	fn from(output: RotateJobOutput) -> Self {
		JobOutput::Custom(serde_json::json!({
			"type": "rotate",
			"total_processed": output.total_processed,
			"success_count": output.success_count,
			"error_count": output.error_count,
			"thumbnails_pending": output.thumbnails_pending,
		}))
	}
}

#[async_trait::async_trait]
impl JobHandler for RotateJob {
	type Output = RotateJobOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		if self.state.phase == RotatePhase::Discovery {
			self.run_discovery(&ctx).await?;
			self.state.phase = RotatePhase::Processing;
		}

		ctx.log(format!(
			"Rotate processing phase starting with {} entries (op: {:?})",
			self.state.entries.len(),
			self.config.op
		));

		let op = self.config.op;
		let total = self.state.entries.len();

		while self.state.processed < total {
			ctx.check_interrupt().await?;

			let (_entry_id, entry_uuid, path) = self.state.entries[self.state.processed].clone();

			ctx.log(format!(
				"Processing {}/{}: {}",
				self.state.processed + 1,
				total,
				path.display()
			));

			// Image work is CPU-bound; keep it off the async runtime threads.
			let result = tokio::task::spawn_blocking(move || rotate_file(&path, op))
				.await
				.map_err(|e| JobError::execution(format!("Rotate task panicked: {}", e)))?;

			match result {
				Ok(info) => {
					ctx.log(format!(
						"Rotated to {}x{} ({} bytes, icc={}, orientation_normalized={})",
						info.width,
						info.height,
						info.size_bytes,
						info.icc_preserved,
						info.orientation_normalized
					));
					self.state.success_count += 1;
					if let Some(uuid) = entry_uuid {
						self.state.rotated_entries.push(uuid);
					}
				}
				Err(e) => {
					let path = &self.state.entries[self.state.processed].2;
					warn!("Rotate failed for {}: {}", path.display(), e);
					ctx.log(format!("ERROR: Rotate error for {}: {}", path.display(), e));
					self.state.error_count += 1;
				}
			}

			self.state.processed += 1;

			ctx.progress(Progress::Count {
				current: self.state.processed,
				total,
			});

			// Checkpoint after each image so resume restarts at the next entry.
			ctx.checkpoint().await?;
		}

		self.regenerate_thumbnails(&ctx).await;

		self.state.phase = RotatePhase::Complete;
		ctx.log(format!(
			"Rotate complete: {} success, {} errors",
			self.state.success_count, self.state.error_count
		));

		Ok(RotateJobOutput {
			total_processed: self.state.processed,
			success_count: self.state.success_count,
			error_count: self.state.error_count,
			thumbnails_pending: self.state.rotated_entries.len(),
		})
	}

	async fn on_resume(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log(format!(
			"Resuming rotate job at {}/{}",
			self.state.processed,
			self.state.entries.len()
		));
		Ok(())
	}

	async fn on_pause(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log("Pausing rotate job - state will be preserved");
		Ok(())
	}

	async fn on_cancel(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log(format!(
			"Cancelling rotate job - rotated {} images",
			self.state.success_count
		));
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::infra::job::traits::SerializableJob;
	use crate::ops::media::rotate::config::RotateOp;
	use std::path::PathBuf;

	#[test]
	fn job_state_round_trips_for_resume() {
		// Resumability is inherited from the shared Job trait: state serializes to
		// MessagePack via SerializableJob and deserializes back identically, which
		// is exactly what the job manager checkpoints and restores.
		let mut job = RotateJob::new(RotateJobConfig::new(RotateOp::Cw90));
		job.state.phase = RotatePhase::Processing;
		job.state.entries = vec![
			(1, Some(uuid::Uuid::nil()), PathBuf::from("/images/a.jpg")),
			(2, None, PathBuf::from("/images/b.png")),
		];
		job.state.processed = 1;
		job.state.success_count = 1;
		job.state.rotated_entries = vec![uuid::Uuid::nil()];

		let bytes = job.serialize_state().expect("serialize");
		let restored = RotateJob::deserialize_state(&bytes).expect("deserialize");

		assert_eq!(restored.state.phase, RotatePhase::Processing);
		assert_eq!(restored.state.processed, 1);
		assert_eq!(restored.state.success_count, 1);
		assert_eq!(restored.state.entries.len(), 2);
		assert_eq!(restored.state.rotated_entries.len(), 1);
		assert_eq!(restored.config.op, RotateOp::Cw90);
	}
}
