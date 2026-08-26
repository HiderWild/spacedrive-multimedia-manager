//! Image → model tensor helpers shared by all scene-embedding backends.

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};
use std::path::Path;

/// Load any supported image and resize to square RGB.
pub fn load_and_resize_rgb(path: &Path, size: u32) -> Result<DynamicImage, String> {
	let img = image::open(path).map_err(|e| format!("open image: {e}"))?;
	Ok(img.resize_exact(size, size, FilterType::Triangle))
}

/// Convert to NCHW float32 with ImageNet mean/std (CLIP / common vision stacks).
///
/// Output layout: `[1, 3, H, W]` flattened row-major channels-first.
pub fn image_to_nchw_f32(img: &DynamicImage, size: u32) -> Vec<f32> {
	const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
	const STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];

	let rgb = img.to_rgb8();
	let (w, h) = rgb.dimensions();
	// Expect already resized; if not, caller should resize first.
	let _ = size;
	let mut out = vec![0.0f32; (3 * w * h) as usize];
	let plane = (w * h) as usize;
	for y in 0..h {
		for x in 0..w {
			let p = rgb.get_pixel(x, y);
			let idx = (y * w + x) as usize;
			for c in 0..3 {
				let v = p[c] as f32 / 255.0;
				out[c * plane + idx] = (v - MEAN[c]) / STD[c];
			}
		}
	}
	out
}

/// Simple spatial RGB histogram fingerprint (no ML weights).
///
/// Produces a fixed 192-d vector: 64 bins per channel, L1 then L2 normalized.
pub fn histogram_embedding(img: &DynamicImage) -> Vec<f32> {
	const BINS: usize = 64;
	let rgb = img.to_rgb8();
	let mut hist = vec![0.0f32; BINS * 3];
	for (_, _, p) in rgb.enumerate_pixels() {
		for c in 0..3 {
			let bin = ((p[c] as usize) * BINS / 256).min(BINS - 1);
			hist[c * BINS + bin] += 1.0;
		}
	}
	let sum: f32 = hist.iter().sum::<f32>().max(1.0);
	for x in &mut hist {
		*x /= sum;
	}
	// L2 normalize for cosine clustering
	let mut n2 = 0.0f32;
	for x in &hist {
		n2 += *x * *x;
	}
	let n = n2.sqrt().max(1e-12);
	for x in &mut hist {
		*x /= n;
	}
	hist
}
