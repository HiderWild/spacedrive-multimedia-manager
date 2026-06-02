//! Transcode error types

use thiserror::Error;

pub type TranscodeResult<T> = Result<T, TranscodeError>;

#[derive(Error, Debug)]
pub enum TranscodeError {
	#[error("File not found: {0}")]
	FileNotFound(String),

	#[error("Unsupported codec/container combination: {0}")]
	UnsupportedCombination(String),

	#[error("Video encoding failed: {0}")]
	EncodingFailed(String),

	#[error("FFmpeg not found in PATH")]
	FFmpegNotFound,

	#[error("FFmpeg process failed with status: {0}")]
	FFmpegProcessFailed(i32),

	#[error("Requested hardware accelerator unavailable: {0}")]
	HardwareAccelUnavailable(String),

	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("Other error: {0}")]
	Other(String),
}

impl TranscodeError {
	pub fn other(msg: impl Into<String>) -> Self {
		Self::Other(msg.into())
	}

	pub fn encoding_failed(msg: impl Into<String>) -> Self {
		Self::EncodingFailed(msg.into())
	}
}
