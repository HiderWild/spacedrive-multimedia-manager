//! Encoder-selection tests for GPU-accelerated transcoding (task B-02).
//!
//! These exercise the deterministic, injectable selection path: an
//! [`AvailableEncoders`] set is supplied directly so the outcome never depends on
//! whether the host actually has a GPU. They cover the three required cases:
//! Auto picking a hardware encoder, Auto falling back to CPU, and an explicit
//! backend that is forced but unavailable.

use sd_core::ops::media::transcode::encoder::select_encoder_hw;
use sd_core::ops::media::transcode::{
	resolve_hw_encoder, AvailableEncoders, HwAccel, TranscodeCodec, TranscodeError,
	TranscodeQuality,
};

/// (a) Auto + NVENC available for H.264 -> picks `h264_nvenc`.
#[test]
fn auto_selects_nvenc_when_available() {
	let available = AvailableEncoders::from_names(["libx264", "h264_nvenc", "hevc_nvenc"]);

	let resolved = resolve_hw_encoder(TranscodeCodec::H264, HwAccel::Auto, &available)
		.expect("auto never errors");
	assert_eq!(resolved.map(|r| r.encoder), Some("h264_nvenc"));

	let selection = select_encoder_hw(
		TranscodeCodec::H264,
		TranscodeQuality::Crf(23),
		"veryfast",
		HwAccel::Auto,
		&available,
	)
	.expect("selection succeeds");

	assert!(selection.hardware);
	assert_eq!(selection.encoder, "h264_nvenc");
	let video_args: Vec<String> = selection
		.video_args
		.iter()
		.map(|a| a.to_string_lossy().into_owned())
		.collect();
	assert!(
		video_args.windows(2).any(|w| w[0] == "-cq" && w[1] == "23"),
		"NVENC CRF should map to -cq: {video_args:?}"
	);
}

/// (b) Auto + no hardware encoders -> falls back to `libx264`.
#[test]
fn auto_falls_back_to_libx264_without_hardware() {
	let available = AvailableEncoders::from_names(["libx264", "libx265", "libvpx-vp9"]);

	let resolved = resolve_hw_encoder(TranscodeCodec::H264, HwAccel::Auto, &available)
		.expect("auto never errors");
	assert!(resolved.is_none(), "no hardware encoder should resolve");

	let selection = select_encoder_hw(
		TranscodeCodec::H264,
		TranscodeQuality::Crf(23),
		"veryfast",
		HwAccel::Auto,
		&available,
	)
	.expect("selection succeeds");

	assert!(!selection.hardware);
	assert_eq!(selection.encoder, "libx264");
}

/// (c) Explicit NVENC forced but unavailable -> errors.
///
/// Chosen behavior: forcing a specific backend that the ffmpeg build cannot
/// satisfy is a hard error (`TranscodeError::HardwareAccelUnavailable`) rather
/// than a silent CPU downgrade, so callers who demand hardware are told it is
/// missing. `Auto` remains the way to opt into "hardware if possible, else CPU".
#[test]
fn forced_nvenc_unavailable_errors() {
	let available = AvailableEncoders::from_names(["libx264"]);

	let err = resolve_hw_encoder(TranscodeCodec::H264, HwAccel::Nvenc, &available)
		.expect_err("forced unavailable backend must error");
	assert!(matches!(err, TranscodeError::HardwareAccelUnavailable(_)));

	let err = select_encoder_hw(
		TranscodeCodec::H264,
		TranscodeQuality::Crf(23),
		"veryfast",
		HwAccel::Nvenc,
		&available,
	)
	.expect_err("selection must propagate the error");
	assert!(matches!(err, TranscodeError::HardwareAccelUnavailable(_)));
}

/// Explicit QSV for AV1 maps to `av1_qsv` and uses `-global_quality`.
#[test]
fn explicit_qsv_av1_uses_global_quality() {
	let available = AvailableEncoders::from_names(["libsvtav1", "av1_qsv"]);

	let selection = select_encoder_hw(
		TranscodeCodec::Av1,
		TranscodeQuality::Crf(30),
		"medium",
		HwAccel::Qsv,
		&available,
	)
	.expect("selection succeeds");

	assert_eq!(selection.encoder, "av1_qsv");
	let video_args: Vec<String> = selection
		.video_args
		.iter()
		.map(|a| a.to_string_lossy().into_owned())
		.collect();
	assert!(video_args
		.windows(2)
		.any(|w| w[0] == "-global_quality" && w[1] == "30"));
}
