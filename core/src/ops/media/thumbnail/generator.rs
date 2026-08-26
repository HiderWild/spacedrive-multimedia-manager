//! Thumbnail generation engine using existing Spacedrive crates

use super::error::{ThumbnailError, ThumbnailResult};
use sd_media_metadata::exif::Orientation;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Information about a generated thumbnail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailInfo {
	pub size_bytes: usize,
	pub dimensions: (u32, u32),
	pub format: String,
	pub blurhash: Option<String>,
}

/// Multi-format thumbnail generator
#[derive(Debug)]
pub enum ThumbnailGenerator {
	Image(ImageGenerator),
	Video(VideoGenerator),
	Document(DocumentGenerator),
}

impl ThumbnailGenerator {
	/// Create appropriate generator for a MIME type
	pub fn for_mime_type(mime_type: &str) -> ThumbnailResult<Self> {
		match mime_type {
			mime if mime.starts_with("image/") => Ok(Self::Image(ImageGenerator::new())),
			mime if mime.starts_with("video/") => {
				#[cfg(feature = "ffmpeg")]
				{
					Ok(Self::Video(VideoGenerator::new()))
				}
				#[cfg(not(feature = "ffmpeg"))]
				{
					Err(ThumbnailError::other(
						"Video thumbnail generation requires FFmpeg feature to be enabled",
					))
				}
			}
			"application/pdf" => Ok(Self::Document(DocumentGenerator::new())),
			_ => Err(ThumbnailError::unsupported_format(mime_type)),
		}
	}

	/// Generate thumbnail
	pub async fn generate(
		&self,
		source_path: &Path,
		output_path: &Path,
		size: u32,
		quality: u8,
	) -> ThumbnailResult<ThumbnailInfo> {
		match self {
			Self::Image(gen) => gen.generate(source_path, output_path, size, quality).await,
			Self::Video(gen) => gen.generate(source_path, output_path, size, quality).await,
			Self::Document(gen) => gen.generate(source_path, output_path, size, quality).await,
		}
	}

	/// Decode once and export multiple sizes (images/PDFs). Videos fall back per target.
	pub async fn generate_many(
		&self,
		source_path: &Path,
		targets: &[ThumbnailTarget],
	) -> ThumbnailResult<Vec<ThumbnailInfo>> {
		if targets.is_empty() {
			return Ok(Vec::new());
		}
		match self {
			Self::Image(gen) => gen.generate_many(source_path, targets).await,
			Self::Document(gen) => gen.generate_many(source_path, targets).await,
			Self::Video(gen) => {
				let mut out = Vec::with_capacity(targets.len());
				for t in targets {
					out.push(
						gen.generate(source_path, &t.output_path, t.size, t.quality)
							.await?,
					);
				}
				Ok(out)
			}
		}
	}
}

/// One output requested from a multi-size thumbnail pass.
#[derive(Debug, Clone)]
pub struct ThumbnailTarget {
	pub output_path: std::path::PathBuf,
	pub size: u32,
	pub quality: u8,
}

/// Image thumbnail generator using sd-images crate
#[derive(Debug)]
pub struct ImageGenerator;

impl ImageGenerator {
	pub fn new() -> Self {
		Self
	}

	pub async fn generate(
		&self,
		source_path: &Path,
		output_path: &Path,
		size: u32,
		quality: u8,
	) -> ThumbnailResult<ThumbnailInfo> {
		let targets = [ThumbnailTarget {
			output_path: output_path.to_path_buf(),
			size,
			quality,
		}];
		let mut infos = self.generate_many(source_path, &targets).await?;
		infos
			.pop()
			.ok_or_else(|| ThumbnailError::other("No thumbnail generated"))
	}

	/// Load/orient once, then emit every requested size.
	pub async fn generate_many(
		&self,
		source_path: &Path,
		targets: &[ThumbnailTarget],
	) -> ThumbnailResult<Vec<ThumbnailInfo>> {
		for t in targets {
			if t.quality > 100 {
				return Err(ThumbnailError::InvalidQuality(t.quality));
			}
			if let Some(parent) = t.output_path.parent() {
				tokio::fs::create_dir_all(parent).await?;
			}
		}

		let source_path = source_path.to_path_buf();
		let targets = targets.to_vec();

		tokio::task::spawn_blocking(move || {
			let min_edge = targets.iter().map(|t| t.size).max().unwrap_or(256);
			let mut img = sd_images::format_image_for_thumbnail(&source_path, min_edge)
				.map_err(|e| ThumbnailError::other(format!("Failed to load image: {}", e)))?;

			if let Some(orientation) = Orientation::from_path(&source_path) {
				img = orientation.correct_thumbnail(img);
			}

			// Cap huge bitmaps before multi-size work so decode*N does not thrash RAM.
			img = downscale_for_thumbnail_budget(img);

			let mut results = Vec::with_capacity(targets.len());
			for t in &targets {
				results.push(encode_thumbnail_variant(
					&img,
					&t.output_path,
					t.size,
					t.quality,
				)?);
			}
			Ok::<Vec<ThumbnailInfo>, ThumbnailError>(results)
		})
		.await
		.map_err(|e| ThumbnailError::other(format!("Task join error: {}", e)))?
	}
}

