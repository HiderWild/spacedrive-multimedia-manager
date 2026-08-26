//! Optional JPEG decode path using libjpeg-turbo (TurboJPEG).
//!
//! Enabled with the `turbojpeg` feature. When active, large JPEG files can be
//! decompressed with a hardware/SIMD-friendly IDCT and an optional scale factor
//! (1/2, 1/4, 1/8) so thumbnails never need a full-resolution bitmap.

use crate::error::{Error, Result};
use image::{DynamicImage, RgbImage};
use std::path::Path;
use tracing::debug;
use turbojpeg::{Decompressor, Image, PixelFormat, ScalingFactor};

/// Prefer the smallest TurboJPEG scale factor whose long edge is still >= `min_edge`.
pub fn scale_for_min_edge(width: usize, height: usize, min_edge: u32) -> ScalingFactor {
	let long = width.max(height) as u32;
	if long == 0 || min_edge == 0 {
		return ScalingFactor::ONE;
	}
	// Prefer coarser scales first for memory/CPU; keep long edge >= min_edge when possible.
	const CANDIDATES: [ScalingFactor; 4] = [
		ScalingFactor::ONE_EIGHTH,
		ScalingFactor::ONE_QUARTER,
		ScalingFactor::ONE_HALF,
		ScalingFactor::ONE,
	];
	for factor in CANDIDATES {
		let scaled = ((long as u64) * (factor.num() as u64) / (factor.denom() as u64)).max(1) as u32;
		if scaled >= min_edge {
			return factor;
		}
	}
	ScalingFactor::ONE
}

/// Decode a JPEG file into an RGB DynamicImage, optionally pre-scaled via TurboJPEG.
pub fn decode_jpeg(path: &Path, min_edge: Option<u32>) -> Result<DynamicImage> {
	let jpeg_data =
		std::fs::read(path).map_err(|e| Error::Io(e, path.to_path_buf().into_boxed_path()))?;

	let mut decompressor =
		Decompressor::new().map_err(|e| Error::ImageDecode(format!("turbojpeg init: {e}")))?;

	let header = decompressor
		.read_header(&jpeg_data)
		.map_err(|e| Error::ImageDecode(format!("turbojpeg header: {e}")))?;

	let scaling = min_edge
		.map(|edge| scale_for_min_edge(header.width, header.height, edge))
		.unwrap_or(ScalingFactor::ONE);

	if scaling != ScalingFactor::ONE {
		decompressor
			.set_scaling_factor(scaling)
			.map_err(|e| Error::ImageDecode(format!("turbojpeg set scale: {e}")))?;
		debug!(
			path = %path.display(),
			width = header.width,
			height = header.height,
			scale_num = scaling.num(),
			scale_denom = scaling.denom(),
			"JPEG turbo decode with scale"
		);
	}

	let scaled = header.scaled(scaling);
	let width = scaled.width;
	let height = scaled.height;
	let pitch = width * 3;
	let mut pixels = vec![0u8; pitch * height];
	let image = Image {
		pixels: pixels.as_mut_slice(),
		width,
		pitch,
		height,
		format: PixelFormat::RGB,
	};

	decompressor
		.decompress(&jpeg_data, image)
		.map_err(|e| Error::ImageDecode(format!("turbojpeg decompress: {e}")))?;

	let rgb = RgbImage::from_raw(width as u32, height as u32, pixels)
		.ok_or(Error::RgbImageConversion)?;
	Ok(DynamicImage::ImageRgb8(rgb))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn scale_picks_half_when_edge_allows() {
		// 4000px long edge, min 256 → 1/8 of 4000 = 500 >= 256
		let f = scale_for_min_edge(4000, 3000, 256);
		assert_eq!((f.num(), f.denom()), (1, 8));
	}

	#[test]
	fn scale_falls_back_to_one_for_tiny_targets_on_small_images() {
		let f = scale_for_min_edge(200, 100, 256);
		assert_eq!((f.num(), f.denom()), (1, 1));
	}
}
