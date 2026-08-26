//! Catalog of whole-image embedding models for scene clustering.
//!
//! Primary routes (effect-first, local, GPU via ORT CUDA):
//! - OpenCLIP ViT-B/32 (export to ONNX, ~150–350MB)
//! - DINOv2 ViT-B/14 (export to ONNX)
//! - Histogram baseline (no external weights) for offline pipeline/cluster tests
//!
//! Place ONNX files under `{data_dir}/models/image_embedding/`.

use super::types::{ModelInfo, ModelProvider, ModelType};
use std::path::{Path, PathBuf};

/// Which embedder backend to run (and evaluate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ImageEmbeddingBackend {
	/// OpenCLIP ViT-B/32 vision tower (best default for cluster + text search).
	OpenClipVitB32,
	/// DINOv2 ViT-B/14 (strong pure-vision clusters).
	DinoV2VitB14,
	/// Cheap fixed histogram / spatial-RGB fingerprint (no ONNX) for baseline eval.
	HistogramBaseline,
}

impl ImageEmbeddingBackend {
	pub fn id(self) -> &'static str {
		match self {
			Self::OpenClipVitB32 => "openclip-vit-b-32",
			Self::DinoV2VitB14 => "dinov2-vit-b-14",
			Self::HistogramBaseline => "histogram-baseline",
		}
	}

	pub fn display_name(self) -> &'static str {
		match self {
			Self::OpenClipVitB32 => "OpenCLIP ViT-B/32",
			Self::DinoV2VitB14 => "DINOv2 ViT-B/14",
			Self::HistogramBaseline => "Histogram baseline (no ML)",
		}
	}

	pub fn filename(self) -> &'static str {
		match self {
			Self::OpenClipVitB32 => "openclip-vit-b-32.onnx",
			Self::DinoV2VitB14 => "dinov2-vit-b-14.onnx",
			Self::HistogramBaseline => "", // no weights
		}
	}

	/// Expected embedding dimensionality after pooling.
	pub fn dims(self) -> usize {
		match self {
			Self::OpenClipVitB32 => 512,
			Self::DinoV2VitB14 => 768,
			Self::HistogramBaseline => 192, // 64 bins × RGB
		}
	}

	/// Input spatial size (square) expected by the vision backbone.
	pub fn input_size(self) -> u32 {
		match self {
			Self::OpenClipVitB32 => 224,
			Self::DinoV2VitB14 => 224,
			Self::HistogramBaseline => 128,
		}
	}

	/// Optional HuggingFace export hint (operator-provided ONNX).
	pub fn download_hint(self) -> Option<&'static str> {
		match self {
			Self::OpenClipVitB32 => {
				Some("Export OpenCLIP ViT-B-32 vision tower to ONNX and place as openclip-vit-b-32.onnx")
			}
			Self::DinoV2VitB14 => {
				Some("Export DINOv2 ViT-B/14 to ONNX and place as dinov2-vit-b-14.onnx")
			}
			Self::HistogramBaseline => None,
		}
	}

	pub fn requires_onnx(self) -> bool {
		!matches!(self, Self::HistogramBaseline)
	}

	pub fn all() -> &'static [Self] {
		&[
			Self::OpenClipVitB32,
			Self::DinoV2VitB14,
			Self::HistogramBaseline,
		]
	}

	/// Default for production quality.
	pub fn preferred() -> Self {
		Self::OpenClipVitB32
	}

	pub fn from_id(id: &str) -> Option<Self> {
		match id {
			"openclip-vit-b-32" | "openclip" | "clip" => Some(Self::OpenClipVitB32),
			"dinov2-vit-b-14" | "dinov2" | "dino" => Some(Self::DinoV2VitB14),
			"histogram-baseline" | "histogram" | "baseline" => Some(Self::HistogramBaseline),
			_ => None,
		}
	}

	pub fn to_model_info(self, downloaded: bool) -> ModelInfo {
		ModelInfo {
			id: self.id().to_string(),
			name: self.display_name().to_string(),
			model_type: ModelType::ImageEmbedding,
			size_bytes: match self {
				Self::OpenClipVitB32 => 350 * 1024 * 1024,
				Self::DinoV2VitB14 => 330 * 1024 * 1024,
				Self::HistogramBaseline => 0,
			},
			provider: ModelProvider::Direct {
				url: self.download_hint().unwrap_or("").to_string(),
			},
			filename: self.filename().to_string(),
			downloaded,
			description: Some(match self {
				Self::OpenClipVitB32 => {
					"Best default for scene clusters + optional text query. GPU via ORT CUDA."
						.to_string()
				}
				Self::DinoV2VitB14 => {
					"Highest pure-vision clustering quality. GPU via ORT CUDA.".to_string()
				}
				Self::HistogramBaseline => {
					"No weights; structural RGB histogram for pipeline/eval baseline only."
						.to_string()
				}
			}),
		}
	}
}

/// Spec resolved against a data directory.
#[derive(Debug, Clone)]
pub struct ImageEmbeddingModelSpec {
	pub backend: ImageEmbeddingBackend,
	pub path: PathBuf,
	pub available: bool,
}

impl ImageEmbeddingModelSpec {
	pub fn resolve(data_dir: &Path, backend: ImageEmbeddingBackend) -> Self {
		if !backend.requires_onnx() {
			return Self {
				backend,
				path: PathBuf::new(),
				available: true,
			};
		}
		let path = get_image_embedding_models_dir(data_dir).join(backend.filename());
		let available = path.is_file();
		Self {
			backend,
			path,
			available,
		}
	}
}

pub fn get_image_embedding_models_dir(data_dir: &Path) -> PathBuf {
	super::get_models_dir(data_dir).join("image_embedding")
}

/// Env override: `SD_SCENE_EMBED_BACKEND=openclip|dinov2|histogram`
pub fn backend_from_env() -> ImageEmbeddingBackend {
	std::env::var("SD_SCENE_EMBED_BACKEND")
		.ok()
		.and_then(|s| ImageEmbeddingBackend::from_id(&s))
		.unwrap_or_else(ImageEmbeddingBackend::preferred)
}
