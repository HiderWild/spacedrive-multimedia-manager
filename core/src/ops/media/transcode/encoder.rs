//! Video encoder selection for transcoding.
//!
//! Maps a [`TranscodeCodec`] to the ffmpeg encoder and the codec-specific quality
//! flags. CPU encoders cover every generic codec, while the hardware branch adds
//! the B-02 Vulkan H.264 path and falls back to CPU when no matching accelerator
//! is available.

use super::config::{TranscodeCodec, TranscodeQuality};
use super::error::TranscodeResult;
use super::hwaccel::{hw_rate_control_args, resolve_hw_encoder, AvailableEncoders, HwAccel};
use crate::ops::media::proxy::{detect_hardware_accel, HardwareAccel};
use std::ffi::OsString;

/// A resolved encoder plus the arguments that express the requested quality.
#[derive(Debug, Clone)]
pub struct EncoderSelection {
	/// ffmpeg arguments that must appear before `-i`.
	pub input_args: Vec<OsString>,
	/// ffmpeg encoder name passed to `-c:v` (e.g. `libx264`).
	pub encoder: &'static str,
	/// Whether the encoder runs on a hardware path.
	pub hardware: bool,
	/// Filter suffix needed by the encoder after scaling.
	pub video_filter_suffix: Option<&'static str>,
	/// Codec-specific video arguments (preset, CRF/bitrate, pixel format).
	pub video_args: Vec<OsString>,
}

/// CPU encoder name for a codec. Verified against this build of ffmpeg with
/// `ffmpeg -hide_banner -encoders`.
fn cpu_encoder(codec: TranscodeCodec) -> &'static str {
	match codec {
		TranscodeCodec::H264 => "libx264",
		TranscodeCodec::Hevc => "libx265",
		TranscodeCodec::Vp9 => "libvpx-vp9",
		TranscodeCodec::Av1 => "libsvtav1",
	}
}

/// Translate a generic speed preset into SVT-AV1's numeric preset ladder
/// (0 = slowest/best, 13 = fastest). Named x264/x265 presets are mapped onto
/// roughly equivalent rungs.
fn svtav1_preset(preset: &str) -> &'static str {
	match preset {
		"ultrafast" | "superfast" => "12",
		"veryfast" | "faster" => "10",
		"fast" => "8",
		"medium" => "6",
		"slow" | "slower" | "veryslow" => "4",
		_ => "8",
	}
}

fn push(args: &mut Vec<OsString>, value: impl Into<OsString>) {
	args.push(value.into());
}

/// Build the quality arguments for an x264/x265-style encoder that accepts named
/// `-preset` plus `-crf` or `-b:v`.
fn x26x_quality_args(preset: &str, quality: TranscodeQuality) -> Vec<OsString> {
	let mut args = Vec::new();
	push(&mut args, "-preset");
	push(&mut args, preset);
	match quality {
		TranscodeQuality::Crf(crf) => {
			push(&mut args, "-crf");
			push(&mut args, crf.to_string());
		}
		TranscodeQuality::Bitrate(kbps) => {
			push(&mut args, "-b:v");
			push(&mut args, format!("{}k", kbps));
		}
	}
	push(&mut args, "-pix_fmt");
	push(&mut args, "yuv420p");
	args
}

/// Select the encoder and quality arguments for a codec.
///
/// When `use_hardware_accel` is true, detection may select a hardware H.264
/// encoder. Unsupported codec/hardware combinations return the CPU encoder.
pub fn select_encoder(
	codec: TranscodeCodec,
	quality: TranscodeQuality,
	preset: &str,
	use_hardware_accel: bool,
) -> EncoderSelection {
	let hardware_accel = if use_hardware_accel {
		detect_hardware_accel()
	} else {
		None
	};

	select_encoder_with_hardware(codec, quality, preset, hardware_accel)
}

