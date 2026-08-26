//! Scene embedding pipeline (multi-backend, GPU-first when enabled).
//!
//! Backends: OpenCLIP ViT-B/32, DINOv2 ViT-B/14 (ONNX via ORT), histogram baseline.
//! Inference prefers CUDA EP when feature `scene-embed-cuda` is enabled.

pub mod backend;
pub mod eval;
pub mod job;
pub mod preprocess;

pub use backend::{
	detect_execution_device, embed_image_path, ExecutionDevice, SceneEmbedError, SceneEmbedResult,
};
pub use eval::{
	evaluate_backends, EvalConfig, EvalReport, EvalSampleResult, BACKEND_COMPARE_DEFAULT,
};
pub use job::{SceneEmbedJob, SceneEmbedJobConfig, SceneEmbedOutput};
pub use preprocess::{image_to_nchw_f32, load_and_resize_rgb};
