//! Non-blocking derivative work queue helpers.
//!
//! Photo **add / index** paths must stay lightweight: they record content identity
//! and enqueue sidecar work (`pending`) instead of generating thumbs/faces inline.
//! Background jobs (`ThumbnailJob`, future face/embedding jobs) drain the queue.
//!
//! Status is stored on the existing `sidecar` table:
//! - `kind=thumb` + variant (e.g. `grid@1x`) → thumbnail ready/pending/failed
//! - `kind=embeddings` + variant `face` → face embedding ready/pending/failed
//! - `kind=embeddings` + variant `scene` → scene embedding (CLIP/DINO) ready/pending/failed
//!
//! Callers that need the bit "has this content a ready thumbnail?" query via
//! [`derivative_status_for_content`].

use crate::{
	infra::db::entities::{content_identity, entry, sidecar},
	library::Library,
	ops::media::ai_exclusion::{is_excluded_from_face, is_excluded_from_scene},
	ops::media::thumbnail::{ThumbnailJob, ThumbnailJobConfig, ThumbnailVariants},
	ops::sidecar::types::{
		SidecarFormat, SidecarKind, SidecarStatus, SidecarVariant,
	},
};
use anyhow::Result;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Debounce window for merging watcher-driven thumbnail jobs.
const ENQUEUE_DEBOUNCE: Duration = Duration::from_millis(400);

struct PendingBatch {
	/// Entries waiting for a thumb job (already have pending sidecar rows).
	entry_uuids: HashSet<Uuid>,
	/// Flush task already scheduled for this library.
	flush_scheduled: bool,
}

/// Global debounce buffer: library_id → pending entry UUIDs.
fn pending_batches() -> &'static Mutex<HashMap<Uuid, PendingBatch>> {
	static MAP: OnceLock<Mutex<HashMap<Uuid, PendingBatch>>> = OnceLock::new();
	MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// High-level readiness of one derivative kind for a content UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DerivativeKindStatus {
	/// No work is expected (e.g. non-image for face vectors).
	NotApplicable,
	/// No sidecar row yet (never enqueued).
	Missing,
	/// Enqueued / in progress.
	Pending,
	/// Artifact exists and is ready to serve.
	Ready,
	/// Last attempt failed (may be re-enqueued).
	Failed,
}

impl DerivativeKindStatus {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::NotApplicable => "not_applicable",
			Self::Missing => "missing",
			Self::Pending => "pending",
			Self::Ready => "ready",
			Self::Failed => "failed",
		}
	}

	/// True when the UI may treat the derivative as available.
	pub fn is_ready(self) -> bool {
		matches!(self, Self::Ready)
	}
}

/// Snapshot of common media derivatives for one content identity.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ContentDerivativeStatus {
	pub content_uuid: Uuid,
	/// Aggregated status for import default thumb variant (`grid@1x`).
	pub thumbnail: DerivativeKindStatus,
	/// Face embedding vector sidecar (`embeddings` / `face`).
	pub face_embedding: DerivativeKindStatus,
	/// Whole-image scene embedding (`embeddings` / `scene`) for clustering.
	pub scene_embedding: DerivativeKindStatus,
}

/// Canonical face embedding variant name.
pub const FACE_EMBEDDING_VARIANT: &str = "face";

/// Canonical whole-image scene embedding variant (CLIP / DINOv2 / etc.).
pub const SCENE_EMBEDDING_VARIANT: &str = "scene";