/// Select an encoder using a pre-detected hardware accelerator.
pub fn select_encoder_with_hardware(
	codec: TranscodeCodec,
	quality: TranscodeQuality,
	preset: &str,
	hardware_accel: Option<HardwareAccel>,
) -> EncoderSelection {
	if let Some(hw) = hardware_accel {
		if let Some(selection) = hardware_encoder(codec, quality, hw) {
			return selection;
		}
	}

	let encoder = cpu_encoder(codec);
	let video_args = match codec {
		TranscodeCodec::H264 | TranscodeCodec::Hevc => x26x_quality_args(preset, quality),
		TranscodeCodec::Vp9 => {
			// VP9 has no -preset; -deadline/-cpu-used drive speed. CRF mode needs
			// `-b:v 0` so libvpx targets quality rather than a bitrate cap.
			let mut args = Vec::new();
			match quality {
				TranscodeQuality::Crf(crf) => {
					push(&mut args, "-crf");
					push(&mut args, crf.to_string());
					push(&mut args, "-b:v");
					push(&mut args, "0");
				}
				TranscodeQuality::Bitrate(kbps) => {
					push(&mut args, "-b:v");
					push(&mut args, format!("{}k", kbps));
				}
			}
			push(&mut args, "-deadline");
			push(&mut args, "good");
			push(&mut args, "-cpu-used");
			push(&mut args, "4");
			push(&mut args, "-pix_fmt");
			push(&mut args, "yuv420p");
			args
		}
		TranscodeCodec::Av1 => {
			// SVT-AV1 uses a numeric -preset and CRF; bitrate mode uses -b:v.
			let mut args = Vec::new();
			push(&mut args, "-preset");
			push(&mut args, svtav1_preset(preset));
			match quality {
				TranscodeQuality::Crf(crf) => {
					push(&mut args, "-crf");
					push(&mut args, crf.to_string());
				}
				TranscodeQuality::Bitrate(kbps) => {
					push(&mut args, "-b:v");
					push(&mut args, format!("{}k", kbps));
				}
			}
			push(&mut args, "-pix_fmt");
			push(&mut args, "yuv420p");
			args
		}
	};

	EncoderSelection {
		input_args: Vec::new(),
		encoder,
		hardware: false,
		video_filter_suffix: None,
		video_args,
	}
}

/// Select an encoder honouring a B-02 [`HwAccel`] preference against a known set
/// of available ffmpeg encoders.
///
/// `Auto` picks the best available hardware encoder for the codec (else CPU),
/// `None` forces CPU, and an explicit backend forces that hardware encoder or
/// errors via [`super::error::TranscodeError::HardwareAccelUnavailable`] when it
/// is not present. The available-encoder set is injected so selection is
/// deterministic and testable without a GPU.
pub fn select_encoder_hw(
	codec: TranscodeCodec,
	quality: TranscodeQuality,
	preset: &str,
	hw_accel: HwAccel,
	available: &AvailableEncoders,
) -> TranscodeResult<EncoderSelection> {
	match resolve_hw_encoder(codec, hw_accel, available)? {
		Some(resolved) => Ok(EncoderSelection {
			input_args: Vec::new(),
			encoder: resolved.encoder,
			hardware: true,
			// NVENC/QSV/AMF/VideoToolbox accept system-memory frames, so no
			// hwupload filter is required (unlike the Vulkan path).
			video_filter_suffix: None,
			video_args: hw_rate_control_args(resolved.family, quality),
		}),
		None => Ok(select_encoder(codec, quality, preset, false)),
	}
}

fn hardware_encoder(
	codec: TranscodeCodec,
	quality: TranscodeQuality,
	hardware_accel: HardwareAccel,
) -> Option<EncoderSelection> {
	if codec != TranscodeCodec::H264 {
		return None;
	}

	match hardware_accel {
		HardwareAccel::Vulkan => Some(vulkan_h264_encoder(quality)),
		_ => None,
	}
}

