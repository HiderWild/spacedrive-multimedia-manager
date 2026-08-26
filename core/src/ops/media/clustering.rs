//! Generic embedding clustering (faces, scenes, …).
//!
//! Pure-Rust cosine DBSCAN so host jobs and extension WASM can share one
//! algorithm without GPU. Embedding **inference** may use CUDA via ORT;
//! clustering itself stays on CPU and is usually cheap relative to encode.

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// One sample in embedding space, tagged with a stable id (content/entry/face).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingPoint {
	pub id: Uuid,
	/// Dense embedding; will be L2-normalized before clustering.
	pub vector: Vec<f32>,
}

/// Output cluster with members and centroid in the same space as inputs.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EmbeddingCluster {
	pub members: Vec<Uuid>,
	pub centroid: Vec<f32>,
	/// Optional noise flag if a point was isolated (only used when expose_noise).
	#[serde(default)]
	pub is_noise: bool,
}

/// DBSCAN parameters in cosine-distance space (`distance = 1 - cos_sim`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub struct DbscanParams {
	/// Neighborhood radius. CLIP scene embeddings often use ~0.28–0.40;
	/// face embeddings often use ~0.35–0.55 depending on model scale.
	pub eps: f32,
	/// Minimum neighbors (including self) to form a core point.
	pub min_samples: usize,
}

impl Default for DbscanParams {
	fn default() -> Self {
		Self {
			eps: 0.32,
			min_samples: 3,
		}
	}
}

/// Face-oriented defaults (tighter when vectors are well-normalized ArcFace-style).
pub fn face_dbscan_params(threshold: f32) -> DbscanParams {
	// Config historically stores "similarity threshold"; map to cosine distance eps.
	let sim = threshold.clamp(0.0, 1.0);
	DbscanParams {
		eps: (1.0 - sim).clamp(0.05, 0.9),
		min_samples: 2,
	}
}

/// Scene embedding defaults (OpenCLIP / SigLIP style).
pub fn scene_dbscan_params() -> DbscanParams {
	DbscanParams {
		eps: 0.30,
		min_samples: 3,
	}
}

/// L2-normalize a vector in place; zeros stay zero.
pub fn l2_normalize(v: &mut [f32]) {
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

/// Cosine similarity for equal-length L2-normalized vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
	if a.len() != b.len() || a.is_empty() {
		return 0.0;
	}
	let mut dot = 0.0f32;
	for i in 0..a.len() {
		dot += a[i] * b[i];
	}
	dot.clamp(-1.0, 1.0)
}

/// Cosine distance = 1 - similarity.
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
	1.0 - cosine_similarity(a, b)
}

/// Run cosine DBSCAN. Empty / singleton inputs return empty clusters (noise omitted).
pub fn cosine_dbscan(points: &[EmbeddingPoint], params: DbscanParams) -> Vec<EmbeddingCluster> {
	if points.is_empty() {
		return Vec::new();
	}

	let n = points.len();
	let mut vectors: Vec<Vec<f32>> = points
		.iter()
		.map(|p| {
			let mut v = p.vector.clone();
			l2_normalize(&mut v);
			v
		})
		.collect();

	// Drop zero / dimension-mismatched samples from neighborhood graph.
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
				if vectors[j].is_empty() {
					continue;
				}
				if cosine_distance(&vectors[i], &vectors[j]) <= params.eps {
					nb.push(j);
				}
			}
			nb
		})
		.collect();

	// labels: -1 unvisited, -2 noise, >=0 cluster id
	let mut labels = vec![-1i32; n];
	let mut cluster_id = 0i32;

	for i in 0..n {
		if labels[i] != -1 {
			continue;
		}
		if neighbors[i].len() < params.min_samples {
			labels[i] = -2;
			continue;
		}

		// Expand cluster from core point i.
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
				if neighbors[j].len() >= params.min_samples {
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

	let mut clusters: Vec<EmbeddingCluster> = (0..cluster_id)
		.map(|_| EmbeddingCluster {
			members: Vec::new(),
			centroid: vec![0.0; dim],
			is_noise: false,
		})
		.collect();

	for (i, label) in labels.iter().enumerate() {
		if *label < 0 {
			continue;
		}
		let c = &mut clusters[*label as usize];
		c.members.push(points[i].id);
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

#[cfg(test)]
mod tests {
	use super::*;

	fn pt(id: u128, v: Vec<f32>) -> EmbeddingPoint {
		EmbeddingPoint {
			id: Uuid::from_u128(id),
			vector: v,
		}
	}

	#[test]
	fn separates_two_far_clusters() {
		// Two tight groups far apart in 2D (will be normalized).
		let points = vec![
			pt(1, vec![1.0, 0.0]),
			pt(2, vec![0.99, 0.01]),
			pt(3, vec![0.98, 0.02]),
			pt(4, vec![0.0, 1.0]),
			pt(5, vec![0.01, 0.99]),
			pt(6, vec![0.02, 0.98]),
		];
		let clusters = cosine_dbscan(
			&points,
			DbscanParams {
				eps: 0.15,
				min_samples: 2,
			},
		);
		assert_eq!(clusters.len(), 2);
		assert!(clusters.iter().all(|c| c.members.len() == 3));
	}

	#[test]
	fn empty_input() {
		assert!(cosine_dbscan(&[], DbscanParams::default()).is_empty());
	}

	#[test]
	fn face_params_map_similarity() {
		let p = face_dbscan_params(0.6);
		assert!((p.eps - 0.4).abs() < 1e-5);
	}
}
