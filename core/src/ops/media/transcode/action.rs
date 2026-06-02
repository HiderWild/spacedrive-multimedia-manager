//! Single-file transcode action.
//!
//! Mirrors `GenerateProxyAction`: a library action invoked over RPC that
//! transcodes one entry (looked up by content/entry UUID) to a chosen codec and
//! container. It reuses the same [`TranscodeGenerator`] plumbing the batch job
//! uses, so the action and the job stay behaviorally identical for a single
//! file.

use super::{
	config::{
		TranscodeCodec, TranscodeConfig, TranscodeContainer, TranscodeQuality, TranscodeResolution,
	},
	generator::TranscodeGenerator,
	hwaccel::HwAccel,
};
use crate::{
	context::CoreContext,
	infra::action::{error::ActionError, LibraryAction},
	ops::indexing::path_resolver::PathResolver,
};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;

/// Transcode a single video entry to a target codec.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type)]
pub struct TranscodeInput {
	/// UUID of the entry to transcode.
	pub entry_uuid: Uuid,
	/// Target codec.
	pub codec: TranscodeCodec,
	/// Output container; defaults to the codec's preferred container.
	pub container: Option<TranscodeContainer>,
	/// Scale longest side to at most this many pixels; `None` keeps the source.
	pub max_dimension: Option<u32>,
	/// CRF quality (takes precedence over `bitrate_kbps` when both are set).
	pub crf: Option<u32>,
	/// Target bitrate in kbps.
	pub bitrate_kbps: Option<u32>,
	/// Encoder speed preset.
	pub preset: Option<String>,
	/// Hardware acceleration backend; defaults to `Auto` (best available, else CPU).
	#[serde(default)]
	pub hw_accel: Option<HwAccel>,
	/// Re-encode even when the output already exists.
	pub force: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type)]
pub struct TranscodeOutput {
	/// Absolute path of the produced file.
	pub output_path: String,
	/// Output size in bytes.
	pub size_bytes: u64,
	/// Encoding wall-clock time in seconds.
	pub encoding_time_secs: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscodeAction {
	input: TranscodeInput,
}

impl TranscodeAction {
	pub fn new(input: TranscodeInput) -> Self {
		Self { input }
	}

	fn build_config(&self) -> TranscodeConfig {
		let container = self
			.input
			.container
			.unwrap_or_else(|| self.input.codec.default_container());

		let resolution = match self.input.max_dimension {
			Some(dim) => TranscodeResolution::MaxDimension(dim),
			None => TranscodeResolution::Keep,
		};

		let quality = match (self.input.crf, self.input.bitrate_kbps) {
			(Some(crf), _) => TranscodeQuality::Crf(crf),
			(None, Some(kbps)) => TranscodeQuality::Bitrate(kbps),
			(None, None) => TranscodeQuality::default(),
		};

		TranscodeConfig {
			codec: self.input.codec,
			container,
			resolution,
			quality,
			preset: self
				.input
				.preset
				.clone()
				.unwrap_or_else(|| "veryfast".into()),
			use_hardware_accel: false,
			hw_accel: self.input.hw_accel.unwrap_or_default(),
		}
	}
}

impl LibraryAction for TranscodeAction {
	type Input = TranscodeInput;
	type Output = TranscodeOutput;

	fn from_input(input: TranscodeInput) -> Result<Self, String> {
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

		// Stable output name from content UUID when available.
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

		let output_dir = library.path().join("transcodes");
		let stem = content_uuid
			.map(|u| u.to_string())
			.unwrap_or_else(|| format!("entry_{}", entry.id));
		let output_path = output_dir.join(format!("{}.{}", stem, config.extension()));

		if !self.input.force && output_path.exists() {
			return Err(ActionError::Internal(format!(
				"Output already exists: {} (set force to overwrite)",
				output_path.display()
			)));
		}

		let start = std::time::Instant::now();
		let generator = TranscodeGenerator::new(config);
		let info = generator
			.generate(&path, &output_path)
			.await
			.map_err(|e| ActionError::Internal(format!("Transcode failed: {}", e)))?;

		Ok(TranscodeOutput {
			output_path: output_path.to_string_lossy().into_owned(),
			size_bytes: info.size_bytes,
			encoding_time_secs: start.elapsed().as_secs(),
		})
	}

	fn action_kind(&self) -> &'static str {
		"media.transcode"
	}
}

crate::register_library_action!(TranscodeAction, "media.transcode");
