//! Generalized video transcoding.
//!
//! Where the proxy system produces fixed low-resolution preview variants, the
//! transcode system re-encodes videos to an arbitrary target: codec (H.264,
//! HEVC, VP9, AV1), container (MP4/MKV/WebM), resolution, and quality
//! (CRF or bitrate). It exposes the same two shapes as proxy, a batch
//! [`TranscodeJob`] for whole-library runs and a [`TranscodeAction`] for a
//! single file, both built on the shared [`TranscodeGenerator`].

pub mod action;
pub mod config;
pub mod encoder;
mod error;
pub mod generator;
pub mod hwaccel;
pub mod job;
mod state;

pub use action::{TranscodeAction, TranscodeInput, TranscodeOutput};
pub use config::{
	TranscodeCodec, TranscodeConfig, TranscodeContainer, TranscodeJobConfig, TranscodeQuality,
	TranscodeResolution,
};
pub use encoder::{select_encoder, EncoderSelection};
pub use error::{TranscodeError, TranscodeResult};
pub use generator::{TranscodeGenerator, TranscodeInfo};
pub use hwaccel::{resolve_hw_encoder, AvailableEncoders, HwAccel, HwFamily};
pub use job::{TranscodeJob, TranscodeJobOutput};
pub use state::{TranscodePhase, TranscodeState};
