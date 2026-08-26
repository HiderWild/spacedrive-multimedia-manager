//! Hardware acceleration detection for video encoding

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Supported hardware acceleration platforms
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum HardwareAccel {
	/// Vendor-neutral Vulkan Video encoder
	Vulkan,
	/// Apple VideoToolbox (macOS/iOS)
	VideoToolbox,
	/// NVIDIA NVENC
	NVENC,
	/// Intel QuickSync
	QuickSync,
	/// AMD AMF
	AMF,
	/// VA-API (Linux)
	VAAPI,
}

impl HardwareAccel {
	/// Get the FFmpeg encoder name for this acceleration platform
	pub fn encoder_name(&self) -> &'static str {
		match self {
			Self::Vulkan => "h264_vulkan",
			Self::VideoToolbox => "h264_videotoolbox",
			Self::NVENC => "h264_nvenc",
			Self::QuickSync => "h264_qsv",
			Self::AMF => "h264_amf",
			Self::VAAPI => "h264_vaapi",
		}
	}

	/// Get recommended preset for this encoder
	pub fn preset(&self) -> Option<&'static str> {
		match self {
			Self::Vulkan => None,
			Self::VideoToolbox => None, // VideoToolbox doesn't use presets
			Self::NVENC | Self::QuickSync | Self::AMF => Some("fast"),
			Self::VAAPI => None,
		}
	}

	/// Additional arguments for this encoder
	pub fn extra_args(&self) -> Vec<&'static str> {
		match self {
			Self::Vulkan => vec![],
			Self::VideoToolbox => vec![],
			Self::NVENC => vec!["-rc", "vbr"],
			Self::QuickSync => vec!["-look_ahead", "0"],
			Self::AMF => vec![],
			Self::VAAPI => vec![],
		}
	}
}

/// Detect available hardware acceleration
pub fn detect_hardware_accel() -> Option<HardwareAccel> {
	let encoders_output = match crate::ops::media::ffmpeg_bin::command()
		.args(["-hide_banner", "-encoders"])
		.output()
	{
		Ok(out) => out,
		Err(e) => {
			warn!("Failed to run ffmpeg to detect hardware encoders: {}", e);
			return None;
		}
	};

	if !encoders_output.status.success() {
		warn!("ffmpeg -encoders command failed");
		return None;
	}

	let hwaccels_output = crate::ops::media::ffmpeg_bin::command()
		.args(["-hide_banner", "-hwaccels"])
		.output()
		.ok()
		.filter(|out| out.status.success());

	let encoders = String::from_utf8_lossy(&encoders_output.stdout);
	let hwaccels = hwaccels_output
		.as_ref()
		.map(|out| String::from_utf8_lossy(&out.stdout))
		.unwrap_or_default();

	detect_hardware_accel_from_ffmpeg_output(&encoders, &hwaccels)
}

/// Choose the best H.264 hardware encoder from ffmpeg capability output.
pub fn detect_hardware_accel_from_ffmpeg_output(
	encoders: &str,
	hwaccels: &str,
) -> Option<HardwareAccel> {
	// Vulkan requires both the encoder and a hardware device type; the driver may
	// still reject encode at runtime, in which case callers should fall back to CPU.
	if encoders.contains("h264_vulkan") && hwaccels.contains("vulkan") {
		debug!("Detected Vulkan hardware acceleration");
		return Some(HardwareAccel::Vulkan);
	}

	// Platform-specific detection order (prefer native first)
	#[cfg(target_os = "macos")]
	{
		if encoders.contains("h264_videotoolbox") {
			debug!("Detected VideoToolbox hardware acceleration");
			return Some(HardwareAccel::VideoToolbox);
		}
	}

	// NVENC (NVIDIA)
	if encoders.contains("h264_nvenc") {
		debug!("Detected NVENC hardware acceleration");
		return Some(HardwareAccel::NVENC);
	}

	// QuickSync (Intel)
	if encoders.contains("h264_qsv") {
		debug!("Detected QuickSync hardware acceleration");
		return Some(HardwareAccel::QuickSync);
	}

	// AMD
	if encoders.contains("h264_amf") {
		debug!("Detected AMF hardware acceleration");
		return Some(HardwareAccel::AMF);
	}

	// VA-API (Linux)
	#[cfg(target_os = "linux")]
	{
		if encoders.contains("h264_vaapi") {
			debug!("Detected VA-API hardware acceleration");
			return Some(HardwareAccel::VAAPI);
		}
	}

	debug!("No hardware acceleration detected, will use software encoding");
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_encoder_names() {
		assert_eq!(HardwareAccel::Vulkan.encoder_name(), "h264_vulkan");
		assert_eq!(
			HardwareAccel::VideoToolbox.encoder_name(),
			"h264_videotoolbox"
		);
		assert_eq!(HardwareAccel::NVENC.encoder_name(), "h264_nvenc");
		assert_eq!(HardwareAccel::QuickSync.encoder_name(), "h264_qsv");
	}

	#[test]
	fn detects_vulkan_when_encoder_and_hwaccel_are_present() {
		let hw = detect_hardware_accel_from_ffmpeg_output(
			" V....D h264_vulkan Vulkan H.264",
			"Hardware acceleration methods:\nvulkan\n",
		);

		assert_eq!(hw, Some(HardwareAccel::Vulkan));
	}

	#[test]
	fn skips_vulkan_when_hwaccel_is_missing() {
		let hw = detect_hardware_accel_from_ffmpeg_output(
			" V....D h264_vulkan Vulkan H.264",
			"Hardware acceleration methods:\ncuda\n",
		);

		assert_ne!(hw, Some(HardwareAccel::Vulkan));
	}
}
