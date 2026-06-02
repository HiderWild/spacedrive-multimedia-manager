//! Rotation error types.

use thiserror::Error;

pub type RotateResult<T> = Result<T, RotateError>;

#[derive(Error, Debug)]
pub enum RotateError {
	#[error("File not found: {0}")]
	FileNotFound(String),

	#[error("Unsupported image format: {0}")]
	Unsupported(String),

	#[error("Image decode/encode failed: {0}")]
	Image(#[from] image::ImageError),

	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("Other error: {0}")]
	Other(String),
}

impl RotateError {
	pub fn other(msg: impl Into<String>) -> Self {
		Self::Other(msg.into())
	}
}