/// Video thumbnail generator using sd-ffmpeg crate
#[derive(Debug)]
pub struct VideoGenerator;

impl VideoGenerator {
	pub fn new() -> Self {
		Self
	}

	pub async fn generate(
		&self,
		source_path: &Path,
		output_path: &Path,
		size: u32,
		quality: u8,
	) -> ThumbnailResult<ThumbnailInfo> {
		#[cfg(feature = "ffmpeg")]
		{
			if quality > 100 {
				return Err(ThumbnailError::InvalidQuality(quality));
			}

			// Blurhash generation disabled for performance
			let blurhash: Option<String> = None;

			// Use sd-ffmpeg helper function to generate thumbnail
			sd_ffmpeg::to_thumbnail(
				source_path,
				output_path,
				sd_ffmpeg::ThumbnailSize::Scale(size),
				quality as f32,
			)
			.await
			.map_err(|e| {
				ThumbnailError::video_processing(format!("FFmpeg processing failed: {}", e))
			})?;

			// Get file size and return info
			let file_size = tokio::fs::metadata(output_path).await?.len() as usize;

			// Calculate approximate dimensions (actual dimensions would require parsing FFmpeg output)
			let dimensions = calculate_video_dimensions(size);

			Ok(ThumbnailInfo {
				size_bytes: file_size,
				dimensions,
				format: "webp".to_string(),
				blurhash,
			})
		}

		#[cfg(not(feature = "ffmpeg"))]
		{
			let _ = (source_path, output_path, size, quality); // Suppress unused variable warnings
			Err(ThumbnailError::other(
				"Video thumbnail generation requires FFmpeg feature to be enabled",
			))
		}
	}
}

/// Document thumbnail generator using sd-images crate (PDF support)
#[derive(Debug)]
pub struct DocumentGenerator;

impl DocumentGenerator {
	pub fn new() -> Self {
		Self
	}

	pub async fn generate(
		&self,
		source_path: &Path,
		output_path: &Path,
		size: u32,
		quality: u8,
	) -> ThumbnailResult<ThumbnailInfo> {
		let targets = [ThumbnailTarget {
			output_path: output_path.to_path_buf(),
			size,
			quality,
		}];
		let mut infos = self.generate_many(source_path, &targets).await?;
		infos
			.pop()
			.ok_or_else(|| ThumbnailError::other("No thumbnail generated"))
	}

	/// Load PDF page once, then emit every requested size.
	pub async fn generate_many(
		&self,
		source_path: &Path,
		targets: &[ThumbnailTarget],
	) -> ThumbnailResult<Vec<ThumbnailInfo>> {
		for t in targets {
			if t.quality > 100 {
				return Err(ThumbnailError::InvalidQuality(t.quality));
			}
			if let Some(parent) = t.output_path.parent() {
				tokio::fs::create_dir_all(parent).await?;
			}
		}

		let source_path = source_path.to_path_buf();
		let targets = targets.to_vec();

		tokio::task::spawn_blocking(move || {
			let mut img = sd_images::format_image(&source_path)
				.map_err(|e| ThumbnailError::other(format!("Failed to load PDF: {}", e)))?;

			if let Some(orientation) = Orientation::from_path(&source_path) {
				img = orientation.correct_thumbnail(img);
			}

			img = downscale_for_thumbnail_budget(img);

			let mut results = Vec::with_capacity(targets.len());
			for t in &targets {
				results.push(encode_thumbnail_variant(
					&img,
					&t.output_path,
					t.size,
					t.quality,
				)?);
			}
			Ok::<Vec<ThumbnailInfo>, ThumbnailError>(results)
		})
		.await
		.map_err(|e| ThumbnailError::other(format!("Task join error: {}", e)))?
	}
}

/// Peak decoded bitmap edge allowed before an intermediate downscale.
const MAX_THUMB_SOURCE_EDGE: u32 = 4096;
/// Above this pixel count, force a pre-downscale even if edge is below the max edge.
const MAX_THUMB_SOURCE_PIXELS: u64 = 16_000_000;

/// Downscale extremely large sources once so subsequent resizes stay cheap and memory-safe.
fn downscale_for_thumbnail_budget(img: image::DynamicImage) -> image::DynamicImage {
	let w = img.width();
	let h = img.height();
	let pixels = (w as u64) * (h as u64);
	let needs =
		w > MAX_THUMB_SOURCE_EDGE || h > MAX_THUMB_SOURCE_EDGE || pixels > MAX_THUMB_SOURCE_PIXELS;
	if !needs {
		return img;
	}
	let (tw, th) = calculate_dimensions(w, h, MAX_THUMB_SOURCE_EDGE);
	img.resize(tw, th, image::imageops::FilterType::Triangle)
}

