//! Exercises the streaming-package generator against synthesized fixtures.
//!
//! Generates a tiny source clip with the Wave-0 ffmpeg fixture helper, packages
//! it into HLS and DASH bundles, and verifies the on-disk structure: an HLS
//! master playlist plus at least one variant playlist and segment that the
//! manifests reference, and a DASH `.mpd` with at least one segment. The suite
//! skips gracefully when ffmpeg is unavailable.

use std::path::Path;

use sd_core::ops::media::stream::{Rendition, StreamConfig, StreamGenerator, StreamProtocol};
use sd_core::testing::media_fixtures;
use tracing::warn;

/// Small ladder (heights below the 64x64 fixture) so renditions never upscale
/// and the encodes stay fast.
fn small_ladder() -> Vec<Rendition> {
	vec![
		Rendition::new("48p", 48, 200),
		Rendition::new("32p", 32, 100),
	]
}

/// Read a text manifest, returning empty string on failure.
fn read_text(path: &Path) -> String {
	std::fs::read_to_string(path).unwrap_or_default()
}

/// Count files in `dir` whose extension matches `ext`.
fn count_with_ext(dir: &Path, ext: &str) -> usize {
	std::fs::read_dir(dir)
		.map(|rd| {
			rd.filter_map(|e| e.ok())
				.filter(|e| {
					e.path()
						.extension()
						.and_then(|x| x.to_str())
						.map(|x| x.eq_ignore_ascii_case(ext))
						.unwrap_or(false)
				})
				.count()
		})
		.unwrap_or(0)
}

#[tokio::test]
async fn packages_hls_and_dash() {
	if !media_fixtures::ffmpeg_available() {
		warn!("ffmpeg not found; skipping stream integration test");
		return;
	}

	let dir = tempfile::tempdir().expect("tempdir");

	// Source clip: a 1-second H.264 mp4 from the Wave-0 fixture helper.
	let source = media_fixtures::synthesize_clip(dir.path(), "h264", 1)
		.expect("source clip synthesizes when ffmpeg present");
	assert!(source.exists(), "source fixture should exist");

	// --- HLS ---------------------------------------------------------------
	let hls_dir = dir.path().join("hls");
	let hls_config = StreamConfig::new(StreamProtocol::Hls)
		.with_ladder(small_ladder())
		.with_segment_duration(6)
		.with_preset("veryfast");

	let hls_info = StreamGenerator::new(hls_config)
		.generate(&source, &hls_dir)
		.await
		.expect("HLS packaging succeeds with ffmpeg present");

	// (a) master playlist exists.
	let master_path = hls_dir.join("master.m3u8");
	assert!(
		master_path.exists(),
		"HLS master playlist should exist at {}",
		master_path.display()
	);
	assert_eq!(hls_info.manifest_path, master_path);

	// (b) at least one variant playlist and at least one segment file exist.
	let variant_path = hls_dir.join("stream_0.m3u8");
	assert!(
		variant_path.exists(),
		"HLS variant playlist stream_0.m3u8 should exist"
	);
	let ts_segments = count_with_ext(&hls_dir, "ts");
	assert!(
		ts_segments >= 1,
		"expected at least one .ts segment, found {ts_segments}"
	);
	assert!(
		hls_info.segment_count >= 1,
		"reported segment_count should be >= 1"
	);

	// (c) the manifests reference the renditions and segments.
	let master = read_text(&master_path);
	assert!(
		master.contains("stream_0.m3u8"),
		"master must reference the variant playlist, got:\n{master}"
	);
	assert!(
		master.contains("#EXT-X-STREAM-INF:"),
		"master must contain stream-inf tags"
	);
	let variant = read_text(&variant_path);
	assert!(
		variant.contains("stream_0_seg_") && variant.contains(".ts"),
		"variant playlist must reference its segments, got:\n{variant}"
	);

	// --- DASH --------------------------------------------------------------
	let dash_dir = dir.path().join("dash");
	let dash_config = StreamConfig::new(StreamProtocol::Dash)
		.with_ladder(small_ladder())
		.with_segment_duration(6)
		.with_preset("veryfast");

	let dash_info = StreamGenerator::new(dash_config)
		.generate(&source, &dash_dir)
		.await
		.expect("DASH packaging succeeds with ffmpeg present");

	// DASH manifest exists and references segments.
	let mpd_path = dash_dir.join("manifest.mpd");
	assert!(
		mpd_path.exists(),
		"DASH manifest should exist at {}",
		mpd_path.display()
	);
	assert_eq!(dash_info.manifest_path, mpd_path);

	let m4s_segments = count_with_ext(&dash_dir, "m4s");
	assert!(
		m4s_segments >= 1,
		"expected at least one DASH .m4s segment, found {m4s_segments}"
	);
	let mpd = read_text(&mpd_path);
	assert!(
		mpd.contains("<MPD") && (mpd.contains("media=") || mpd.contains("chunk-stream")),
		"mpd must reference its segment template, got:\n{mpd}"
	);
}

#[tokio::test]
async fn hls_rerun_skips_completed_renditions() {
	if !media_fixtures::ffmpeg_available() {
		warn!("ffmpeg not found; skipping stream resume test");
		return;
	}

	let dir = tempfile::tempdir().expect("tempdir");
	let source = media_fixtures::synthesize_clip(dir.path(), "h264", 1)
		.expect("source clip synthesizes when ffmpeg present");

	let pkg = dir.path().join("pkg");
	let config = StreamConfig::new(StreamProtocol::Hls)
		.with_ladder(small_ladder())
		.with_segment_duration(6)
		.with_preset("veryfast");

	// First run encodes both renditions.
	let first = StreamGenerator::new(config.clone())
		.generate(&source, &pkg)
		.await
		.expect("first HLS run succeeds");
	assert_eq!(first.renditions_encoded, 2, "first run encodes both");
	assert_eq!(first.renditions_skipped, 0);

	// Second run (non-regenerating) finds both variant playlists and skips them.
	let second = StreamGenerator::new(config)
		.generate(&source, &pkg)
		.await
		.expect("second HLS run succeeds");
	assert_eq!(
		second.renditions_skipped, 2,
		"re-run should skip both completed renditions"
	);
	assert_eq!(second.renditions_encoded, 0);
}
