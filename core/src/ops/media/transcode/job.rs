//! Batch transcode job.
//!
//! `TranscodeJob` walks every video in a library and re-encodes it to a single
//! [`TranscodeConfig`] target. It mirrors the proxy job: a discovery phase loads
//! the work list, a processing phase handles one file at a time, progress is
//! reported per file, and the job checkpoints after each encode so an
//! interrupted run resumes at the next unprocessed entry. Per-file failures are
//! logged and skipped so one bad video never aborts the batch.

use super::{
	config::TranscodeJobConfig,
	generator::TranscodeGenerator,
	state::{TranscodePhase, TranscodeState},
};
use crate::infra::job::{prelude::*, traits::DynJob};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use tracing::warn;

/// Batch transcode job.
#[derive(Serialize, Deserialize)]
pub struct TranscodeJob {
	config: TranscodeJobConfig,
	state: TranscodeState,
}

impl TranscodeJob {
	pub fn new(config: TranscodeJobConfig) -> Self {
		Self {
			config,
			state: TranscodeState::new(),
		}
	}

	pub fn with_defaults() -> Self {
		Self::new(TranscodeJobConfig::default())
	}

	/// Directory outputs are written to: the configured dir, or
	/// `<library>/transcodes`.
	fn output_dir(&self, library: &crate::library::Library) -> PathBuf {
		self.config
			.output_dir
			.clone()
			.unwrap_or_else(|| library.path().join("transcodes"))
	}

	async fn run_discovery(&mut self, ctx: &JobContext<'_>) -> JobResult<()> {
		use crate::infra::db::entities::{content_identity, entry};
		use sea_orm::{
			ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
		};

		ctx.log("Starting transcode discovery phase");
		let db = ctx.library_db();

		let results = entry::Entity::find()
			.filter(entry::Column::Kind.eq(0)) // Files only
			.join(JoinType::InnerJoin, entry::Relation::ContentIdentity.def())
			.filter(content_identity::Column::KindId.eq(2)) // Video kind
			.filter(content_identity::Column::Uuid.is_not_null())
			.all(db)
			.await
			.map_err(|e| JobError::execution(format!("Database query failed: {}", e)))?;

		ctx.log(format!("Found {} video entries", results.len()));

		for entry_model in results {
			let path = crate::ops::indexing::PathResolver::get_full_path(db, entry_model.id)
				.await
				.map_err(|e| JobError::execution(format!("Failed to resolve path: {}", e)))?;

			// Resolve the content UUID for stable output naming.
			let content_uuid = match entry_model.content_id {
				Some(content_id) => content_identity::Entity::find_by_id(content_id)
					.one(db)
					.await
					.ok()
					.flatten()
					.and_then(|ci| ci.uuid),
				None => None,
			};

			self.state
				.entries
				.push((entry_model.id, path, content_uuid));
		}

		ctx.log(format!(
			"Discovery complete: {} entries to process",
			self.state.entries.len()
		));

		Ok(())
	}
}

impl Job for TranscodeJob {
	const NAME: &'static str = "transcode";
	const RESUMABLE: bool = true;
	const DESCRIPTION: Option<&'static str> =
		Some("Transcode videos to a target codec and container");
}

impl DynJob for TranscodeJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscodeJobOutput {
	pub total_processed: usize,
	pub success_count: usize,
	pub error_count: usize,
	pub total_encoding_time_secs: u64,
}

impl From<TranscodeJobOutput> for JobOutput {
	fn from(output: TranscodeJobOutput) -> Self {
		JobOutput::Custom(serde_json::json!({
			"type": "transcode",
			"total_processed": output.total_processed,
			"success_count": output.success_count,
			"error_count": output.error_count,
			"encoding_time_secs": output.total_encoding_time_secs,
		}))
	}
}

#[async_trait::async_trait]
impl JobHandler for TranscodeJob {
	type Output = TranscodeJobOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		if self.state.phase == TranscodePhase::Discovery {
			self.run_discovery(&ctx).await?;
			self.state.phase = TranscodePhase::Processing;
		}

		ctx.log(format!(
			"Transcode processing phase starting with {} entries",
			self.state.entries.len()
		));