/// Ensure a pending sidecar row exists without generating the artifact.
///
/// Returns `true` when a new pending row was inserted (or a failed row was
/// reset to pending). Existing `ready` / `pending` rows are left unchanged.
pub async fn ensure_pending_sidecar(
	library: &Library,
	content_uuid: Uuid,
	kind: &SidecarKind,
	variant: &SidecarVariant,
	format: &SidecarFormat,
) -> Result<bool> {
	let db = library.db().conn();
	let existing = sidecar::Entity::find()
		.filter(sidecar::Column::ContentUuid.eq(content_uuid))
		.filter(sidecar::Column::Kind.eq(kind.as_str()))
		.filter(sidecar::Column::Variant.eq(variant.as_str()))
		.one(db)
		.await?;

	if let Some(row) = existing {
		match row.status.as_str() {
			"ready" | "pending" => return Ok(false),
			"failed" => {
				let mut active: sidecar::ActiveModel = row.into();
				active.status = ActiveValue::Set(SidecarStatus::Pending.as_str().to_string());
				active.updated_at = ActiveValue::Set(Utc::now());
				active.update(db).await?;
				return Ok(true);
			}
			_ => {
				let mut active: sidecar::ActiveModel = row.into();
				active.status = ActiveValue::Set(SidecarStatus::Pending.as_str().to_string());
				active.updated_at = ActiveValue::Set(Utc::now());
				active.update(db).await?;
				return Ok(true);
			}
		}
	}

	// Rel path may not exist yet; compute for consistency with SidecarManager.
	let rel_path = if let Some(mgr) = library.core_context().get_sidecar_manager().await {
		mgr.compute_path(&library.id(), &content_uuid, kind, variant, format)
			.await
			.map(|p| p.relative_path.to_string_lossy().into_owned())
			.unwrap_or_default()
	} else {
		String::new()
	};

	let model = sidecar::ActiveModel {
		uuid: ActiveValue::Set(Uuid::new_v4()),
		content_uuid: ActiveValue::Set(content_uuid),
		kind: ActiveValue::Set(kind.as_str().to_string()),
		variant: ActiveValue::Set(variant.as_str().to_string()),
		format: ActiveValue::Set(format.as_str().to_string()),
		rel_path: ActiveValue::Set(rel_path),
		size: ActiveValue::Set(0),
		checksum: ActiveValue::Set(None),
		status: ActiveValue::Set(SidecarStatus::Pending.as_str().to_string()),
		source: ActiveValue::Set(Some("derivative_queue".to_string())),
		version: ActiveValue::Set(1),
		created_at: ActiveValue::Set(Utc::now()),
		updated_at: ActiveValue::Set(Utc::now()),
		..Default::default()
	};
	model.insert(db).await?;
	Ok(true)
}

fn status_from_rows(
	rows: &[sidecar::Model],
	kind: &str,
	variant: &str,
) -> DerivativeKindStatus {
	let Some(row) = rows
		.iter()
		.find(|r| r.kind == kind && r.variant == variant)
	else {
		return DerivativeKindStatus::Missing;
	};
	match row.status.as_str() {
		"ready" => DerivativeKindStatus::Ready,
		"pending" => DerivativeKindStatus::Pending,
		"failed" => DerivativeKindStatus::Failed,
		_ => DerivativeKindStatus::Pending,
	}
}

/// Load derivative readiness for a content UUID.
pub async fn derivative_status_for_content(
	library: &Library,
	content_uuid: Uuid,
) -> Result<ContentDerivativeStatus> {
	let db = library.db().conn();
	let rows = sidecar::Entity::find()
		.filter(sidecar::Column::ContentUuid.eq(content_uuid))
		.all(db)
		.await?;

	let thumb_variant = ThumbnailVariants::grid_1x().variant;
	let thumbnail = status_from_rows(&rows, SidecarKind::Thumb.as_str(), thumb_variant.as_str());
	let face_embedding = status_from_rows(
		&rows,
		SidecarKind::Embeddings.as_str(),
		FACE_EMBEDDING_VARIANT,
	);
	let scene_embedding = status_from_rows(
		&rows,
		SidecarKind::Embeddings.as_str(),
		SCENE_EMBEDDING_VARIANT,
	);

	Ok(ContentDerivativeStatus {
		content_uuid,
		thumbnail,
		face_embedding,
		scene_embedding,
	})
}

