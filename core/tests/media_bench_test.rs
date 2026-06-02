//! Sample media benchmarks. These are `#[ignore]`d so they stay out of the
//! normal `cargo test` run; execute them explicitly with:
//!
//! ```text
//! cargo test -p sd-core --release --test media_bench_test -- --ignored --nocapture
//! ```
//!
//! They benchmark trivial-but-real operations today so the harness is runnable
//! before the media tasks land. Later tasks extend them:
//!   - B-04 swaps the rotation closure for the real rotate job.
//!   - A-05 swaps the tag-resolution closure for cached effective-tag lookup.
//!   - B-01 fills in the transcode benchmark with the real TranscodeJob.

use std::collections::HashSet;

use image::imageops;
use sd_core::testing::media_bench::bench;
use sd_core::testing::media_fixtures::{self, ImageFormatKind};
use tracing::warn;

#[test]
#[ignore = "benchmark; run with --ignored --nocapture in release"]
fn bench_image_rotation() {
	let bytes = media_fixtures::encode_image(256, 192, ImageFormatKind::Png).unwrap();
	let img = image::load_from_memory(&bytes).unwrap();

	let stats = bench("rotate90_256x192", 200, || {
		let rotated = imageops::rotate90(&img);
		std::hint::black_box(rotated);
	});

	println!("{stats}");
	assert!(stats.mean.as_nanos() > 0);
}

#[test]
#[ignore = "benchmark; run with --ignored --nocapture in release"]
fn bench_tag_resolution_placeholder() {
	// Stand-in for A-05 effective-tag resolution: union of ancestor tag sets.
	let direct: HashSet<u32> = (0..64).collect();
	let inherited: HashSet<u32> = (32..160).collect();

	let stats = bench("tag_union_64x128", 5_000, || {
		let effective: HashSet<u32> = direct.union(&inherited).copied().collect();
		std::hint::black_box(effective);
	});

	println!("{stats}");
	assert!(stats.iterations == 5_000);
}

#[test]
#[ignore = "benchmark; requires ffmpeg; run with --ignored --nocapture in release"]
fn bench_transcode_h264_to_vp9() {
	if !media_fixtures::ffmpeg_available() {
		warn!("ffmpeg not found; skipping transcode benchmark");
		return;
	}

	let dir = tempfile::tempdir().unwrap();
	let source = match media_fixtures::synthesize_clip(dir.path(), "h264", 1) {
		Ok(path) => path,
		Err(e) => {
			warn!(error = %e, "could not synthesize source clip; skipping");
			return;
		}
	};

	let stats = bench("transcode_h264_to_vp9", 3, || {
		let out = dir.path().join("out.webm");
		let status = std::process::Command::new("ffmpeg")
			.args(["-y", "-i"])
			.arg(&source)
			.args(["-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p"])
			.arg(&out)
			.stdout(std::process::Stdio::null())
			.stderr(std::process::Stdio::null())
			.status()
			.expect("ffmpeg runs");
		std::hint::black_box(status);
	});

	println!("{stats}");
}
