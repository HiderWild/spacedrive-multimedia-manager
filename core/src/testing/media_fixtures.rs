//! # Media test fixtures
//!
//! `core::testing::media_fixtures` produces tiny media files for tests and
//! benchmarks without committing binary blobs or touching the network. Image
//! fixtures are generated in-memory with the `image` crate so they stay a few
//! hundred bytes each, and EXIF orientation / ICC profile metadata is injected
//! by hand so rotation work (task B-04) and metadata extraction have something
//! deterministic to round-trip against.
//!
//! Video fixtures need real codec data, which only ffmpeg can synthesize. Rather
//! than commit megabytes of `.mp4`, [`synthesize_clip`] shells out to the
//! `ffmpeg` binary at test time to render a one-second `testsrc` clip. Callers
//! must guard those paths with [`ffmpeg_available`] so suites skip gracefully on
//! hosts without ffmpeg.
//!
//! ## Example
//! ```no_run
//! use sd_core::testing::media_fixtures::{self, ImageFormatKind};
//!
//! let dir = tempfile::tempdir().unwrap();
//! let written = media_fixtures::write_image_fixtures(dir.path()).unwrap();
//! assert!(!written.is_empty());
//!
//! if media_fixtures::ffmpeg_available() {
//!     let clip = media_fixtures::synthesize_clip(dir.path(), "h264", 1).unwrap();
//!     assert!(clip.exists());
//! }
//! # let _ = ImageFormatKind::Png;
//! ```

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use image::{ImageFormat, Rgb, RgbImage};
use tracing::{debug, warn};

/// Errors raised while generating media fixtures.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
	#[error("image encoding failed: {0}")]
	Image(#[from] image::ImageError),

	#[error("filesystem error: {0}")]
	Io(#[from] std::io::Error),

	#[error("ffmpeg is not available on this host")]
	FfmpegMissing,

	#[error("ffmpeg failed to synthesize clip: {0}")]
	Ffmpeg(String),
}

/// Result alias for fixture operations.
pub type FixtureResult<T> = Result<T, FixtureError>;

/// On-disk image encoding for a generated fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormatKind {
	Png,
	Jpeg,
}

impl ImageFormatKind {
	fn extension(self) -> &'static str {
		match self {
			ImageFormatKind::Png => "png",
			ImageFormatKind::Jpeg => "jpg",
		}
	}

	fn format(self) -> ImageFormat {
		match self {
			ImageFormatKind::Png => ImageFormat::Png,
			ImageFormatKind::Jpeg => ImageFormat::Jpeg,
		}
	}
}

/// Declarative description of one image fixture.
///
/// The set returned by [`default_image_specs`] intentionally spans landscape,
/// portrait, and square aspect ratios plus the eight EXIF orientation values so
/// rotation and layout code (tasks B-04 and C-02) can exercise edge cases.
#[derive(Debug, Clone)]
pub struct ImageFixtureSpec {
	/// File stem (no extension); the extension comes from `format`.
	pub name: &'static str,
	pub width: u32,
	pub height: u32,
	pub format: ImageFormatKind,
	/// EXIF orientation tag value (1-8), or `None` to omit the EXIF block.
	pub orientation: Option<u16>,
	/// Embed a synthetic ICC profile so preservation can be verified.
	pub with_icc: bool,
}

/// The default curated fixture set.
///
/// Kept tiny (a few hundred bytes each, generated at test time) so nothing is
/// committed and the whole set materializes in milliseconds.
pub fn default_image_specs() -> Vec<ImageFixtureSpec> {
	vec![
		ImageFixtureSpec {
			name: "landscape_16x9",
			width: 64,
			height: 36,
			format: ImageFormatKind::Png,
			orientation: None,
			with_icc: false,
		},
		ImageFixtureSpec {
			name: "portrait_9x16",
			width: 36,
			height: 64,
			format: ImageFormatKind::Png,
			orientation: None,
			with_icc: false,
		},
		ImageFixtureSpec {
			name: "square_1x1",
			width: 48,
			height: 48,
			format: ImageFormatKind::Jpeg,
			orientation: None,
			with_icc: false,
		},
		ImageFixtureSpec {
			name: "exif_orientation_1",
			width: 64,
			height: 48,
			format: ImageFormatKind::Jpeg,
			orientation: Some(1),
			with_icc: false,
		},
		ImageFixtureSpec {
			name: "exif_orientation_6",
			width: 64,
			height: 48,
			format: ImageFormatKind::Jpeg,
			orientation: Some(6),
			with_icc: false,
		},
		ImageFixtureSpec {
			name: "exif_orientation_8",
			width: 64,
			height: 48,
			format: ImageFormatKind::Jpeg,
			orientation: Some(8),
			with_icc: false,
		},
		ImageFixtureSpec {
			name: "icc_tagged",
			width: 48,
			height: 48,
			format: ImageFormatKind::Jpeg,
			orientation: Some(1),
			with_icc: true,
		},
	]
}

