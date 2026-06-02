//! Pixel-level image rotation with metadata preservation.
//!
//! [`rotate_file`] decodes an image, applies a [`RotateOp`] to its pixels, and
//! writes the transformed image back to the same path in its original format.
//! For JPEG it additionally normalizes the EXIF orientation tag to Top-Left and
//! re-attaches the source ICC profile, both of which the `image` crate drops on
//! re-encode (see [`super::jpeg_meta`]).

use super::{
	config::RotateOp,
	error::{RotateError, RotateResult},
	jpeg_meta,
};
use image::{DynamicImage, ImageFormat};
use std::{io::Cursor, path::Path};

/// Outcome of rotating a single file.
#[derive(Debug, Clone)]
pub struct RotateInfo {
	/// Width of the written image, after any dimension swap.
	pub width: u32,
	/// Height of the written image, after any dimension swap.
	pub height: u32,
	/// Output byte size on disk.
	pub size_bytes: u64,
	/// Whether an ICC profile was carried through to the output.
	pub icc_preserved: bool,
	/// Whether the output's EXIF orientation was normalized to Top-Left.
	pub orientation_normalized: bool,
}

/// Apply `op` to the image at `path`, writing the result back in place.
///
/// This is synchronous and CPU-bound; callers on an async runtime should wrap it
/// in `spawn_blocking`. Per-file errors are returned so a batch can log and skip
/// without aborting.
pub fn rotate_file(path: &Path, op: RotateOp) -> RotateResult<RotateInfo> {
	if !path.exists() {
		return Err(RotateError::FileNotFound(path.display().to_string()));
	}

	let format = ImageFormat::from_path(path)
		.map_err(|_| RotateError::Unsupported(path.display().to_string()))?;

	let source_bytes = std::fs::read(path)?;
	let image = image::load_from_memory(&source_bytes)?;
	let rotated = apply(&image, op);

	let mut encoded = Vec::new();
	rotated.write_to(&mut Cursor::new(&mut encoded), format)?;

	// JPEG loses APP segments on re-encode, so re-attach ICC and force the EXIF
	// orientation to Top-Left now that the pixels are physically rotated.
	let (final_bytes, icc_preserved, orientation_normalized) = if format == ImageFormat::Jpeg {
		let icc = jpeg_meta::read_icc_profile(&source_bytes);
		let had_icc = icc.is_some();
		let bytes = jpeg_meta::attach_metadata(encoded, icc.as_deref());
		(bytes, had_icc, true)
	} else {
		(encoded, false, false)
	};

	std::fs::write(path, &final_bytes)?;

	Ok(RotateInfo {
		width: rotated.width(),
		height: rotated.height(),
		size_bytes: final_bytes.len() as u64,
		icc_preserved,
		orientation_normalized,
	})
}

/// Map a [`RotateOp`] onto the matching `image` crate transform.
///
/// `rotate90` is clockwise and `rotate270` is counter-clockwise in the `image`
/// crate, matching the [`RotateOp`] semantics directly.
fn apply(image: &DynamicImage, op: RotateOp) -> DynamicImage {
	match op {
		RotateOp::Cw90 => image.rotate90(),
		RotateOp::Ccw90 => image.rotate270(),
		RotateOp::Rotate180 => image.rotate180(),
		RotateOp::FlipH => image.fliph(),
		RotateOp::FlipV => image.flipv(),
	}
}