/// Enqueue default import thumbnails + optional face/scene embeddings for one entry.
///
/// Does **not** generate anything. Thumbnails dispatch a background job (when
/// scheduled). Face/scene embeddings are only marked `pending` until extension
/// or core ORT jobs drain them.
pub async fn enqueue_derivatives_for_entry(
	library: &Library,
	entry_uuid: Uuid,
	mime_type: Option<&str>,
	want_face: bool,
) -> Result<()> {
	enqueue_derivatives_for_entry_ext(library, entry_uuid, mime_type, want_face, want_face).await
}

/// Like [`enqueue_derivatives_for_entry`] with separate face/scene flags.
pub async fn enqueue_derivatives_for_entry_ext(
	library: &Library,
	entry_uuid: Uuid,
	mime_type: Option<&str>,
	want_face: bool,
	want_scene: bool,
) -> Result<()> {
	let db = library.db().conn();

	let entry_model = entry::Entity::find()
		.filter(entry::Column::Uuid.eq(entry_uuid))
		.one(db)
		.await?
		.ok_or_else(|| anyhow::anyhow!("entry {entry_uuid} not found"))?;

	let content_id = entry_model
		.content_id
		.ok_or_else(|| anyhow::anyhow!("entry {entry_uuid} has no content_id yet"))?;

	let ci = content_identity::Entity::find_by_id(content_id)
		.one(db)
		.await?
		.ok_or_else(|| anyhow::anyhow!("content identity {content_id} not found"))?;

	let content_uuid = ci
		.uuid
		.ok_or_else(|| anyhow::anyhow!("content identity missing uuid"))?;

	let is_image = mime_type
		.map(|m| m.starts_with("image/"))
		.unwrap_or(false);
	let is_video = mime_type
		.map(|m| m.starts_with("video/"))
		.unwrap_or(false);
	// Thumbnail-worthy media (images/videos/pdfs handled by job filters)
	let want_thumb = is_image || is_video || mime_type == Some("application/pdf");

	if want_thumb {
		// Pending rows for each import default variant.
		for variant_cfg in ThumbnailVariants::import_defaults() {
			let _ = ensure_pending_sidecar(
				library,
				content_uuid,
				&SidecarKind::Thumb,
				&variant_cfg.variant,
				&variant_cfg.format(),
			)
			.await?;
		}

		// Coalesce per-entry watcher floods into one job per debounce window.
		// Callers that already hold `Arc<Library>` should prefer
		// [`schedule_derivative_enqueue`] so Arc can be moved into the flush task.
		debug!(
			entry = %entry_uuid,
			content = %content_uuid,
			"Thumbnail pending marked (dispatch via schedule_derivative_enqueue or batch helper)"
		);
	}

	if want_face && is_image {
		let excluded = is_excluded_from_face(db, content_uuid).await.unwrap_or(false);
		if excluded {
			debug!(
				content = %content_uuid,
				"Face embedding skipped (content excluded from face recognition)"
			);
		} else {
			let _ = ensure_pending_sidecar(
				library,
				content_uuid,
				&SidecarKind::Embeddings,
				&SidecarVariant::new(FACE_EMBEDDING_VARIANT),
				&SidecarFormat::MessagePack,
			)
			.await?;
			// Face generation is extension-driven today; pending marks work for later drain.
			info!(
				content = %content_uuid,
				"Face embedding marked pending (background / extension drain)"
			);
		}
	}

	if want_scene && is_image {
		let excluded = is_excluded_from_scene(db, content_uuid).await.unwrap_or(false);
		if excluded {
			debug!(
				content = %content_uuid,
				"Scene embedding skipped (content excluded from scene clustering)"
			);
		} else {
			let _ = ensure_pending_sidecar(
				library,
				content_uuid,
				&SidecarKind::Embeddings,
				&SidecarVariant::new(SCENE_EMBEDDING_VARIANT),
				&SidecarFormat::MessagePack,
			)
			.await?;
			info!(
				content = %content_uuid,
				"Scene embedding marked pending (CLIP/DINO job drain)"
			);
		}
	}

	Ok(())
}

