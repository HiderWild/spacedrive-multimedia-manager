//! Single-file streaming-package action.
//!
//! Mirrors `TranscodeAction`: a library action invoked over RPC that packages
//! one entry (looked up by UUID) into an HLS or DASH adaptive-streaming package.
//! It reuses the same [`StreamGenerator`] plumbing the batch job uses, so the
//! action and the job stay behaviorally identical for a single file.

use super::{
	config::{Rendition, SegmentType, StreamConfig, StreamProtocol},
	generator::StreamGenerator,
};
use crate::{
	context::CoreContext,
	infra::action::{error::ActionError, LibraryAction},
	ops::indexing::path_resolver::PathResolver,
};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;

/// Package a single video entry for adaptive streaming.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type)]
pub struct StreamInput {
	/// UUID of the entry to package.
	pub entry_uuid: Uuid,
	/// Streaming protocol to emit; defaults to HLS.
	#[serde(default)]
	pub protocol: Option<StreamProtocol>,
	/// Target media-segment duration in seconds; defaults to 6.
	pub segment_duration: Option<u32>,
	/// Adaptive bitrate ladder; defaults to 1080p/720p/480p.
	pub ladder: Option<Vec<Rendition>>,
	/// HLS segment container; defaults to MPEG-TS.
	#[serde(default)]
	pub segment_type: Option<SegmentType>,
	/// libx264 speed preset; defaults to `veryfast`.
	pub preset: Option<String>,
	/// Re-package even when a manifest already exists.
	pub force: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type)]
pub struct StreamOutput {
	/// Absolute path of the produced manifest (`master.m3u8` or `manifest.mpd`).
	pub manifest_path: String,
	/// Absolute path of the package directory.
	pub package_dir: String,
	/// Number of renditions in the package.
	pub rendition_count: u32,
	/// Number of media segment files produced.
	pub segment_count: u32,
	/// Total package size in bytes.
	pub total_size_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamAction {
	input: StreamInput,
}

impl StreamAction {
	pub fn new(input: StreamInput) -> Self {
		Self { input }
	}

	fn build_config(&self) -> StreamConfig {
		let mut config = StreamConfig::new(self.input.protocol.unwrap_or_default());

		if let Some(duration) = self.input.segment_duration {
			config = config.with_segment_duration(duration);
		}
		if let Some(ladder) = self.input.ladder.clone() {
			config = config.with_ladder(ladder);
		}
		if let Some(segment_type) = self.input.segment_type {
			config = config.with_segment_type(segment_type);
		}
		if let Some(preset) = self.input.preset.clone() {
			config = config.with_preset(preset);
		}

		config
	}
}

impl LibraryAction for StreamAction {
	type Input = StreamInput;
	type Output = StreamOutput;

	fn from_input(input: StreamInput) -> Result<Self, String> {
		Ok(Self::new(input))
	}

	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		use crate::infra::db::entities;
		use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

		let db = library.db().conn();

		let entry = entities::entry::Entity::find()
			.filter(entities::entry::Column::Uuid.eq(self.input.entry_uuid))
			.one(db)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to load entry: {}", e)))?
			.ok_or_else(|| ActionError::Internal("Entry not found".to_string()))?;

		let path = PathResolver::get_full_path(db, entry.id)
			.await
			.map_err(|e| ActionError::Internal(format!("Failed to resolve path: {}", e)))?;

		let content_uuid = match entry.content_id {
			Some(content_id) => entities::content_identity::Entity::find_by_id(content_id)
				.one(db)
				.await
				.ok()
				.flatten()
				.and_then(|ci| ci.uuid),
			None => None,
		};

		let config = self.build_config();
		config
			.validate()
			.map_err(|e| ActionError::Internal(e.to_string()))?;

		let manifest_name = config.manifest_name();
		let stem = content_uuid
			.map(|u| u.to_string())
			.unwrap_or_else(|| format!("entry_{}", entry.id));
		let package_dir = library.path().join("streams").join(&stem);

		if !self.input.force && package_dir.join(manifest_name).exists() {
			return Err(ActionError::Internal(format!(
				"Package already exists: {} (set force to overwrite)",
				package_dir.display()
			)));
		}

		let generator = if self.input.force {
			StreamGenerator::new_regenerating(config)
		} else {
			StreamGenerator::new(config)
		};
		let info = generator
			.generate(&path, &package_dir)
			.await
			.map_err(|e| ActionError::Internal(format!("Stream packaging failed: {}", e)))?;

		Ok(StreamOutput {
			manifest_path: info.manifest_path.to_string_lossy().into_owned(),
			package_dir: package_dir.to_string_lossy().into_owned(),
			rendition_count: info.rendition_count as u32,
			segment_count: info.segment_count as u32,
			total_size_bytes: info.total_size_bytes,
		})
	}

	fn action_kind(&self) -> &'static str {
		"media.stream"
	}
}

crate::register_library_action!(StreamAction, "media.stream");
