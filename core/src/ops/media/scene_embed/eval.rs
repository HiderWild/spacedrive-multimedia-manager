//! Horizontal evaluation of scene-embedding backends (quality vs cost).
//!
//! For each backend that is available (ONNX on disk or baseline), measures:
//! - mean/p95 embed latency
//! - execution device (CUDA/CPU/baseline)
//! - clustering coherence proxy on labeled sample sets (when labels provided)
//!
//! Use from CLI/scripts after placing models under `models/image_embedding/`.

use super::backend::{detect_execution_device, embed_image_path, ExecutionDevice};
use crate::ops::media::clustering::{cosine_dbscan, cosine_similarity, DbscanParams, EmbeddingPoint};
use crate::ops::models::image_embedding::{
	ImageEmbeddingBackend, ImageEmbeddingModelSpec,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

/// Default backends to compare in an eval pass.
pub const BACKEND_COMPARE_DEFAULT: &[ImageEmbeddingBackend] = &[
	ImageEmbeddingBackend::OpenClipVitB32,
	ImageEmbeddingBackend::DinoV2VitB14,
	ImageEmbeddingBackend::HistogramBaseline,
];

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EvalConfig {
	/// Image paths to embed.
	pub images: Vec<PathBuf>,
	/// Optional ground-truth cluster ids aligned with `images` (same length).
	pub labels: Option<Vec<u32>>,
	/// Data dir for model lookup.
	pub data_dir: PathBuf,
	/// Backends to try (missing ONNX files are reported skipped).
	pub backends: Vec<String>,
	pub dbscan_eps: f32,
	pub dbscan_min_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EvalSampleResult {
	pub path: String,
	pub latency_ms: f64,
	pub ok: bool,
	pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EvalReport {
	pub backend: String,
	pub available: bool,
	pub device: ExecutionDevice,
	pub preferred_device: ExecutionDevice,
	pub mean_latency_ms: f64,
	pub p95_latency_ms: f64,
	pub samples_ok: usize,
	pub samples_failed: usize,
	pub cluster_count: Option<usize>,
	/// Fraction of nearest-neighbor pairs that share the same ground-truth label.
	pub nn_label_accuracy: Option<f64>,
	pub samples: Vec<EvalSampleResult>,
	pub notes: String,
}

/// Run embed + clustering metrics for each backend.
pub fn evaluate_backends(config: &EvalConfig) -> Vec<EvalReport> {
	let preferred = detect_execution_device();
	let backends: Vec<ImageEmbeddingBackend> = if config.backends.is_empty() {
		BACKEND_COMPARE_DEFAULT.to_vec()
	} else {
		config
			.backends
			.iter()
			.filter_map(|s| ImageEmbeddingBackend::from_id(s))
			.collect()
	};

	let mut reports = Vec::new();
	for backend in backends {
		reports.push(eval_one(config, backend, preferred));
	}
	reports
}

fn eval_one(
	config: &EvalConfig,
	backend: ImageEmbeddingBackend,
	preferred: ExecutionDevice,
) -> EvalReport {
	let spec = ImageEmbeddingModelSpec::resolve(&config.data_dir, backend);
	if backend.requires_onnx() && !spec.available {
		return EvalReport {
			backend: backend.id().to_string(),
			available: false,
			device: preferred,
			preferred_device: preferred,
			mean_latency_ms: 0.0,
			p95_latency_ms: 0.0,
			samples_ok: 0,
			samples_failed: 0,
			cluster_count: None,
			nn_label_accuracy: None,
			samples: Vec::new(),
			notes: format!(
				"ONNX missing at {} — place weights to enable GPU/CPU eval",
				spec.path.display()
			),
		};
	}

	let mut latencies = Vec::new();
	let mut samples = Vec::new();
	let mut points: Vec<EmbeddingPoint> = Vec::new();
	let mut device_seen = ExecutionDevice::Unknown;
	let mut ok = 0usize;
	let mut failed = 0usize;

	for (i, path) in config.images.iter().enumerate() {
		match embed_image_path(path, &spec) {
			Ok((vector, dur, device)) => {
				device_seen = device;
				let ms = dur.as_secs_f64() * 1000.0;
				latencies.push(ms);
				samples.push(EvalSampleResult {
					path: path.display().to_string(),
					latency_ms: ms,
					ok: true,
					error: None,
				});
				points.push(EmbeddingPoint {
					id: Uuid::from_u128(i as u128 + 1),
					vector,
				});
				ok += 1;
			}
			Err(e) => {
				failed += 1;
				samples.push(EvalSampleResult {
					path: path.display().to_string(),
					latency_ms: 0.0,
					ok: false,
					error: Some(e.to_string()),
				});
			}
		}
	}

	latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
	let mean = if latencies.is_empty() {
		0.0
	} else {
		latencies.iter().sum::<f64>() / latencies.len() as f64
	};
	let p95 = if latencies.is_empty() {
		0.0
	} else {
		let idx = ((latencies.len() as f64) * 0.95).floor() as usize;
		latencies[idx.min(latencies.len() - 1)]
	};

	let params = DbscanParams {
		eps: config.dbscan_eps,
		min_samples: config.dbscan_min_samples,
	};
	let clusters = if points.len() >= params.min_samples {
		Some(cosine_dbscan(&points, params).len())
	} else {
		None
	};

	let nn_label_accuracy = config.labels.as_ref().and_then(|labels| {
		if labels.len() != config.images.len() || points.len() < 2 {
			return None;
		}
		// Map point index (1-based id) to label
		let mut correct = 0usize;
		let mut total = 0usize;
		for i in 0..points.len() {
			let mut best_j = None;
			let mut best_sim = -2.0f32;
			for j in 0..points.len() {
				if i == j {
					continue;
				}
				let s = cosine_similarity(&points[i].vector, &points[j].vector);
				if s > best_sim {
					best_sim = s;
					best_j = Some(j);
				}
			}
			if let Some(j) = best_j {
				// ids were i+1
				let li = labels.get(i).copied();
				let lj = labels.get(j).copied();
				if let (Some(a), Some(b)) = (li, lj) {
					total += 1;
					if a == b {
						correct += 1;
					}
				}
			}
		}
		if total == 0 {
			None
		} else {
			Some(correct as f64 / total as f64)
		}
	});

	EvalReport {
		backend: backend.id().to_string(),
		available: true,
		device: device_seen,
		preferred_device: preferred,
		mean_latency_ms: mean,
		p95_latency_ms: p95,
		samples_ok: ok,
		samples_failed: failed,
		cluster_count: clusters,
		nn_label_accuracy,
		samples,
		notes: match (backend, device_seen) {
			(ImageEmbeddingBackend::OpenClipVitB32, ExecutionDevice::Cuda) => {
				"Primary route: GPU OpenCLIP".into()
			}
			(ImageEmbeddingBackend::DinoV2VitB14, ExecutionDevice::Cuda) => {
				"Quality route: GPU DINOv2".into()
			}
			(_, ExecutionDevice::Cpu) => {
				"Running on CPU — enable scene-embed-cuda + NVIDIA runtime for GPU".into()
			}
			(ImageEmbeddingBackend::HistogramBaseline, _) => {
				"Baseline only — not for production clustering".into()
			}
			_ => String::new(),
		},
	}
}

/// Write reports as pretty JSON.
pub fn write_report_json(path: &Path, reports: &[EvalReport]) -> std::io::Result<()> {
	let s = serde_json::to_string_pretty(reports)
		.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
	std::fs::write(path, s)
}

/// Human-readable summary table.
pub fn format_summary(reports: &[EvalReport]) -> String {
	let mut out = String::from(
		"backend                 | avail | device | mean_ms | p95_ms | ok/fail | clusters | nn_acc\n",
	);
	out.push_str(
		"------------------------+-------+--------+---------+--------+---------+----------+-------\n",
	);
	for r in reports {
		out.push_str(&format!(
			"{:<23} | {:^5} | {:<6} | {:>7.1} | {:>6.1} | {:>3}/{:<3} | {:>8} | {}\n",
			r.backend,
			if r.available { "yes" } else { "no" },
			format!("{:?}", r.device).to_lowercase(),
			r.mean_latency_ms,
			r.p95_latency_ms,
			r.samples_ok,
			r.samples_failed,
			r.cluster_count
				.map(|c| c.to_string())
				.unwrap_or_else(|| "-".into()),
			r.nn_label_accuracy
				.map(|a| format!("{:.2}", a))
				.unwrap_or_else(|| "-".into()),
		));
	}
	out
}
