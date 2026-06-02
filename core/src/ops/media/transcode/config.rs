//! Transcode configuration: codec, container, resolution, and quality.
//!
//! [`TranscodeConfig`] describes a single output: which codec to encode with,
//! the container to wrap it in, how to scale the picture, and the quality
//! target (constant-quality CRF or a bitrate). The job-level
//! [`TranscodeJobConfig`] layers batch concerns on top (which codec/container to
//! produce, whether to overwrite existing outputs, and where to write them).

use super::hwaccel::HwAccel;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Target video codec.
///
/// CPU encoder selection lives in [`crate::ops::media::transcode::encoder`] so a
/// hardware branch (task B-02) can be added without touching call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum TranscodeCodec {
	/// H.264 / AVC (`libx264`). Most compatible.
	H264,
	/// H.265 / HEVC (`libx265`). Better compression than H.264.
	Hevc,
	/// VP9 (`libvpx-vp9`). Royalty-free, pairs with WebM.
	Vp9,
	/// AV1 (`libsvtav1`). Best compression, slowest to encode.
	Av1,
}

impl TranscodeCodec {
	/// `codec_name` ffprobe reports for this codec, useful in assertions.
	pub fn probe_name(&self) -> &'static str {
		match self {
			Self::H264 => "h264",
			Self::Hevc => "hevc",
			Self::Vp9 => "vp9",
			Self::Av1 => "av1",
		}
	}

	/// Container that is always valid for this codec, used when the caller does
	/// not pin one explicitly.
	pub fn default_container(&self) -> TranscodeContainer {
		match self {
			Self::H264 | Self::Hevc => TranscodeContainer::Mp4,
			Self::Vp9 => TranscodeContainer::Webm,
			Self::Av1 => TranscodeContainer::Mp4,
		}
	}
}

/// Output container format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum TranscodeContainer {
	Mp4,
	Mkv,
	Webm,
}

impl TranscodeContainer {
	pub fn extension(&self) -> &'static str {
		match self {
			Self::Mp4 => "mp4",
			Self::Mkv => "mkv",
			Self::Webm => "webm",
		}
	}

	/// Audio encoder to pair with this container. WebM only permits Opus/Vorbis,
	/// so it gets Opus while MP4/MKV use AAC.
	pub fn audio_encoder(&self) -> &'static str {
		match self {
			Self::Mp4 | Self::Mkv => "aac",
			Self::Webm => "libopus",
		}
	}

	/// WebM rejects H.264/HEVC; reject those pairings up front.
	pub fn supports(&self, codec: TranscodeCodec) -> bool {
		match self {
			Self::Mp4 | Self::Mkv => true,
			Self::Webm => matches!(codec, TranscodeCodec::Vp9 | TranscodeCodec::Av1),
		}
	}
}

/// How to scale the output picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscodeResolution {
	/// Keep the source resolution.
	Keep,
	/// Fit within a square of `0` on the longest side, preserving aspect ratio.
	MaxDimension(u32),
	/// Scale to an exact width and height.
	Scale { width: u32, height: u32 },
}

impl Default for TranscodeResolution {
	fn default() -> Self {
		Self::Keep
	}
}

/// Quality target for the video stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscodeQuality {
	/// Constant Rate Factor (lower is higher quality).
	Crf(u32),
	/// Target average bitrate in kilobits per second.
	Bitrate(u32),
}

impl Default for TranscodeQuality {
	fn default() -> Self {
		Self::Crf(23)
	}
}

/// Full description of a single transcode output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TranscodeConfig {
	pub codec: TranscodeCodec,
	pub container: TranscodeContainer,
	pub resolution: TranscodeResolution,
	pub quality: TranscodeQuality,
	/// Encoder speed preset. Interpreted per-encoder (libx264/libx265 use named
	/// presets like `veryfast`; SVT-AV1 maps it onto its numeric preset ladder).
	pub preset: String,
	/// Prefer a hardware encoder when one is available. B-01 uses this for the
	/// Vulkan H.264 path; it gates the B-01 Auto branch in the generator.
	pub use_hardware_accel: bool,
	/// Hardware acceleration backend (B-02). `Auto` picks the best available
	/// accelerator and falls back to CPU; an explicit backend forces it and errors
	/// when missing; `None` forces the CPU encoder.
	#[serde(default)]
	pub hw_accel: HwAccel,
}

impl TranscodeConfig {
	/// Build a config for `codec` using its default container and CRF 23.
	pub fn new(codec: TranscodeCodec) -> Self {
		Self {
			codec,
			container: codec.default_container(),
			resolution: TranscodeResolution::Keep,
			quality: TranscodeQuality::default(),
			preset: "veryfast".to_string(),
			use_hardware_accel: false,
			hw_accel: HwAccel::default(),
		}
	}

	pub fn with_container(mut self, container: TranscodeContainer) -> Self {
		self.container = container;
		self
	}

	pub fn with_resolution(mut self, resolution: TranscodeResolution) -> Self {
		self.resolution = resolution;
		self
	}

	pub fn with_quality(mut self, quality: TranscodeQuality) -> Self {
		self.quality = quality;
		self
	}

	pub fn with_preset(mut self, preset: impl Into<String>) -> Self {
		self.preset = preset.into();
		self
	}

	/// File extension for outputs produced with this config.
	pub fn extension(&self) -> &'static str {
		self.container.extension()
	}

	/// Reject codec/container pairings the muxer cannot handle.
	pub fn validate(&self) -> Result<(), super::error::TranscodeError> {
		if !self.container.supports(self.codec) {
			return Err(super::error::TranscodeError::UnsupportedCombination(
				format!("{:?} cannot be muxed into {:?}", self.codec, self.container),
			));
		}
		Ok(())
	}
}

/// Configuration for a batch transcode job.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscodeJobConfig {
	/// Codec/container/quality/resolution to produce for each discovered video.
	pub output: TranscodeConfig,
	/// Re-encode even when an output already exists on disk.
	pub regenerate: bool,
	/// Directory for outputs. When `None` the job writes under
	/// `<library>/transcodes`.
	pub output_dir: Option<std::path::PathBuf>,
}

impl TranscodeJobConfig {
	pub fn new(output: TranscodeConfig) -> Self {
		Self {
			output,
			regenerate: false,
			output_dir: None,
		}
	}
}

impl Default for TranscodeJobConfig {
	fn default() -> Self {
		Self::new(TranscodeConfig::new(TranscodeCodec::H264))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_containers_match_codecs() {
		assert_eq!(
			TranscodeCodec::H264.default_container(),
			TranscodeContainer::Mp4
		);
		assert_eq!(
			TranscodeCodec::Vp9.default_container(),
			TranscodeContainer::Webm
		);
	}

	#[test]
	fn webm_rejects_h264() {
		let cfg =
			TranscodeConfig::new(TranscodeCodec::H264).with_container(TranscodeContainer::Webm);
		assert!(cfg.validate().is_err());

		let ok = TranscodeConfig::new(TranscodeCodec::Vp9).with_container(TranscodeContainer::Webm);
		assert!(ok.validate().is_ok());
	}

	#[test]
	fn config_round_trips_messagepack() {
		let cfg = TranscodeConfig::new(TranscodeCodec::Av1)
			.with_resolution(TranscodeResolution::MaxDimension(720))
			.with_quality(TranscodeQuality::Bitrate(2000));
		let bytes = rmp_serde::to_vec_named(&cfg).unwrap();
		let back: TranscodeConfig = rmp_serde::from_slice(&bytes).unwrap();
		assert_eq!(cfg, back);
	}
}
