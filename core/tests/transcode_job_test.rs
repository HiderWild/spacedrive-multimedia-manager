//! Exercises the generic transcode generator against synthesized fixtures.
//!
//! Generates a tiny source clip with the Wave-0 ffmpeg fixture helper, transcodes
//! it to H.264 plus other codecs, and verifies each output exists, is non-empty,
//! and probes as the requested codec. The whole suite skips gracefully when
//! ffmpeg is unavailable, and individual codecs are skipped (with a logged note)
//! when this ffmpeg build lacks their encoder.

use std::path::Path;
use std::process::Command;

use sd_core::ops::media::transcode::{
	TranscodeCodec, TranscodeConfig, TranscodeContainer, TranscodeGenerator, TranscodeQuality,
	TranscodeResolution,
};
use sd_core::testing::media_fixtures;
use tracing::warn;

/// Probe the codec_name of the first video stream, e.g. "h264", "vp9", "av1".
fn probe_video_codec(path: &Path) -> Option<String> {
	let output = Command::new("ffprobe")
		.args([
			"-v",
			"error",
			"-select_streams",
			"v:0",
			"-show_entries",
			"stream=codec_name",
			"-of",
			"default=nw=1:nk=1",
		])
		.arg(path)
		.output()
		.ok()?;

	if !output.status.success() {
		return None;
	}

	let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
	if name.is_empty() {
		None
	} else {
		Some(name)
	}
}

/// True when this ffmpeg build can encode `codec` (checked via `-encoders`).
fn encoder_available(encoder: &str) -> bool {
	Command::new("ffmpeg")
		.args(["-hide_banner", "-encoders"])
		.output()
		.map(|o| String::from_utf8_lossy(&o.stdout).contains(encoder))
		.unwrap_or(false)
}

fn cpu_encoder_name(codec: TranscodeCodec) -> &'static str {
	match codec {
		TranscodeCodec::H264 => "libx264",
		TranscodeCodec::Hevc => "libx265",
		TranscodeCodec::Vp9 => "libvpx-vp9",
		TranscodeCodec::Av1 => "libsvtav1",
	}
}

#[tokio::test]
async fn transcodes_to_multiple_codecs() {
	if !media_fixtures::ffmpeg_available() {
		warn!("ffmpeg not found; skipping transcode integration test");
		return;
	}

	let dir = tempfile::tempdir().expect("tempdir");

	// Source clip: an H.264 mp4 from the Wave-0 fixture helper.
	let source = media_fixtures::synthesize_clip(dir.path(), "h264", 1)
		.expect("source clip synthesizes when ffmpeg present");
	assert!(source.exists(), "source fixture should exist");

	// (codec, container) targets. H.264 is mandatory; the rest are attempted and
	// skipped if the local ffmpeg lacks the encoder.
	let targets = [
		(TranscodeCodec::H264, TranscodeContainer::Mp4),
		(TranscodeCodec::Vp9, TranscodeContainer::Webm),
		(TranscodeCodec::Hevc, TranscodeContainer::Mp4),
		(TranscodeCodec::Av1, TranscodeContainer::Mp4),
	];

	let mut succeeded = Vec::new();

	for (codec, container) in targets {
		let encoder = cpu_encoder_name(codec);
		if !encoder_available(encoder) {
			warn!(
				?codec,
				encoder, "encoder absent from this ffmpeg build; skipping codec"
			);
			continue;
		}

		let config = TranscodeConfig::new(codec)
			.with_container(container)
			.with_resolution(TranscodeResolution::Keep)
			// High CRF keeps these tiny fixtures fast to encode.
			.with_quality(TranscodeQuality::Crf(40))
			.with_preset("veryfast");

		let output = dir.path().join(format!(
			"out_{}.{}",
			codec.probe_name(),
			container.extension()
		));

		let generator = TranscodeGenerator::new(config);
		let info = generator
			.generate(&source, &output)
			.await
			.unwrap_or_else(|e| panic!("transcode to {:?} failed: {e}", codec));

		assert!(output.exists(), "{:?} output should exist", codec);
		assert!(
			info.size_bytes > 0,
			"{:?} output should be non-empty",
			codec
		);
		let on_disk = std::fs::metadata(&output).expect("metadata").len();
		assert_eq!(on_disk, info.size_bytes, "reported size matches disk");

		let probed = probe_video_codec(&output)
			.unwrap_or_else(|| panic!("could not probe {:?} output", codec));
		assert_eq!(
			probed,
			codec.probe_name(),
			"{:?} output should probe as {}",
			codec,
			codec.probe_name()
		);

		succeeded.push(codec);
	}

	// H.264 must always succeed; ffmpeg shipping without libx264 is unexpected.
	assert!(
		succeeded.contains(&TranscodeCodec::H264),
		"H.264 transcode must succeed"
	);
	// And at least one additional codec must have run to prove generality.
	assert!(
		succeeded.len() >= 2,
		"expected H.264 plus at least one other codec, got {:?}",
		succeeded
	);
}
