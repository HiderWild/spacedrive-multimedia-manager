//! Multi-backend scene embedding runners (GPU-first when compiled in).

use super::preprocess::{histogram_embedding, image_to_nchw_f32, load_and_resize_rgb};
use crate::ops::media::clustering::l2_normalize;
use crate::ops::models::image_embedding::{ImageEmbeddingBackend, ImageEmbeddingModelSpec};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum SceneEmbedError {
	#[error("{0}")]
	Msg(String),
	#[error("ONNX model missing at {0} — export/place weights (see design doc)")]
	ModelMissing(String),
	#[error("scene-embed feature disabled at compile time")]
	FeatureDisabled,
}

pub type SceneEmbedResult<T> = Result<T, SceneEmbedError>;

/// Device that actually ran inference (best-effort detection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionDevice {
	Cuda,
	Cpu,
	/// Histogram path (no ORT).
	Baseline,
	Unknown,
}

/// Detect preferred execution device string for logging / eval reports.
pub fn detect_execution_device() -> ExecutionDevice {
	#[cfg(feature = "scene-embed-cuda")]
	{
		// Prefer CUDA when feature compiled in; runtime may still fall back.
		ExecutionDevice::Cuda
	}
	#[cfg(all(feature = "scene-embed", not(feature = "scene-embed-cuda")))]
	{
		ExecutionDevice::Cpu
	}
	#[cfg(not(feature = "scene-embed"))]
	{
		ExecutionDevice::Baseline
	}
}

/// Embed a single image file with the given backend.
///
/// Returns L2-normalized vector and wall time.
pub fn embed_image_path(
	path: &Path,
	spec: &ImageEmbeddingModelSpec,
) -> SceneEmbedResult<(Vec<f32>, std::time::Duration, ExecutionDevice)> {
	let start = Instant::now();
	let backend = spec.backend;

	if matches!(backend, ImageEmbeddingBackend::HistogramBaseline) {
		let size = backend.input_size();
		let img = load_and_resize_rgb(path, size).map_err(SceneEmbedError::Msg)?;
		let mut v = histogram_embedding(&img);
		l2_normalize(&mut v);
		return Ok((v, start.elapsed(), ExecutionDevice::Baseline));
	}

	if !spec.available {
		return Err(SceneEmbedError::ModelMissing(spec.path.display().to_string()));
	}

	#[cfg(feature = "scene-embed")]
	{
		let (mut v, device) = ort_embed(path, spec)?;
		l2_normalize(&mut v);
		return Ok((v, start.elapsed(), device));
	}

	#[cfg(not(feature = "scene-embed"))]
	{
		let _ = path;
		Err(SceneEmbedError::FeatureDisabled)
	}
}

