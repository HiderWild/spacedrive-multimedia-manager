//! Streaming-package error types.

use thiserror::Error;

pub type StreamResult<T> = Result<T, StreamError>;

#[derive(Error, Debug)]
pub enum StreamError {
	#[error("File not found: {0}")]
	FileNotFound(String),

	#[error("Stream configuration invalid: {0}")]
	InvalidConfig(String),

	#[error("FFmpeg not found in PATH")]
	FFmpegNotFound,

	#[error("FFmpeg process failed with status: {0}")]
	FFmpegProcessFailed(i32),

	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("Other error: {0}")]
	Other(String),
}

impl StreamError {
	pub fn other(msg: impl Into<String>) -> Self {
		Self::Other(msg.into())
	}

	pub fn invalid_config(msg: impl Into<String>) -> Self {
		Self::InvalidConfig(msg.into())
	}
}