/// Render a deterministic RGB gradient so encoders have real, non-degenerate
/// content to compress.
fn gradient_image(width: u32, height: u32) -> RgbImage {
	let mut img = RgbImage::new(width.max(1), height.max(1));
	let (w, h) = (img.width().max(1), img.height().max(1));
	for (x, y, pixel) in img.enumerate_pixels_mut() {
		let r = ((x * 255) / w) as u8;
		let g = ((y * 255) / h) as u8;
		let b = (((x + y) * 255) / (w + h)) as u8;
		*pixel = Rgb([r, g, b]);
	}
	img
}

/// Encode a freshly generated gradient image to the requested format.
pub fn encode_image(width: u32, height: u32, format: ImageFormatKind) -> FixtureResult<Vec<u8>> {
	let img = gradient_image(width, height);
	let mut buf = Cursor::new(Vec::new());
	img.write_to(&mut buf, format.format())?;
	Ok(buf.into_inner())
}

/// Build the bytes for one fixture spec, injecting EXIF/ICC as requested.
///
/// EXIF and ICC injection only applies to JPEG fixtures because the hand-built
/// APP1/APP2 segments target the JFIF container. PNG specs ignore those flags.
pub fn build_fixture_bytes(spec: &ImageFixtureSpec) -> FixtureResult<Vec<u8>> {
	let mut bytes = encode_image(spec.width, spec.height, spec.format)?;

	if spec.format == ImageFormatKind::Jpeg {
		if let Some(orientation) = spec.orientation {
			bytes = inject_exif_orientation(&bytes, orientation);
		}
		if spec.with_icc {
			bytes = inject_icc_profile(&bytes, &synthetic_icc_profile());
		}
	}

	Ok(bytes)
}

/// Write the full default fixture set into `dir`, returning the created paths.
pub fn write_image_fixtures(dir: impl AsRef<Path>) -> FixtureResult<Vec<PathBuf>> {
	let dir = dir.as_ref();
	std::fs::create_dir_all(dir)?;

	let mut written = Vec::new();
	for spec in default_image_specs() {
		let bytes = build_fixture_bytes(&spec)?;
		let path = dir.join(format!("{}.{}", spec.name, spec.format.extension()));
		std::fs::write(&path, &bytes)?;
		debug!(
			fixture = spec.name,
			bytes = bytes.len(),
			"wrote image fixture"
		);
		written.push(path);
	}
	Ok(written)
}

/// Insert a minimal EXIF APP1 segment carrying a single Orientation tag.
///
/// The segment uses big-endian (`MM`) TIFF byte order and is spliced in right
/// after the JPEG SOI marker, where decoders expect application segments. Any
/// existing metadata is left untouched.
pub fn inject_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
	// Build the TIFF block: header + one-entry IFD0.
	let mut tiff = Vec::new();
	tiff.extend_from_slice(b"MM"); // big-endian
	tiff.extend_from_slice(&[0x00, 0x2A]); // TIFF magic
	tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x08]); // offset to IFD0
	tiff.extend_from_slice(&[0x00, 0x01]); // one entry
	tiff.extend_from_slice(&[0x01, 0x12]); // tag: Orientation
	tiff.extend_from_slice(&[0x00, 0x03]); // type: SHORT
	tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]); // count: 1
	tiff.extend_from_slice(&orientation.to_be_bytes()); // value (high 2 bytes)
	tiff.extend_from_slice(&[0x00, 0x00]); // value padding
	tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // next IFD offset

	let mut payload = Vec::new();
	payload.extend_from_slice(b"Exif\0\0");
	payload.extend_from_slice(&tiff);

	splice_app_segment(jpeg, 0xE1, &payload)
}

/// Insert an ICC profile as a single-chunk APP2 `ICC_PROFILE` segment.
pub fn inject_icc_profile(jpeg: &[u8], profile: &[u8]) -> Vec<u8> {
	let mut payload = Vec::new();
	payload.extend_from_slice(b"ICC_PROFILE\0");
	payload.push(1); // chunk sequence number
	payload.push(1); // total chunk count
	payload.extend_from_slice(profile);

	splice_app_segment(jpeg, 0xE2, &payload)
}

