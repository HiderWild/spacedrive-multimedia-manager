//! Batch streaming-package job.
//!
//! `StreamJob` walks every video in a library and packages it for adaptive
//! streaming to a single [`StreamConfig`] target. It mirrors the transcode job:
//! a discovery phase loads the work list, a processing phase handles one file at
//! a time, progress is reported per file, and the job checkpoints after each
//! package so an interrupted run resumes at the next unprocessed entry.
//! Per-rendition skipping inside a package is handled by the generator. Per-file
//! failures are logged and skipped so one bad video never aborts the batch.

use super::{
	config::StreamJobConfig,
	generator::StreamGenerator,
	state::{StreamPhase, StreamState},
};
use crate::infra::job::{prelude::*, traits::DynJob};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use tracing::warn;

/// Batch streaming-package job.
#[derive(Serialize, Deserialize)]
pub struct StreamJob {
	config: StreamJobConfig,
	state: StreamState,
}

impl StreamJob {
	pub fn new(config: StreamJobConfig) -> Self {
		Self {
			config,
			state: StreamState::new(),
		}
	}

	pub fn with_defaults() -> Self {
		Self::new(StreamJobConfig::default())
	}

	/// Directory packages are written under: the configured dir, or
	/// `<library>/streams`.
	fn output_dir(&self, library: &crate::library::Library) -> PathBuf {
		self.config
			.output_dir
			.clone()
			.unwrap_or_else(|| library.path().join("streams"))
	}

	async fn run_discovery(&mut self, ctx: &JobContext<'_>) -> JobResult<()> {
		use crate::infra::db::entities::{content_identity, entry};
		use sea_orm::{
			ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait,
		};

		ctx.log("Starting stream discovery phase");
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

impl Job for StreamJob {
	const NAME: &'static str = "stream";
	const RESUMABLE: bool = true;
	const DESCRIPTION: Option<&'static str> =
		Some("Package videos into HLS/DASH adaptive streaming");
}

impl DynJob for StreamJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct StreamJobOutput {
	pub total_processed: usize,
	pub success_count: usize,
	pub error_count: usize,
	pub total_segments: usize,
}

impl From<StreamJobOutput> for JobOutput {
	fn from(output: StreamJobOutput) -> Self {
		JobOutput::Custom(serde_json::json!({
			"type": "stream",
			"total_processed": output.total_processed,
			"success_count": output.success_count,
			"error_count": output.error_count,
			"total_segments": output.total_segments,
		}))
	}
}

#[async_trait::async_trait]
impl JobHandler for StreamJob {
	type Output = StreamJobOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		if self.state.phase == StreamPhase::Discovery {
			self.run_discovery(&ctx).await?;
			self.state.phase = StreamPhase::Processing;
		}

		ctx.log(format!(
			"Stream processing phase starting with {} entries",
			self.state.entries.len()
		));

		let library = ctx.library_arc();
		let output_dir = self.output_dir(&library);
		let generator = if self.config.regenerate {
			StreamGenerator::new_regenerating(self.config.output.clone())
		} else {
			StreamGenerator::new(self.config.output.clone())
		};
		let manifest_name = self.config.output.manifest_name();
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

			// Stable package name: content UUID when known, else entry id.
			let stem = content_uuid
				.map(|u| u.to_string())
				.unwrap_or_else(|| format!("entry_{}", entry_id));
			let package_dir = output_dir.join(&stem);

			// Whole-package skip: when not regenerating and the manifest already
			// exists, the package is complete. Per-rendition skipping for partial
			// HLS packages is handled inside the generator.
			if !self.config.regenerate && package_dir.join(manifest_name).exists() {
				ctx.log(format!(
					"Skipping existing package: {}",
					package_dir.display()
				));
				self.state.processed += 1;
				ctx.progress(Progress::Count {
					current: self.state.processed,
					total,
				});
				ctx.checkpoint().await?;
				continue;
			}

			// Per-file errors must NOT abort the batch: log and continue.
			match generator.generate(path, &package_dir).await {
				Ok(info) => {
					self.state.total_segments += info.segment_count;
					ctx.log(format!(
						"Packaged {} -> {} ({} renditions, {} segments)",
						path.display(),
						info.manifest_path.display(),
						info.rendition_count,
						info.segment_count
					));
					self.state.success_count += 1;
				}
				Err(e) => {
					warn!("Stream packaging failed for {}: {}", path.display(), e);
					ctx.log(format!("ERROR: Stream error for {}: {}", path.display(), e));
					self.state.error_count += 1;
				}
			}

			self.state.processed += 1;

			ctx.progress(Progress::Count {
				current: self.state.processed,
				total,
			});

			// Checkpoint after each package so resume restarts at the next entry.
			ctx.checkpoint().await?;
		}

		self.state.phase = StreamPhase::Complete;
		ctx.log(format!(
			"Stream complete: {} success, {} errors, {} segments",
			self.state.success_count, self.state.error_count, self.state.total_segments
		));

		Ok(StreamJobOutput {
			total_processed: self.state.processed,
			success_count: self.state.success_count,
			error_count: self.state.error_count,
			total_segments: self.state.total_segments,
		})
	}

	async fn on_resume(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log(format!(
			"Resuming stream job at {}/{}",
			self.state.processed,
			self.state.entries.len()
		));
		Ok(())
	}

	async fn on_pause(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log("Pausing stream job - state will be preserved");
		Ok(())
	}

	async fn on_cancel(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log(format!(
			"Cancelling stream job - packaged {} videos",
			self.state.success_count
		));
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::infra::job::traits::SerializableJob;
	use crate::ops::media::stream::config::{StreamConfig, StreamProtocol};

	#[test]
	fn job_state_round_trips_for_resume() {
		let mut job = StreamJob::new(StreamJobConfig::new(StreamConfig::new(StreamProtocol::Hls)));
		job.state.phase = StreamPhase::Processing;
		job.state.entries = vec![
			(1, PathBuf::from("/videos/a.mov"), Some(uuid::Uuid::nil())),
			(2, PathBuf::from("/videos/b.mov"), None),
		];
		job.state.processed = 1;
		job.state.success_count = 1;
		job.state.total_segments = 7;

		let bytes = job.serialize_state().expect("serialize");
		let restored = StreamJob::deserialize_state(&bytes).expect("deserialize");

		assert_eq!(restored.state.phase, StreamPhase::Processing);
		assert_eq!(restored.state.processed, 1);
		assert_eq!(restored.state.success_count, 1);
		assert_eq!(restored.state.total_segments, 7);
		assert_eq!(restored.state.entries.len(), 2);
		assert_eq!(restored.config.output.protocol, StreamProtocol::Hls);
	}
}