/// Batch-friendly: mark pending rows + dispatch one thumbs job for many entries.
pub async fn enqueue_thumbnails_for_entries(
	library: &Library,
	entry_uuids: Vec<Uuid>,
) -> Result<()> {
	if entry_uuids.is_empty() {
		return Ok(());
	}

	let db = library.db().conn();
	let models = entry::Entity::find()
		.filter(entry::Column::Uuid.is_in(entry_uuids.clone()))
		.all(db)
		.await?;

	for entry_model in &models {
		let Some(_entry_uuid) = entry_model.uuid else {
			continue;
		};
		let Some(content_id) = entry_model.content_id else {
			continue;
		};
		let Ok(Some(ci)) = content_identity::Entity::find_by_id(content_id).one(db).await else {
			continue;
		};
		let Some(content_uuid) = ci.uuid else {
			continue;
		};
		for variant_cfg in ThumbnailVariants::import_defaults() {
			let _ = ensure_pending_sidecar(
				library,
				content_uuid,
				&SidecarKind::Thumb,
				&variant_cfg.variant,
				&variant_cfg.format(),
			)
			.await;
		}
	}

	#[cfg(feature = "ffmpeg")]
	{
		let config = ThumbnailJobConfig::default();
		let job = ThumbnailJob::for_entries(entry_uuids, config);
		library
			.jobs()
			.dispatch(job)
			.await
			.map_err(|e| anyhow::anyhow!("dispatch thumbnail batch: {e}"))?;
	}

	Ok(())
}

/// Mark pending rows immediately; coalesce job dispatch with a short debounce.
///
/// Preferred on the watcher hot path: many files added in one burst become one
/// `ThumbnailJob` instead of N tiny jobs.
pub async fn schedule_derivative_enqueue(
	library: Arc<Library>,
	entry_uuid: Uuid,
	mime_type: Option<&str>,
	want_face: bool,
) -> Result<()> {
	// Always write pending rows immediately (status visible to UI).
	enqueue_derivatives_for_entry(library.as_ref(), entry_uuid, mime_type, want_face).await?;

	let is_image = mime_type
		.map(|m| m.starts_with("image/"))
		.unwrap_or(false);
	let is_video = mime_type
		.map(|m| m.starts_with("video/"))
		.unwrap_or(false);
	let want_thumb = is_image || is_video || mime_type == Some("application/pdf");
	if !want_thumb {
		return Ok(());
	}

	#[cfg(feature = "ffmpeg")]
	{
		let library_id = library.id();
		let should_spawn = {
			let mut map = pending_batches()
				.lock()
				.unwrap_or_else(|e| e.into_inner());
			let batch = map.entry(library_id).or_insert_with(|| PendingBatch {
				entry_uuids: HashSet::new(),
				flush_scheduled: false,
			});
			batch.entry_uuids.insert(entry_uuid);
			if batch.flush_scheduled {
				false
			} else {
				batch.flush_scheduled = true;
				true
			}
		};

		if should_spawn {
			let lib = library.clone();
			tokio::spawn(async move {
				tokio::time::sleep(ENQUEUE_DEBOUNCE).await;
				let uuids = {
					let mut map = pending_batches()
						.lock()
						.unwrap_or_else(|e| e.into_inner());
					if let Some(batch) = map.get_mut(&library_id) {
						batch.flush_scheduled = false;
						batch.entry_uuids.drain().collect::<Vec<_>>()
					} else {
						Vec::new()
					}
				};
				if uuids.is_empty() {
					return;
				}
				let count = uuids.len();
				if let Err(e) = enqueue_thumbnails_for_entries(lib.as_ref(), uuids).await {
					warn!(
						library = %library_id,
						error = %e,
						"Debounced thumbnail batch dispatch failed"
					);
				} else {
					info!(
						library = %library_id,
						entries = count,
						"Debounced thumbnail batch dispatched"
					);
				}
			});
		}
	}

	Ok(())
}