/// Splice an APPn segment (`marker` is the low byte, e.g. 0xE1 for APP1) with
/// the given payload immediately after the SOI marker.
fn splice_app_segment(jpeg: &[u8], marker: u8, payload: &[u8]) -> Vec<u8> {
	// Segment length covers the 2 length bytes plus the payload.
	let seg_len = (payload.len() + 2) as u16;

	let mut out = Vec::with_capacity(jpeg.len() + payload.len() + 4);
	if jpeg.len() >= 2 && jpeg[0] == 0xFF && jpeg[1] == 0xD8 {
		out.extend_from_slice(&jpeg[0..2]); // SOI
		out.push(0xFF);
		out.push(marker);
		out.extend_from_slice(&seg_len.to_be_bytes());
		out.extend_from_slice(payload);
		out.extend_from_slice(&jpeg[2..]);
	} else {
		// Not a JPEG we recognize; return input unchanged.
		warn!("splice target is not a JPEG (no SOI); returning input unchanged");
		out.extend_from_slice(jpeg);
	}
	out
}

/// Read the EXIF orientation tag from a JPEG, if present.
///
/// Supports both `II`/`MM` byte orders so it round-trips fixtures written by
/// [`inject_exif_orientation`]. Returns `None` when no EXIF Orientation tag is
/// found. This is a deliberately small parser for fixture verification, not a
/// general EXIF reader.
pub fn read_exif_orientation(jpeg: &[u8]) -> Option<u16> {
	let tiff = find_app1_tiff(jpeg)?;
	parse_orientation(tiff)
}

/// Read the embedded ICC profile bytes from a JPEG APP2 segment, if present.
pub fn read_icc_profile(jpeg: &[u8]) -> Option<Vec<u8>> {
	let mut i = 2; // skip SOI
	while i + 4 <= jpeg.len() {
		if jpeg[i] != 0xFF {
			break;
		}
		let marker = jpeg[i + 1];
		if marker == 0xDA {
			break; // start of scan; no more metadata segments
		}
		let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
		let seg_start = i + 4;
		let seg_end = i + 2 + len;
		if seg_end > jpeg.len() {
			break;
		}
		if marker == 0xE2 && jpeg[seg_start..].starts_with(b"ICC_PROFILE\0") {
			// Skip "ICC_PROFILE\0" (12) + seq (1) + count (1).
			let data_start = seg_start + 14;
			if data_start <= seg_end {
				return Some(jpeg[data_start..seg_end].to_vec());
			}
		}
		i = seg_end;
	}
	None
}

/// Locate the TIFF block inside the first APP1 EXIF segment.
fn find_app1_tiff(jpeg: &[u8]) -> Option<&[u8]> {
	let mut i = 2; // skip SOI
	while i + 4 <= jpeg.len() {
		if jpeg[i] != 0xFF {
			return None;
		}
		let marker = jpeg[i + 1];
		if marker == 0xDA {
			return None;
		}
		let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
		let seg_start = i + 4;
		let seg_end = i + 2 + len;
		if seg_end > jpeg.len() {
			return None;
		}
		if marker == 0xE1 && jpeg[seg_start..].starts_with(b"Exif\0\0") {
			return Some(&jpeg[seg_start + 6..seg_end]);
		}
		i = seg_end;
	}
	None
}

/// Parse the Orientation tag (0x0112) out of a TIFF block.
fn parse_orientation(tiff: &[u8]) -> Option<u16> {
	if tiff.len() < 8 {
		return None;
	}
	let big_endian = match &tiff[0..2] {
		b"MM" => true,
		b"II" => false,
		_ => return None,
	};

	let read_u16 = |b: &[u8]| -> u16 {
		if big_endian {
			u16::from_be_bytes([b[0], b[1]])
		} else {
			u16::from_le_bytes([b[0], b[1]])
		}
	};
	let read_u32 = |b: &[u8]| -> u32 {
		if big_endian {
			u32::from_be_bytes([b[0], b[1], b[2], b[3]])
		} else {
			u32::from_le_bytes([b[0], b[1], b[2], b[3]])
		}
	};

	let ifd_offset = read_u32(&tiff[4..8]) as usize;
	if ifd_offset + 2 > tiff.len() {
		return None;
	}
	let entry_count = read_u16(&tiff[ifd_offset..ifd_offset + 2]) as usize;
	let mut entry = ifd_offset + 2;
	for _ in 0..entry_count {
		if entry + 12 > tiff.len() {
			return None;
		}
		let tag = read_u16(&tiff[entry..entry + 2]);
		if tag == 0x0112 {
			// SHORT value lives in the first two bytes of the value field.
			return Some(read_u16(&tiff[entry + 8..entry + 10]));
		}
		entry += 12;
	}
	None
}

