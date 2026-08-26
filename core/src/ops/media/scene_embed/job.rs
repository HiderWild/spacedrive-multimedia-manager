//! Background job that drains pending `embeddings/scene` sidecars.
//!
//! For each image with status `pending`, decodes + embeds via the configured
//! backend (OpenCLIP / DINOv2 ONNX, GPU-first; histogram baseline as fallback),
//! writes the vector as MsgPack to the sidecar file on disk, and flips the
//! sidecar DB row status to `ready` (or `failed`). Clustering itself is a
//! separate pass (`cluster_scenes` in the photos extension).

use crate::infra::db::entities::{content_identity, entry, sidecar};
use crate::infra::job::prelude::*;
use crate::infra::job::traits::DynJob;
use crate::ops::indexing::PathResolver;
use crate::ops::media::derivative_queue::SCENE_EMBEDDING_VARIANT;
use crate::ops::media::scene_embed::backend::{detect_execution_device, embed_image_path};
use crate::ops::models::image_embedding::{
	backend_from_env, ImageEmbeddingBackend, ImageEmbeddingModelSpec,
};
use crate::ops::sidecar::types::{SidecarFormat, SidecarKind, SidecarStatus, SidecarVariant};
use crate::library::Library;
use sea_orm::{
	ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
	QuerySelect,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Configuration for scene embedding generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneEmbedJobConfig {
	/// Specific entry UUIDs to process. If None, drains all pending scene sidecars.
	pub entry_ids: Option<Vec<Uuid>>,
	/// Backend override. None → env `SD_SCENE_EMBED_BACKEND` or preferred default.
	pub backend: Option<String>,
	/// Max concurrent embeds (model inference is heavy; keep small).
	pub max_concurrent: usize,
	/// Run in background (no persistence / UI progress spam).
	#[serde(default)]
	pub run_in_background: bool,
}

impl Default for SceneEmbedJobConfig {
	fn default() -> Self {
		Self {
			entry_ids: None,
			backend: None,
			max_concurrent: env_usize("SD_SCENE_EMBED_MAX_CONCURRENT", 2),
			run_in_background: false,
		}
	}
}

fn env_usize(key: &str, default: usize) -> usize {
	std::env::var(key)
		.ok()
		.and_then(|v| v.parse().ok())
		.filter(|&n| n > 0)
		.unwrap_or(default)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneEmbedState {
	pub phase: SceneEmbedPhase,
	pub total: usize,
	pub processed: usize,
	pub succeeded: usize,
	pub failed: usize,
	pub skipped: usize,
	pub device: String,
	pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum SceneEmbedPhase {
	#[default]
	Discovery,
	Processing,
	Complete,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SceneEmbedJob {
	pub config: SceneEmbedJobConfig,
	#[serde(skip_serializing_if = "Option::is_none")]
	state: Option<SceneEmbedState>,
}

impl SceneEmbedJob {
	pub fn new(config: SceneEmbedJobConfig) -> Self {
		Self {
			config,
			state: None,
		}
	}

	pub fn for_entries(entry_ids: Vec<Uuid>, config: SceneEmbedJobConfig) -> Self {
		Self {
			config: SceneEmbedJobConfig {
				entry_ids: Some(entry_ids),
				..config
			},
			state: None,
		}
	}

	pub fn with_defaults() -> Self {
		Self::new(SceneEmbedJobConfig::default())
	}
}

impl Job for SceneEmbedJob {
	const NAME: &'static str = "scene_embed";
	const RESUMABLE: bool = true;
	const DESCRIPTION: Option<&'static str> =
		Some("Generate scene embeddings (CLIP/DINO) for image clustering");
}

impl DynJob for SceneEmbedJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
	fn should_persist(&self) -> bool {
		!self.config.run_in_background
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneEmbedOutput {
	pub total: usize,
	pub succeeded: usize,
	pub failed: usize,
	pub skipped: usize,
	pub device: String,
	pub duration: Duration,
	pub errors: Vec<String>,
}

impl From<SceneEmbedOutput> for JobOutput {
	fn from(o: SceneEmbedOutput) -> Self {
		JobOutput::SceneEmbedding {
			total_processed: o.total,
			success_count: o.succeeded,
			error_count: o.failed,
			skipped_count: o.skipped,
			device: o.device,
		}
	}
}

/// On-disk payload shape for a scene embedding sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneEmbeddingPayload {
	pub model: String,
	pub dims: usize,
	pub variant: String,
	pub vector: Vec<f32>,
}

#[async_trait::async_trait]
impl JobHandler for SceneEmbedJob {
	type Output = SceneEmbedOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		if self.state.is_none() {
			self.state = Some(SceneEmbedState::default());
		}
		let started = Instant::now();

		// Resolve backend
		let backend = self
			.config
			.backend
			.as_deref()
			.and_then(ImageEmbeddingBackend::from_id)
			.unwrap_or_else(backend_from_env);

		let data_dir = crate::config::default_data_dir()
			.map_err(|e| JobError::execution(format!("Failed to get data dir: {e}")))?;
		let spec = ImageEmbeddingModelSpec::resolve(&data_dir, backend);

		let preferred_device = detect_execution_device();
		ctx.log(format!(
			"SceneEmbedJob: backend={} available={} device_preference={}",
			backend.id(),
			spec.available,
			format!("{:?}", preferred_device).to_lowercase()
		));

		if backend.requires_onnx() && !spec.available {
			ctx.log(format!(
				"ONNX weights missing at {}. Place file and re-run. See docs/superpowers/specs/2026-07-12-scene-clustering-design.md",
				spec.path.display()
			));
		}

		// Discovery phase
		if self.state.as_ref().unwrap().phase == SceneEmbedPhase::Discovery {
			self.run_discovery(&ctx).await?;
			self.state.as_mut().unwrap().phase = SceneEmbedPhase::Processing;
		}

		// Processing phase
		if self.state.as_ref().unwrap().phase == SceneEmbedPhase::Processing {
			self.run_processing(&ctx, &spec).await?;
			self.state.as_mut().unwrap().phase = SceneEmbedPhase::Complete;
		}

		let s = self.state.as_ref().unwrap();
		Ok(SceneEmbedOutput {
			total: s.total,
			succeeded: s.succeeded,
			failed: s.failed,
			skipped: s.skipped,
			device: s.device.clone(),
			duration: started.elapsed(),
			errors: s.errors.clone(),
		})
	}
}

impl SceneEmbedJob {
	async fn run_discovery(&mut self, ctx: &JobContext<'_>) -> JobResult<()> {
		ctx.progress(Progress::indeterminate(
			"Discovering images pending scene embeddings",
		));

		let library = ctx.library();
		let db = library.db().conn();

		let mut q = sidecar::Entity::find()
			.filter(sidecar::Column::Kind.eq(SidecarKind::Embeddings.as_str()))
			.filter(sidecar::Column::Variant.eq(SCENE_EMBEDDING_VARIANT))
			.filter(sidecar::Column::Status.eq(SidecarStatus::Pending.as_str()));

		if let Some(ref ids) = self.config.entry_ids {
			let content_uuids: Vec<Uuid> = resolve_content_uuids_for_entries(db, ids).await?;
			q = q.filter(sidecar::Column::ContentUuid.is_in(content_uuids));
		}

		let count = q.count(db).await.map_err(|e| JobError::Database(e.to_string()))?;
		let state = self.state.as_mut().unwrap();
		state.total = count as usize;
		ctx.log(format!("Discovered {} pending scene embeddings", state.total));
		Ok(())
	}

	async fn run_processing(
		&mut self,
		ctx: &JobContext<'_>,
		spec: &ImageEmbeddingModelSpec,
	) -> JobResult<()> {
		let library = ctx.library();
		let db = library.db().conn();
		let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent.max(1)));
		let sidecar_mgr = library.core_context().get_sidecar_manager().await;
		let library_id = library.id();

		let mut q = sidecar::Entity::find()
			.filter(sidecar::Column::Kind.eq(SidecarKind::Embeddings.as_str()))
			.filter(sidecar::Column::Variant.eq(SCENE_EMBEDDING_VARIANT))
			.filter(sidecar::Column::Status.eq(SidecarStatus::Pending.as_str()));

		if let Some(ref ids) = self.config.entry_ids {
			let content_uuids: Vec<Uuid> = resolve_content_uuids_for_entries(db, ids).await?;
			q = q.filter(sidecar::Column::ContentUuid.is_in(content_uuids));
		}

		let batch_size = 64u64;
		let mut offset = 0u64;
		let mut device_str = String::new();

		loop {
			let rows = q
				.clone()
				.offset(offset)
				.limit(batch_size)
				.all(db)
				.await
				.map_err(|e| JobError::Database(e.to_string()))?;
			if rows.is_empty() {
				break;
			}

			for row in rows {
				let _permit = semaphore.acquire().await.map_err(|e| {
					JobError::execution(format!("semaphore closed: {e}"))
				})?;

				let content_uuid = row.content_uuid;

				// Resolve entry → file path via content_identity
				let entry_model = entry::Entity::find()
					.join(
						sea_orm::JoinType::InnerJoin,
						content_identity::Entity::belongs_to(entry::Entity)
							.from(content_identity::Column::Id)
							.to(entry::Column::ContentId)
							.into(),
					)
					.filter(content_identity::Column::Uuid.eq(content_uuid))
					.one(db)
					.await
					.map_err(|e| JobError::Database(e.to_string()))?;

				let Some(entry_model) = entry_model else {
					warn!(content = %content_uuid, "no entry for content; skipping scene embed");
					self.state.as_mut().unwrap().skipped += 1;
					continue;
				};

				let full_path = match PathResolver::get_full_path(db, entry_model.id).await {
					Ok(p) => p,
					Err(e) => {
						warn!(entry = entry_model.id, error = %e, "path resolve failed");
						self.state.as_mut().unwrap().skipped += 1;
						continue;
					}
				};

				if !full_path.is_file() {
					self.state.as_mut().unwrap().skipped += 1;
					continue;
				}

				// Run inference on blocking pool (ORT / image decode is sync)
				let spec_for_infer = spec.clone();
				let path_clone = full_path.clone();
				let result = tokio::task::spawn_blocking(move || {
					embed_image_path(&path_clone, &spec_for_infer)
				})
				.await
				.map_err(|e| JobError::execution(format!("join: {e}")))?;

				let state = self.state.as_mut().unwrap();
				state.processed += 1;
				match result {
					Ok((vector, _dur, device)) => {
						if device_str.is_empty() {
							device_str = format!("{:?}", device).to_lowercase();
						}

						let payload = SceneEmbeddingPayload {
							model: spec.backend.id().to_string(),
							dims: vector.len(),
							variant: SCENE_EMBEDDING_VARIANT.to_string(),
							vector,
						};

						// Write embedding to sidecar file on disk
						let write_result = write_sidecar_payload(
							&sidecar_mgr,
							&library_id,
							&content_uuid,
							&payload,
							row.id,
							db,
						)
						.await;

						match write_result {
							Ok(()) => {
								state.succeeded += 1;
								debug!(
									content = %content_uuid,
									dims = payload.dims,
									"scene embedding ready"
								);
							}
							Err(e) => {
								state.failed += 1;
								state.errors.push(format!(
									"{}: write sidecar: {e}",
									full_path.display()
								));
								mark_sidecar_failed(db, row.id, &e.to_string()).await;
							}
						}
					}
					Err(e) => {
						state.failed += 1;
						state.errors.push(format!("{}: {e}", full_path.display()));
						mark_sidecar_failed(db, row.id, &e.to_string()).await;
						warn!(content = %content_uuid, error = %e, "scene embed failed");
					}
				}

				if state.processed % 10 == 0 {
					ctx.progress(Progress::percentage(
						state.processed as f32 / state.total.max(1) as f32,
					));
				}
			}

			offset += batch_size;
		}

		let state = self.state.as_mut().unwrap();
		if device_str.is_empty() {
			device_str = format!("{:?}", detect_execution_device()).to_lowercase();
		}
		state.device = device_str;
		Ok(())
	}
}

async fn resolve_content_uuids_for_entries(
	db: &sea_orm::DatabaseConnection,
	entry_uuids: &[Uuid],
) -> JobResult<Vec<Uuid>> {
	let content_ids: Vec<i32> = entry::Entity::find()
		.filter(entry::Column::Uuid.is_in(entry_uuids.to_vec()))
		.all(db)
		.await
		.map_err(|e| JobError::Database(e.to_string()))?
		.into_iter()
		.filter_map(|e| e.content_id)
		.collect();

	let uuids = content_identity::Entity::find()
		.filter(content_identity::Column::Id.is_in(content_ids))
		.all(db)
		.await
		.map_err(|e| JobError::Database(e.to_string()))?
		.into_iter()
		.filter_map(|ci| ci.uuid)
		.collect();
	Ok(uuids)
}

async fn write_sidecar_payload(
	sidecar_mgr: &Option<Arc<crate::service::sidecar_manager::SidecarManager>>,
	library_id: &Uuid,
	content_uuid: &Uuid,
	payload: &SceneEmbeddingPayload,
	sidecar_db_id: i32,
	db: &sea_orm::DatabaseConnection,
) -> Result<(), anyhow::Error> {
	use sea_orm::ActiveValue::Set;

	// Serialize as MsgPack
	let bytes = rmp_serde::to_vec_named(payload)?;

	// Write to disk via SidecarManager path computation
	if let Some(mgr) = sidecar_mgr {
		let sidecar_path = mgr
			.compute_path(
				library_id,
				content_uuid,
				&SidecarKind::Embeddings,
				&SidecarVariant::new(SCENE_EMBEDDING_VARIANT),
				&SidecarFormat::MessagePack,
			)
			.await?;

		if let Some(parent) = sidecar_path.absolute_path.parent() {
			tokio::fs::create_dir_all(parent).await?;
		}
		tokio::fs::write(&sidecar_path.absolute_path, &bytes).await?;

		// Update DB row: status=ready, size, checksum
		let mut am: sidecar::ActiveModel = sidecar::Entity::find_by_id(sidecar_db_id)
			.one(db)
			.await?
			.ok_or_else(|| anyhow::anyhow!("sidecar row vanished"))?
			.into();
		am.status = Set(SidecarStatus::Ready.as_str().to_string());
		am.size = Set(bytes.len() as i64);
		am.rel_path = Set(
			sidecar_path
				.relative_path
				.to_string_lossy()
				.into_owned(),
		);
		am.updated_at = Set(chrono::Utc::now());
		am.update(db).await?;
	} else {
		// No SidecarManager — just update status (data lost, but row is marked)
		warn!("SidecarManager unavailable; embedding written to DB status only");
		let mut am: sidecar::ActiveModel = sidecar::Entity::find_by_id(sidecar_db_id)
			.one(db)
			.await?
			.ok_or_else(|| anyhow::anyhow!("sidecar row vanished"))?
			.into();
		am.status = Set(SidecarStatus::Ready.as_str().to_string());
		am.size = Set(bytes.len() as i64);
		am.updated_at = Set(chrono::Utc::now());
		am.update(db).await?;
	}

	Ok(())
}

async fn mark_sidecar_failed(db: &sea_orm::DatabaseConnection, sidecar_db_id: i32, error: &str) {
	use sea_orm::ActiveValue::Set;
	if let Ok(Some(row)) = sidecar::Entity::find_by_id(sidecar_db_id).one(db).await {
		let mut am: sidecar::ActiveModel = row.into();
		am.status = Set(SidecarStatus::Failed.as_str().to_string());
		am.source = Set(Some(format!("scene_embed_error: {error}")));
		am.updated_at = Set(chrono::Utc::now());
		let _ = am.update(db).await;
	}
}
