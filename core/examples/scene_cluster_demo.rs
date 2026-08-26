//! Scene clustering end-to-end validation.
//!
//! Loads images from a directory, embeds each with the chosen backend
//! (histogram baseline / OpenCLIP ONNX / DINOv2 ONNX), runs cosine DBSCAN,
//! and prints cluster assignments + timing.
//!
//! Usage:
//!   cargo run --example scene_cluster_demo --features scene-embed -- \
//!       --images .bench-import/images
//!
//! With GPU:
//!   cargo run --example scene_cluster_demo --features scene-embed-cuda -- \
//!       --images .bench-import/images --backend openclip-vit-b-32
//!
//! The histogram baseline needs no model weights and validates the full
//! embed → cluster pipeline immediately.

use clap::Parser;
use sd_core::ops::media::clustering::{cosine_dbscan, cosine_similarity, DbscanParams, EmbeddingPoint};
use sd_core::ops::media::scene_embed::backend::{detect_execution_device, embed_image_path};
use sd_core::ops::models::image_embedding::{ImageEmbeddingBackend, ImageEmbeddingModelSpec};
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;

#[derive(Parser)]
struct Args {
	/// Directory containing images to cluster.
	#[arg(long)]
	images: PathBuf,

	/// Backend: histogram-baseline | openclip-vit-b-32 | dinov2-vit-b-14
	#[arg(long, default_value = "histogram-baseline")]
	backend: String,

	/// Data dir for model lookup (default: ~/.spacedrive).
	#[arg(long)]
	data_dir: Option<PathBuf>,

	/// DBSCAN eps (cosine distance).
	#[arg(long, default_value_t = 0.30)]
	eps: f32,

	/// DBSCAN min_samples.
	#[arg(long, default_value_t = 2)]
	min_samples: usize,
}

fn main() -> anyhow::Result<()> {
	let args = Args::parse();

	// Collect images
	let mut image_paths: Vec<PathBuf> = Vec::new();
	for ext in &["jpg", "jpeg", "png", "webp", "heic", "heif"] {
		let pattern = format!("*.{}", ext);
		for entry in std::fs::read_dir(&args.images)? {
			let entry = entry?;
			let path = entry.path();
			if path
				.extension()
				.map(|e| e.to_string_lossy().to_lowercase() == *ext)
				.unwrap_or(false)
			{
				image_paths.push(path);
			}
			let _ = pattern;
		}
	}
	image_paths.sort();

	if image_paths.is_empty() {
		anyhow::bail!("No images found in {}", args.images.display());
	}

	println!("=== Scene Clustering Demo ===");
	println!("Images:    {}", image_paths.len());
	println!("Backend:   {}", args.backend);
	println!("DBSCAN:    eps={}, min_samples={}", args.eps, args.min_samples);
	println!();

	// Resolve backend
	let backend = ImageEmbeddingBackend::from_id(&args.backend)
		.ok_or_else(|| anyhow::anyhow!("Unknown backend: {}", args.backend))?;

	let data_dir = args.data_dir.unwrap_or_else(|| {
		dirs::home_dir()
			.unwrap_or_else(|| PathBuf::from("."))
			.join(".spacedrive")
	});

	let spec = ImageEmbeddingModelSpec::resolve(&data_dir, backend);

	println!(
		"Model:     {} (available={}, dims={})",
		backend.display_name(),
		spec.available,
		backend.dims()
	);
	println!(
		"Device:    preferred={:?}",
		detect_execution_device()
	);
	if backend.requires_onnx() && !spec.available {
		println!(
			"WARNING:   ONNX missing at {}. Place weights to enable GPU inference.",
			spec.path.display()
		);
		println!();
	}

	// Embed all images
	println!("--- Embedding phase ---");
	let mut points: Vec<EmbeddingPoint> = Vec::new();
	let mut total_embed_time = std::time::Duration::ZERO;
	let mut device_used = String::new();

	for (i, path) in image_paths.iter().enumerate() {
		let start = Instant::now();
		match embed_image_path(path, &spec) {
			Ok((vector, dur, device)) => {
				total_embed_time += dur;
				if device_used.is_empty() {
					device_used = format!("{:?}", device).to_lowercase();
				}
				println!(
					"  [{}/{}] {} → {}d, {:.1}ms ({:?})",
					i + 1,
					image_paths.len(),
					path.file_name().unwrap_or_default().to_string_lossy(),
					vector.len(),
					dur.as_secs_f64() * 1000.0,
					device,
				);
				points.push(EmbeddingPoint {
					id: Uuid::from_u128((i + 1) as u128),
					vector,
				});
			}
			Err(e) => {
				println!(
					"  [{}/{}] {} → FAILED: {}",
					i + 1,
					image_paths.len(),
					path.file_name().unwrap_or_default().to_string_lossy(),
					e
				);
			}
		}
	}

	if points.is_empty() {
		anyhow::bail!("No images embedded successfully");
	}

	println!();
	println!(
		"Embed total: {:.1}ms, avg {:.1}ms/image, device={}",
		total_embed_time.as_secs_f64() * 1000.0,
		total_embed_time.as_secs_f64() * 1000.0 / points.len() as f64,
		device_used,
	);
	println!();

	// Cluster
	println!("--- Clustering phase (cosine DBSCAN) ---");
	let params = DbscanParams {
		eps: args.eps,
		min_samples: args.min_samples,
	};
	let cluster_start = Instant::now();
	let clusters = cosine_dbscan(&points, params);
	let cluster_time = cluster_start.elapsed();

	println!(
		"Clusters: {} (in {:.1}ms, eps={}, min_samples={})",
		clusters.len(),
		cluster_time.as_secs_f64() * 1000.0,
		params.eps,
		params.min_samples,
	);
	println!();

	// Print cluster assignments
	println!("--- Cluster assignments ---");
	for (ci, cluster) in clusters.iter().enumerate() {
		println!("Cluster {} ({} members):", ci + 1, cluster.members.len());
		for member_id in &cluster.members {
			let idx = member_id.as_u128() as usize - 1;
			if idx < image_paths.len() {
				println!(
					"  {}",
					image_paths[idx]
						.file_name()
						.unwrap_or_default()
						.to_string_lossy()
				);
			}
		}
	}

	// Print pairwise similarity matrix (for small datasets)
	if points.len() <= 20 {
		println!();
		println!("--- Pairwise cosine similarity ---");
		print!("          ");
		for j in 0..points.len() {
			print!("{:>10}", format!("img{}", j));
		}
		println!();
		for i in 0..points.len() {
			print!("img{}  ", i);
			for j in 0..points.len() {
				let sim = cosine_similarity(&points[i].vector, &points[j].vector);
				print!("{:>10.3}", sim);
			}
			println!();
		}
	}

	// Noise points (not in any cluster)
	let clustered: std::collections::HashSet<Uuid> = clusters
		.iter()
		.flat_map(|c| c.members.iter().copied())
		.collect();
	let noise: Vec<_> = points
		.iter()
		.filter(|p| !clustered.contains(&p.id))
		.collect();
	if !noise.is_empty() {
		println!();
		println!("Noise (not in any cluster): {} images", noise.len());
		for p in &noise {
			let idx = p.id.as_u128() as usize - 1;
			if idx < image_paths.len() {
				println!(
					"  {}",
					image_paths[idx]
						.file_name()
						.unwrap_or_default()
						.to_string_lossy()
				);
			}
		}
	}

	println!();
	println!("=== Validation {} ===", if clusters.len() > 0 { "PASSED" } else { "NO CLUSTERS" });
	Ok(())
}