		let library = ctx.library_arc();
		let output_dir = self.output_dir(&library);
		let generator = TranscodeGenerator::new(self.config.output.clone());
		let extension = self.config.output.extension();
		let total = self.state.entries.len();

		while self.state.processed < total {
			ctx.check_interrupt().await?;

			let (entry_id, path, content_uuid) = &self.state.entries[self.state.processed];

			ctx.log(format!(
				"Processing {}/{}: {}",
				self.state.processed + 1,
				total,
				path.display()
			));

			// Stable output name: content UUID when known, else entry id.
			let stem = content_uuid
				.map(|u| u.to_string())
				.unwrap_or_else(|| format!("entry_{}", entry_id));
			let output_path = output_dir.join(format!("{}.{}", stem, extension));

			if !self.config.regenerate && output_path.exists() {
				ctx.log(format!(
					"Skipping existing output: {}",
					output_path.display()
				));
				self.state.processed += 1;
				ctx.progress(Progress::Count {
					current: self.state.processed,
					total,
				});
				ctx.checkpoint().await?;
				continue;
			}

			let start = std::time::Instant::now();

			// Per-file errors must NOT abort the batch: log and continue.
			match generator.generate(path, &output_path).await {
				Ok(info) => {
					self.state.total_encoding_time_secs += start.elapsed().as_secs();
					ctx.log(format!(
						"Transcoded {} -> {} ({} bytes)",
						path.display(),
						output_path.display(),
						info.size_bytes
					));
					self.state.success_count += 1;
				}
				Err(e) => {
					warn!("Transcode failed for {}: {}", path.display(), e);
					ctx.log(format!(
						"ERROR: Transcode error for {}: {}",
						path.display(),
						e
					));
					self.state.error_count += 1;
				}
			}

			self.state.processed += 1;

			ctx.progress(Progress::Count {
				current: self.state.processed,
				total,
			});

			// Checkpoint after each video so resume restarts at the next entry.
			ctx.checkpoint().await?;
		}

		self.state.phase = TranscodePhase::Complete;
		ctx.log(format!(
			"Transcode complete: {} success, {} errors, total encoding time: {}s",
			self.state.success_count, self.state.error_count, self.state.total_encoding_time_secs
		));

		Ok(TranscodeJobOutput {
			total_processed: self.state.processed,
			success_count: self.state.success_count,
			error_count: self.state.error_count,
			total_encoding_time_secs: self.state.total_encoding_time_secs,
		})
	}

	async fn on_resume(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log(format!(
			"Resuming transcode job at {}/{} ({}s total encoding time)",
			self.state.processed,
			self.state.entries.len(),
			self.state.total_encoding_time_secs
		));
		Ok(())
	}

	async fn on_pause(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log("Pausing transcode job - state will be preserved");
		Ok(())
	}

	async fn on_cancel(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log(format!(
			"Cancelling transcode job - transcoded {} videos",
			self.state.success_count
		));
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::infra::job::traits::SerializableJob;
	use crate::ops::media::transcode::config::{TranscodeCodec, TranscodeConfig};

	#[test]
	fn job_state_round_trips_for_resume() {
		// Resumability is inherited from the shared Job trait: state serializes to
		// MessagePack via SerializableJob and deserializes back identically, which
		// is exactly what the job manager checkpoints and restores.
		let mut job = TranscodeJob::new(TranscodeJobConfig::new(TranscodeConfig::new(
			TranscodeCodec::H264,
		)));
		job.state.phase = TranscodePhase::Processing;
		job.state.entries = vec![
			(1, PathBuf::from("/videos/a.mov"), Some(uuid::Uuid::nil())),
			(2, PathBuf::from("/videos/b.mov"), None),
		];
		job.state.processed = 1;
		job.state.success_count = 1;

		let bytes = job.serialize_state().expect("serialize");
		let restored = TranscodeJob::deserialize_state(&bytes).expect("deserialize");

		assert_eq!(restored.state.phase, TranscodePhase::Processing);
		assert_eq!(restored.state.processed, 1);
		assert_eq!(restored.state.success_count, 1);
		assert_eq!(restored.state.entries.len(), 2);
		assert_eq!(restored.config.output.codec, TranscodeCodec::H264);
	}
}
