//! Scene clustering job scaffold.
//!
//! Pipeline (host/core drains embeddings, extension consumes vectors):
//! 1. Each image gets a pending `embeddings/scene` sidecar (async derivative queue).
//! 2. A future ORT job writes OpenCLIP / DINOv2 vectors (`ready`).
//! 3. This job loads ready vectors, runs cosine DBSCAN, creates SceneCluster models.
//!
//! Clustering math is intentional pure-Rust (same as face clusters). GPU is used
//! only for the embedding stage.

use spacedrive_sdk::job;
use serde::{Deserialize, Serialize};
use spacedrive_sdk::prelude::*;
use spacedrive_sdk::types::JobResult;
use uuid::Uuid;

use crate::models::*;
use crate::utils::{dbscan_scene_clustering, SceneCluster as SceneClusterGroup};

/// Tunables for scene-space DBSCAN (OpenCLIP ViT-B/32 defaults).
#[derive(Serialize, Deserialize, Clone)]
pub struct SceneClusterConfig {
	/// Cosine distance radius (1 - cos_sim). Typical CLIP: 0.28–0.40.
	pub eps: f32,
	pub min_samples: usize,
	/// Optional text label via reverse CLIP (future).
	pub auto_label: bool,
}

impl Default for SceneClusterConfig {
	fn default() -> Self {
		Self {
			eps: 0.30,
			min_samples: 3,
			auto_label: true,
		}
	}
}

#[derive(Serialize, Deserialize, Default)]
pub struct ClusterScenesState {
	pub photo_ids: Vec<Uuid>,
	pub config: SceneClusterConfig,
	/// Completed cluster ids written this run.
	pub clusters_written: usize,
}

/// Cluster photos that already have scene embeddings.
///
/// Host must fill vectors into sidecars before this job (CLIP/DINO ORT path).
/// Until embeddings exist, the job no-ops successfully.
#[job]
pub async fn cluster_scenes(
	ctx: &JobContext,
	state: &mut ClusterScenesState,
) -> JobResult<()> {
	let mut points: Vec<(Uuid, Vec<f32>)> = Vec::new();

	for photo_id in &state.photo_ids {
		let photo = ctx.vdfs().get_entry(*photo_id).await?;
		let Some(content_uuid) = photo.content_uuid() else {
			continue;
		};

		// Expected sidecar payload once SceneEmbedJob exists:
		// { "model": "openclip-vit-b-32", "dims": 512, "vector": [f32; N] }
		if let Ok(payload) = ctx
			.read_sidecar::<SceneEmbeddingSidecar>(content_uuid, "embeddings")
			.await
		{
			if payload.variant.as_deref() == Some("scene") && !payload.vector.is_empty() {
				points.push((*photo_id, payload.vector));
			}
		}
	}

	if points.is_empty() {
		ctx.log(
			"cluster_scenes: no ready scene embeddings yet — run SceneEmbedJob first",
		);
		return Ok(());
	}

	let clusters = dbscan_scene_clustering(&points, state.config.eps, state.config.min_samples);
	state.clusters_written = 0;

	for (idx, cluster) in clusters.iter().enumerate() {
		let label = if state.config.auto_label {
			// Placeholder title; replace with zero-shot CLIP text ranking later.
			format!("Scene {}", idx + 1)
		} else {
			format!("scene_cluster_{}", idx + 1)
		};

		// Persist cluster identity as a virtual model / sidecar bundle (SDK-shaped).
		ctx.log(&format!(
			"cluster_scenes: cluster '{}' members={}",
			label,
			cluster.photo_ids.len()
		));

		for photo_id in &cluster.photo_ids {
			let photo = ctx.vdfs().get_entry(*photo_id).await?;
			ctx.vdfs()
				.add_tag(photo.metadata_id(), &format!("#scene_cluster:{}", label))
				.await?;
			ctx.vdfs()
				.update_custom_field(*photo_id, "scene_cluster_label", &label)
				.await?;
		}

		state.clusters_written += 1;
	}

	Ok(())
}

/// On-disk / sidecar shape for a scene embedding written by the host embedder.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SceneEmbeddingSidecar {
	pub model: Option<String>,
	pub dims: Option<usize>,
	pub vector: Vec<f32>,
	/// Sidecar variant key — expect `"scene"`.
	pub variant: Option<String>,
}
