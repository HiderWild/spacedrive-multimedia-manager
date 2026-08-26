//! Thumbnail generation system
//!
//! This module provides thumbnail generation capabilities for various media types
//! including images, videos, and documents. It operates as a separate job that
//! can run independently or be triggered after indexing operations.

pub mod action;
mod config;
mod error;
mod generator;
mod job;
pub mod processor;
mod progress;
mod state;
mod utils;

pub use action::ThumbnailAction;
pub use config::{ThumbnailVariantConfig, ThumbnailVariants};
pub use error::{ThumbnailError, ThumbnailResult};
pub use generator::{ImageGenerator, ThumbnailGenerator, ThumbnailInfo, ThumbnailTarget, VideoGenerator};
pub use job::{ThumbnailJob, ThumbnailJobConfig};
pub use processor::ThumbnailProcessor;
pub use state::{ThumbnailEntry, ThumbnailPhase, ThumbnailState, ThumbnailStats};
pub use utils::ThumbnailUtils;

use crate::library::Library;
use crate::ops::sidecar::types::SidecarKind;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// Generate thumbnails for a single file (optimized for watcher/responder use)
///
/// This function generates the default thumbnail variants for a single file
/// and registers them as sidecars. It's designed to be called inline for
/// individual file creates/updates rather than dispatching a job.
///
/// # Arguments
/// * `library` - The library context
/// * `content_uuid` - The UUID of the content identity
/// * `source_path` - The filesystem path to the source file
/// * `mime_type` - The MIME type of the file
///
/// # Returns
/// Number of thumbnails successfully generated
pub async fn generate_thumbnails_for_file(
	library: &Arc<Library>,
	content_uuid: &Uuid,
	source_path: &Path,
	mime_type: &str,
) -> ThumbnailResult<usize> {
	use tracing::{debug, warn};

	// Check if thumbnail generation is supported for this file type
	if !ThumbnailUtils::is_thumbnail_supported(mime_type) {
		debug!(
			"Thumbnail generation not supported for MIME type: {}",
			mime_type
		);
		return Ok(0);
	}

	// Get sidecar manager
	let sidecar_manager = library
		.core_context()
		.get_sidecar_manager()
		.await
		.ok_or_else(|| ThumbnailError::other("SidecarManager not available"))?;

	// Create thumbnail generator for this MIME type
	let generator = ThumbnailGenerator::for_mime_type(mime_type)?;

	// Single grid size keeps bulk import memory bounded.
	let variants = ThumbnailVariants::import_defaults();
	let mut generated_count = 0;

	let mut pending = Vec::new();
	for variant_config in &variants {
		if sidecar_manager
			.exists(
				&library.id(),
				content_uuid,
				&SidecarKind::Thumb,
				&variant_config.variant,
				&variant_config.format(),
			)
			.await
			.unwrap_or(false)
		{
			debug!(
				"Thumbnail already exists for {}: {}",
				content_uuid,
				variant_config.variant.as_str()
			);
			continue;
		}

		let output_path = sidecar_manager
			.compute_path(
				&library.id(),
				content_uuid,
				&SidecarKind::Thumb,
				&variant_config.variant,
				&variant_config.format(),
			)
			.await
			.map_err(|e| ThumbnailError::other(format!("Failed to compute path: {}", e)))?;

		pending.push((variant_config, output_path.absolute_path));
	}

	if !pending.is_empty() {
		let targets: Vec<ThumbnailTarget> = pending
			.iter()
			.map(|(vc, path)| ThumbnailTarget {
				output_path: path.clone(),
				size: vc.size,
				quality: vc.quality,
			})
			.collect();

		match generator.generate_many(source_path, &targets).await {
			Ok(infos) => {
				for ((variant_config, _), thumbnail_info) in pending.iter().zip(infos.into_iter()) {
					if let Err(e) = sidecar_manager
						.record_sidecar(
							library,
							content_uuid,
							&SidecarKind::Thumb,
							&variant_config.variant,
							&variant_config.format(),
							thumbnail_info.size_bytes as u64,
							None,
						)
						.await
					{
						warn!(
							"Failed to record sidecar for {}: {}",
							variant_config.variant.as_str(),
							e
						);
					} else {
						debug!(
							"✓ Generated thumbnail {}: {}x{}",
							variant_config.variant.as_str(),
							thumbnail_info.dimensions.0,
							thumbnail_info.dimensions.1
						);
						generated_count += 1;
					}
				}
			}
			Err(e) => {
				warn!(
					"Failed to generate thumbnails for {}: {}",
					source_path.display(),
					e
				);
			}
		}
	}

	Ok(generated_count)
}