#[cfg(feature = "scene-embed")]
fn ort_embed(
	path: &Path,
	spec: &ImageEmbeddingModelSpec,
) -> SceneEmbedResult<(Vec<f32>, ExecutionDevice)> {
	use ort::session::Session;
	use ort::value::TensorRef;

	let size = spec.backend.input_size();
	let img = load_and_resize_rgb(path, size).map_err(SceneEmbedError::Msg)?;
	let input = image_to_nchw_f32(&img, size);
	let pixel_shape = [1i64, 3, size as i64, size as i64];

	let mut builder = Session::builder().map_err(|e| SceneEmbedError::Msg(e.to_string()))?;

	let mut device = ExecutionDevice::Cpu;
	#[cfg(feature = "scene-embed-cuda")]
	{
		// Prefer CUDA, fall back silently to CPU if registration fails.
		use ort::ep::CUDA;
		match builder.with_execution_providers([CUDA::default().build()]) {
			Ok(b) => {
				builder = b;
				device = ExecutionDevice::Cuda;
			}
			Err(e) => {
				tracing::warn!(
					error = %e,
					"CUDA EP registration failed; scene embed falls back to CPU"
				);
				builder = e.recover();
				device = ExecutionDevice::Cpu;
			}
		}
	}

	let mut session = builder
		.commit_from_file(&spec.path)
		.map_err(|e| SceneEmbedError::Msg(format!("load ONNX: {e}")))?;

	// Inspect inputs to decide how to feed tensors.
	// CLIP has 3 inputs: input_ids (int64), pixel_values (float), attention_mask (int64).
	// Pure vision models have 1 input: pixel_values (float).
	let input_names: Vec<String> = session
		.inputs()
		.iter()
		.map(|i| i.name().to_string())
		.collect();

	// Collect output names before run() (which needs &mut self)
	let output_names: Vec<String> = session
		.outputs()
		.iter()
		.map(|o| o.name().to_string())
		.collect();

	let pixel_tensor = TensorRef::from_array_view((pixel_shape, input.as_slice()))
		.map_err(|e| SceneEmbedError::Msg(format!("pixel tensor: {e}")))?;

	let outputs = if input_names.len() >= 3
		&& input_names.iter().any(|n| n == "input_ids")
		&& input_names.iter().any(|n| n == "pixel_values")
		&& input_names.iter().any(|n| n == "attention_mask")
	{
		// CLIP-style: provide dummy text inputs (single PAD token, zeroed mask)
		let input_ids: i64 = 0;
		let attention_mask: i64 = 0;
		let text_shape = [1i64, 1];
		let ids_tensor =
			TensorRef::from_array_view((text_shape, std::slice::from_ref(&input_ids)))
				.map_err(|e| SceneEmbedError::Msg(format!("ids tensor: {e}")))?;
		let mask_tensor = TensorRef::from_array_view((
			text_shape,
			std::slice::from_ref(&attention_mask),
		))
		.map_err(|e| SceneEmbedError::Msg(format!("mask tensor: {e}")))?;

		session
			.run(ort::inputs! {
				"input_ids" => ids_tensor,
				"pixel_values" => pixel_tensor,
				"attention_mask" => mask_tensor,
			})
			.map_err(|e| SceneEmbedError::Msg(format!("infer: {e}")))?
	} else {
		// Pure vision model: single input
		let input_name = input_names
			.first()
			.cloned()
			.unwrap_or_else(|| "input".into());
		session
			.run(ort::inputs![input_name.as_str() => pixel_tensor])
			.map_err(|e| SceneEmbedError::Msg(format!("infer: {e}")))?
	};

	// Find image_embeds output (CLIP) or first output (pure vision).
	// CLIP outputs: logits_per_image, logits_per_text, text_embeds, image_embeds
	let target_name = output_names
		.iter()
		.find(|n| n == &"image_embeds")
		.or_else(|| output_names.first())
		.cloned()
		.unwrap_or_else(|| "output".to_string());

	let target = outputs
		.get(target_name.as_str())
		.ok_or_else(|| SceneEmbedError::Msg("no image_embeds output".into()))?;

	let (_shape, data) = target
		.try_extract_tensor::<f32>()
		.map_err(|e| SceneEmbedError::Msg(format!("extract: {e}")))?;

	// Output is [1, D] for CLIP image_embeds; flatten to D
	let dims = spec.backend.dims();
	let vec = if data.len() == dims {
		data.to_vec()
	} else if data.len() % dims == 0 {
		// Mean-pool over sequence dim for multi-token outputs
		let tokens = data.len() / dims;
		let mut acc = vec![0.0f32; dims];
		for t in 0..tokens {
			for d in 0..dims {
				acc[d] += data[t * dims + d];
			}
		}
		let inv = 1.0 / tokens as f32;
		for x in &mut acc {
			*x *= inv;
		}
		acc
	} else {
		// Fallback: truncate/pad to dims
		let mut acc = vec![0.0f32; dims];
		for (i, x) in data.iter().enumerate().take(dims) {
			acc[i] = *x;
		}
		acc
	};

	Ok((vec, device))
}