/// Generate a tiny synthetic ICC profile.
///
/// This is a structurally plausible 128-byte ICC header (correct size field and
/// `acsp` signature) with an empty tag table. It is sufficient for verifying
/// that a transform preserves the embedded profile bytes (task B-04); it is not
/// a color-managed profile. B-04 can swap in a real profile when correctness of
/// color rendering matters.
pub fn synthetic_icc_profile() -> Vec<u8> {
	let mut profile = vec![0u8; 132];
	let size = profile.len() as u32;
	profile[0..4].copy_from_slice(&size.to_be_bytes()); // profile size
	profile[36..40].copy_from_slice(b"acsp"); // profile file signature
	profile[16..20].copy_from_slice(b"RGB "); // data color space
	profile[20..24].copy_from_slice(b"XYZ "); // PCS
	profile[12..16].copy_from_slice(b"mntr"); // device class: display
										   // Tag count (4 bytes after the 128-byte header) defaults to zero.
	profile
}

/// Check whether the `ffmpeg` binary is callable on this host.
///
/// Video fixtures depend on ffmpeg, which is not a build dependency, so callers
/// must gate synthesis on this and skip when it returns `false`.
pub fn ffmpeg_available() -> bool {
	Command::new("ffmpeg")
		.arg("-version")
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.status()
		.map(|s| s.success())
		.unwrap_or(false)
}

/// Map a short codec name to an ffmpeg encoder and output container extension.
fn codec_encoder(codec: &str) -> Option<(&'static str, &'static str)> {
	match codec.to_ascii_lowercase().as_str() {
		"h264" | "x264" | "avc" => Some(("libx264", "mp4")),
		"hevc" | "h265" | "x265" => Some(("libx265", "mp4")),
		"vp9" => Some(("libvpx-vp9", "webm")),
		"mpeg4" => Some(("mpeg4", "mp4")),
		_ => None,
	}
}

/// Synthesize a tiny test clip into `dir` using ffmpeg's `testsrc` source.
///
/// Renders a `duration_secs`-long 64x64 clip with the encoder matching `codec`.
/// The output stays a few KB. Returns [`FixtureError::FfmpegMissing`] when
/// ffmpeg is unavailable so callers can skip instead of failing.
pub fn synthesize_clip(
	dir: impl AsRef<Path>,
	codec: &str,
	duration_secs: u32,
) -> FixtureResult<PathBuf> {
	if !ffmpeg_available() {
		return Err(FixtureError::FfmpegMissing);
	}

	let (encoder, ext) = codec_encoder(codec)
		.ok_or_else(|| FixtureError::Ffmpeg(format!("unknown codec: {codec}")))?;

	let dir = dir.as_ref();
	std::fs::create_dir_all(dir)?;
	let output = dir.join(format!("testsrc_{codec}.{ext}"));

	let duration = duration_secs.max(1);
	let lavfi = format!("testsrc=duration={duration}:size=64x64:rate=10");

	let status = Command::new("ffmpeg")
		.args([
			"-y", "-f", "lavfi", "-i", &lavfi, "-c:v", encoder, "-pix_fmt", "yuv420p",
		])
		.arg(&output)
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::piped())
		.output()?;

	if !status.status.success() {
		let stderr = String::from_utf8_lossy(&status.stderr);
		// Encoder may be absent from this ffmpeg build; surface a clear error.
		return Err(FixtureError::Ffmpeg(format!(
			"ffmpeg exited with {}: {}",
			status.status,
			stderr.lines().last().unwrap_or_default()
		)));
	}

	debug!(codec, path = %output.display(), "synthesized video fixture");
	Ok(output)
}

/// Codecs attempted by [`synthesize_all_clips`], in preference order.
pub const DEFAULT_VIDEO_CODECS: &[&str] = &["h264", "hevc", "vp9", "mpeg4"];

/// Synthesize one clip per codec in [`DEFAULT_VIDEO_CODECS`], skipping codecs
/// the local ffmpeg build cannot encode. Returns the clips that succeeded.
///
/// Returns [`FixtureError::FfmpegMissing`] if ffmpeg itself is absent.
pub fn synthesize_all_clips(dir: impl AsRef<Path>) -> FixtureResult<Vec<PathBuf>> {
	if !ffmpeg_available() {
		return Err(FixtureError::FfmpegMissing);
	}

	let dir = dir.as_ref();
	let mut clips = Vec::new();
	for codec in DEFAULT_VIDEO_CODECS {
		match synthesize_clip(dir, codec, 1) {
			Ok(path) => clips.push(path),
			Err(e) => warn!(codec, error = %e, "skipping codec unsupported by local ffmpeg"),
		}
	}
	Ok(clips)
}