fn vulkan_h264_encoder(quality: TranscodeQuality) -> EncoderSelection {
	let qp = match quality {
		TranscodeQuality::Crf(crf) => crf.min(51),
		TranscodeQuality::Bitrate(_) => 25,
	};

	EncoderSelection {
		input_args: vec![
			"-strict".into(),
			"-2".into(),
			"-init_hw_device".into(),
			"vulkan=vk".into(),
			"-filter_hw_device".into(),
			"vk".into(),
		],
		encoder: HardwareAccel::Vulkan.encoder_name(),
		hardware: true,
		video_filter_suffix: Some("format=nv12,hwupload"),
		video_args: vec!["-qp".into(), qp.to_string().into()],
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cpu_encoders_match_codecs() {
		assert_eq!(cpu_encoder(TranscodeCodec::H264), "libx264");
		assert_eq!(cpu_encoder(TranscodeCodec::Hevc), "libx265");
		assert_eq!(cpu_encoder(TranscodeCodec::Vp9), "libvpx-vp9");
		assert_eq!(cpu_encoder(TranscodeCodec::Av1), "libsvtav1");
	}

	#[test]
	fn vp9_crf_sets_zero_bitrate() {
		let sel = select_encoder(
			TranscodeCodec::Vp9,
			TranscodeQuality::Crf(30),
			"veryfast",
			false,
		);
		let joined: Vec<String> = sel
			.video_args
			.iter()
			.map(|a| a.to_string_lossy().into_owned())
			.collect();
		assert!(joined.windows(2).any(|w| w[0] == "-b:v" && w[1] == "0"));
	}

	#[test]
	fn av1_maps_named_preset_to_number() {
		let sel = select_encoder(
			TranscodeCodec::Av1,
			TranscodeQuality::Crf(30),
			"fast",
			false,
		);
		let joined: Vec<String> = sel
			.video_args
			.iter()
			.map(|a| a.to_string_lossy().into_owned())
			.collect();
		assert!(joined.windows(2).any(|w| w[0] == "-preset" && w[1] == "8"));
	}

	#[test]
	fn vulkan_h264_uses_photoprism_device_and_upload_args() {
		let sel = select_encoder_with_hardware(
			TranscodeCodec::H264,
			TranscodeQuality::Crf(25),
			"veryfast",
			Some(HardwareAccel::Vulkan),
		);

		let input_args: Vec<String> = sel
			.input_args
			.iter()
			.map(|a| a.to_string_lossy().into_owned())
			.collect();
		let video_args: Vec<String> = sel
			.video_args
			.iter()
			.map(|a| a.to_string_lossy().into_owned())
			.collect();

		assert!(sel.hardware);
		assert_eq!(sel.encoder, "h264_vulkan");
		assert!(input_args
			.windows(2)
			.any(|w| w[0] == "-init_hw_device" && w[1] == "vulkan=vk"));
		assert!(input_args
			.windows(2)
			.any(|w| w[0] == "-filter_hw_device" && w[1] == "vk"));
		assert_eq!(
			sel.video_filter_suffix.as_deref(),
			Some("format=nv12,hwupload")
		);
		assert_eq!(
			sel.video_filter_suffix
				.as_ref()
				.unwrap()
				.matches("hwupload")
				.count(),
			1
		);
		assert!(video_args.windows(2).any(|w| w[0] == "-qp" && w[1] == "25"));
	}

	#[test]
	fn hardware_request_falls_back_to_cpu_without_h264_hardware() {
		let sel = select_encoder_with_hardware(
			TranscodeCodec::Vp9,
			TranscodeQuality::Crf(30),
			"veryfast",
			Some(HardwareAccel::Vulkan),
		);

		assert!(!sel.hardware);
		assert_eq!(sel.encoder, "libvpx-vp9");
		assert!(sel.input_args.is_empty());
		assert!(sel.video_filter_suffix.is_none());
	}
}
