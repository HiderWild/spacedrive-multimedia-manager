use spacedrive_sdk::prelude::*;
use uuid::Uuid;

use crate::agent::PhotoEvent;
use crate::models::*;

/// Face clustering in embedding space (cosine DBSCAN).
///
/// Mirrors core `ops::media::clustering` so WASM builds stay self-contained.
/// Prefer porting the core routine when host linkage is available.
pub fn dbscan_clustering(faces: &[(Uuid, FaceDetection)], threshold: f32) -> Vec<FaceCluster> {
	let points: Vec<(Uuid, Vec<f32>)> = faces
		.iter()
		.filter(|(_, f)| !f.embedding.is_empty())
		.map(|(id, f)| (*id, f.embedding.clone()))
		.collect();

	// Historical config stores similarity threshold; convert to cosine distance eps.
	let eps = (1.0 - threshold.clamp(0.0, 1.0)).clamp(0.05, 0.9);
	let clusters = cosine_dbscan(&points, eps, 2);

	clusters
		.into_iter()
		.map(|c| {
			let mut faces_out = Vec::new();
			for id in &c.members {
				if let Some((_, face)) = faces.iter().find(|(pid, _)| pid == id) {
					faces_out.push((*id, face.clone()));
				}
			}
			FaceCluster {
				faces: faces_out,
				centroid_embedding: c.centroid,
			}
		})
		.collect()
}

/// Scene clustering over whole-image embeddings (CLIP / DINOv2).
pub fn dbscan_scene_clustering(
	points: &[(Uuid, Vec<f32>)],
	eps: f32,
	min_samples: usize,
) -> Vec<SceneCluster> {
	cosine_dbscan(points, eps, min_samples)
		.into_iter()
		.map(|c| SceneCluster {
			photo_ids: c.members,
			centroid: c.centroid,
		})
		.collect()
}

pub fn cluster_by_location(photos: &[Entry], _radius_meters: f32) -> Vec<PlaceCluster> {
	// Geographic clustering remains a separate axis from scene embeddings.
	let _ = photos;
	Vec::new()
}

pub fn cluster_into_moments(_events: &[PhotoEvent]) -> Vec<MomentGroup> {
	Vec::new()
}

pub struct FaceCluster {
	pub faces: Vec<(Uuid, FaceDetection)>,
	pub centroid_embedding: Vec<f32>,
}

pub struct PlaceCluster {
	pub photos: Vec<Entry>,
	pub center: GpsCoordinates,
}

/// Scene visual cluster (embedding-space, not Places365 labels).
pub struct SceneCluster {
	pub photo_ids: Vec<Uuid>,
	pub centroid: Vec<f32>,
}

struct InternalCluster {
	members: Vec<Uuid>,
	centroid: Vec<f32>,
}

fn l2_normalize(v: &mut [f32]) {
	let mut sum = 0.0f32;
	for x in v.iter() {
		sum += *x * *x;
	}
	let norm = sum.sqrt();
	if norm > 1e-12 {
		for x in v.iter_mut() {
			*x /= norm;
		}
	}
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
	if a.len() != b.len() || a.is_empty() {
		return 1.0;
	}
	let mut dot = 0.0f32;
	for i in 0..a.len() {
		dot += a[i] * b[i];
	}
	1.0 - dot.clamp(-1.0, 1.0)
}

fn cosine_dbscan(
	points: &[(Uuid, Vec<f32>)],
	eps: f32,
	min_samples: usize,
) -> Vec<InternalCluster> {
	if points.is_empty() {
		return Vec::new();
	}
	let n = points.len();
	let mut vectors: Vec<Vec<f32>> = points
		.iter()
		.map(|(_, v)| {
			let mut c = v.clone();
			l2_normalize(&mut c);
			c
		})
		.collect();
	let dim = vectors
		.iter()
		.map(|v| v.len())
		.filter(|d| *d > 0)
		.max()
		.unwrap_or(0);
	if dim == 0 {
		return Vec::new();
	}
	for v in &mut vectors {
		if v.len() != dim {
			v.clear();
		}
	}

	let neighbors: Vec<Vec<usize>> = (0..n)
		.map(|i| {
			if vectors[i].is_empty() {
				return Vec::new();
			}
			let mut nb = Vec::new();
			for j in 0..n {
				if !vectors[j].is_empty() && cosine_distance(&vectors[i], &vectors[j]) <= eps {
					nb.push(j);
				}
			}
			nb
		})
		.collect();

	let mut labels = vec![-1i32; n];
	let mut cluster_id = 0i32;
	for i in 0..n {
		if labels[i] != -1 {
			continue;
		}
		if neighbors[i].len() < min_samples {
			labels[i] = -2;
			continue;
		}
		labels[i] = cluster_id;
		let mut seed = neighbors[i].clone();
		let mut k = 0;
		while k < seed.len() {
			let j = seed[k];
			if labels[j] == -2 {
				labels[j] = cluster_id;
			}
			if labels[j] == -1 {
				labels[j] = cluster_id;
				if neighbors[j].len() >= min_samples {
					for &n_idx in &neighbors[j] {
						if !seed.contains(&n_idx) {
							seed.push(n_idx);
						}
					}
				}
			}
			k += 1;
		}
		cluster_id += 1;
	}

	let mut clusters: Vec<InternalCluster> = (0..cluster_id)
		.map(|_| InternalCluster {
			members: Vec::new(),
			centroid: vec![0.0; dim],
		})
		.collect();

	for (i, label) in labels.iter().enumerate() {
		if *label < 0 {
			continue;
		}
		let c = &mut clusters[*label as usize];
		c.members.push(points[i].0);
		if !vectors[i].is_empty() {
			for d in 0..dim {
				c.centroid[d] += vectors[i][d];
			}
		}
	}

	clusters.retain_mut(|c| {
		if c.members.is_empty() {
			return false;
		}
		let m = c.members.len() as f32;
		for x in &mut c.centroid {
			*x /= m;
		}
		l2_normalize(&mut c.centroid);
		true
	});
	clusters
}
