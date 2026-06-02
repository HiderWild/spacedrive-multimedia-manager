//! Single-file rotate action.
//!
//! Mirrors [`TranscodeAction`][crate::ops::media::transcode::TranscodeAction]: a
//! library action invoked over RPC that rotates one entry (looked up by UUID) in
//! place. It reuses the same [`rotate_file`] helper the batch job uses, so the
//! action and the job stay behaviorally identical for a single file. When the
//! `ffmpeg` feature is enabled it dispatches a thumbnail regeneration pass for
//! the rewritten file.

use super::{config::RotateOp, transform::rotate_file};
use crate::{
	context::CoreContext,
	infra::action::{error::ActionError, LibraryAction},
	ops::indexing::path_resolver::PathResolver,
};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;

/// Rotate a single image entry in place.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type)]
pub struct RotateInput {
	/// UUID of the entry to rotate.
	pub entry_uuid: Uuid,
	/// Transform to apply.
	pub op: RotateOp,
	/// Regenerate the file's thumbnail after rotating (requires the `ffmpeg`
	/// feature; ignored otherwise).
	pub regenerate_thumbnail: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type)]
pub struct RotateOutput {
	/// Absolute path of the rewritten file.
	pub path: String,
	/// Output width after rotation.
	pub width: u32,
	/// Output height after rotation.
	pub height: u32,
	/// Output size in bytes.
	pub size_bytes: u64,
	/// Whether an ICC profile was carried through to the output.
	pub icc_preserved: bool,
	/// Whether the EXIF orientation was normalized to Top-Left.
	pub orientation_normalized: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RotateAction {
	input: RotateInput,
}

impl RotateAction {
	pub fn new(input: RotateInput) -> Self {
		Self { input }
	}
}

impl LibraryAction for RotateAction {
	type Input = RotateInput;
	type Output = RotateOutput;

	fn from_input(input: RotateInput) -> Result<Self, String> {
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
		let path_display = path.to_string_lossy().into_owned();

		let op = self.input.op;
		let info = tokio::task::spawn_blocking(move || rotate_file(&path, op))
			.await
			.map_err(|e| ActionError::Internal(format!("Rotate task panicked: {}", e)))?
			.map_err(|e| ActionError::Internal(format!("Rotate failed: {}", e)))?;

		// Pixels changed, so the cached thumbnail is stale. Best-effort regen.
		#[cfg(feature = "ffmpeg")]
		if self.input.regenerate_thumbnail {
			if let Some(uuid) = entry.uuid {
				if let Err(e) = library.generate_thumbnails(Some(vec![uuid])).await {
					tracing::warn!("Failed to dispatch thumbnail regeneration: {}", e);
				}
			}
		}

		Ok(RotateOutput {
			path: path_display,
			width: info.width,
			height: info.height,
			size_bytes: info.size_bytes,
			icc_preserved: info.icc_preserved,
			orientation_normalized: info.orientation_normalized,
		})
	}

	fn action_kind(&self) -> &'static str {
		"media.rotate"
	}
}

crate::register_library_action!(RotateAction, "media.rotate");
