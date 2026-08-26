//! Media processing operations
//!
//! This module contains jobs for processing media files including:
//! - Thumbnail generation
//! - OCR (text extraction from images/PDFs)
//! - Speech-to-text (audio/video transcription)
//! - Gaussian splat generation (3D view synthesis from images)
//! - Video transcoding
//! - Audio metadata extraction
//! - Image optimization
//! - Blurhash generation for image placeholders

pub mod blurhash;
pub mod ai_exclusion;
pub mod clustering;
pub mod derivative_queue;
pub mod derivative_status_query;
pub mod ffmpeg_bin;
pub mod metadata_extractor;
pub mod ocr;
pub mod proxy;
pub mod rotate;
pub mod scene_embed;
pub mod splat;
pub mod stream;
pub mod transcode;

pub mod speech;
pub mod thumbnail;
pub mod thumbstrip;

pub use clustering::{
	cosine_dbscan, cosine_distance, cosine_similarity, face_dbscan_params, l2_normalize,
	scene_dbscan_params, DbscanParams, EmbeddingCluster, EmbeddingPoint,
};
pub use derivative_queue::{
	derivative_status_for_content, enqueue_derivatives_for_entry, enqueue_thumbnails_for_entries,
	ensure_pending_sidecar, schedule_derivative_enqueue, ContentDerivativeStatus,
	DerivativeKindStatus, FACE_EMBEDDING_VARIANT, SCENE_EMBEDDING_VARIANT,
};
pub use derivative_status_query::{
	DerivativeStatusInput, DerivativeStatusItem, DerivativeStatusOutput, DerivativeStatusQuery,
	DerivativeStatusTarget,
};
pub use ffmpeg_bin::{
	command as ffmpeg_command, ffmpeg_program, tokio_command as ffmpeg_tokio_command,
};

pub use metadata_extractor::{extract_image_metadata, extract_image_metadata_with_blurhash};

#[cfg(feature = "ffmpeg")]
pub use metadata_extractor::{
	extract_audio_metadata, extract_video_metadata, extract_video_metadata_with_blurhash,
};
pub use ocr::{OcrJob, OcrProcessor};
pub use proxy::{ProxyJob, ProxyProcessor};
pub use rotate::{RotateAction, RotateJob};
pub use scene_embed::{
	detect_execution_device, embed_image_path, evaluate_backends, ExecutionDevice, SceneEmbedError,
	SceneEmbedJob, SceneEmbedJobConfig, SceneEmbedOutput, SceneEmbedResult,
};
pub use splat::{GaussianSplatJob, GaussianSplatProcessor};
pub use stream::{StreamAction, StreamJob};
pub use transcode::{TranscodeAction, TranscodeJob};

#[cfg(feature = "speech-to-text")]
pub use speech::{SpeechToTextJob, SpeechToTextProcessor};
#[cfg(feature = "ffmpeg")]
pub use thumbnail::ThumbnailJob;
#[cfg(feature = "ffmpeg")]
pub use thumbstrip::{ThumbstripJob, ThumbstripProcessor};