fn resize_filter(
	original_width: u32,
	original_height: u32,
	target_size: u32,
) -> image::imageops::FilterType {
	let long_edge = original_width.max(original_height);
	// Aggressive downscales: Triangle is cheaper than Lanczos3 with little visual cost at thumb sizes.
	if long_edge as f32 / target_size as f32 > 4.0 {
		image::imageops::FilterType::Triangle
	} else {
		image::imageops::FilterType::Lanczos3
	}
}

fn encode_thumbnail_variant(
	img: &image::DynamicImage,
	output_path: &Path,
	size: u32,
	quality: u8,
) -> ThumbnailResult<ThumbnailInfo> {
	let (original_width, original_height) = (img.width(), img.height());
	let (target_width, target_height) = calculate_dimensions(original_width, original_height, size);
	let filter = resize_filter(original_width, original_height, size);
	let thumbnail = img.resize(target_width, target_height, filter);
	let rgb_thumbnail = thumbnail.to_rgb8();
	let actual_width = rgb_thumbnail.width();
	let actual_height = rgb_thumbnail.height();
	let expected_size = (actual_width * actual_height * 3) as usize;
	let actual_size = rgb_thumbnail.as_raw().len();
	if expected_size != actual_size {
		return Err(ThumbnailError::other(format!(
			"Image buffer size mismatch: expected {} bytes for {}x{}, got {} bytes",
			expected_size, actual_width, actual_height, actual_size
		)));
	}
	let webp_encoder = webp::Encoder::from_rgb(&rgb_thumbnail, actual_width, actual_height);
	let webp_data = webp_encoder.encode(quality as f32).to_vec();
	std::fs::write(output_path, &webp_data)?;
	Ok(ThumbnailInfo {
		size_bytes: webp_data.len(),
		dimensions: (actual_width, actual_height),
		format: "webp".to_string(),
		blurhash: None,
	})
}

/// Calculate target dimensions maintaining aspect ratio
fn calculate_dimensions(width: u32, height: u32, target_size: u32) -> (u32, u32) {
	let aspect_ratio = width as f32 / height as f32;

	if width > height {
		// Landscape
		let target_width = target_size;
		let target_height = (target_size as f32 / aspect_ratio) as u32;
		(target_width, target_height.max(1))
	} else {
		// Portrait or square
		let target_height = target_size;
		let target_width = (target_size as f32 * aspect_ratio) as u32;
		(target_width.max(1), target_height)
	}
}

/// Calculate approximate video thumbnail dimensions
/// In practice, this would need to be extracted from the actual video metadata
fn calculate_video_dimensions(target_size: u32) -> (u32, u32) {
	// Assume 16:9 aspect ratio for now (most common)
	// This is a simplified approach - in practice we'd get actual dimensions from FFmpeg
	let aspect_ratio = 16.0 / 9.0;

	let target_width = target_size;
	let target_height = (target_size as f32 / aspect_ratio) as u32;

	(target_width, target_height.max(1))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_downscale_for_thumbnail_budget_small_unchanged() {
		let img = image::DynamicImage::new_rgb8(800, 600);
		let out = downscale_for_thumbnail_budget(img);
		assert_eq!((out.width(), out.height()), (800, 600));
	}

	#[test]
	fn test_downscale_for_thumbnail_budget_caps_huge() {
		let img = image::DynamicImage::new_rgb8(10000, 8000);
		let out = downscale_for_thumbnail_budget(img);
		assert!(out.width() <= MAX_THUMB_SOURCE_EDGE);
		assert!(out.height() <= MAX_THUMB_SOURCE_EDGE);
	}

	#[test]
	fn test_calculate_dimensions() {
		// Landscape image
		let (w, h) = calculate_dimensions(1920, 1080, 256);
		assert_eq!(w, 256);
		assert_eq!(h, 144);

		// Portrait image
		let (w, h) = calculate_dimensions(1080, 1920, 256);
		assert_eq!(w, 144);
		assert_eq!(h, 256);

		// Square image
		let (w, h) = calculate_dimensions(1000, 1000, 256);
		assert_eq!(w, 256);
		assert_eq!(h, 256);
	}

	#[test]
	fn test_generator_for_mime_type() {
		assert!(matches!(
			ThumbnailGenerator::for_mime_type("image/jpeg"),
			Ok(ThumbnailGenerator::Image(_))
		));

		#[cfg(feature = "ffmpeg")]
		{
			assert!(matches!(
				ThumbnailGenerator::for_mime_type("video/mp4"),
				Ok(ThumbnailGenerator::Video(_))
			));
		}

		#[cfg(not(feature = "ffmpeg"))]
		{
			assert!(ThumbnailGenerator::for_mime_type("video/mp4").is_err());
		}

		assert!(matches!(
			ThumbnailGenerator::for_mime_type("application/pdf"),
			Ok(ThumbnailGenerator::Document(_))
		));

		assert!(ThumbnailGenerator::for_mime_type("text/plain").is_err());
	}
}
