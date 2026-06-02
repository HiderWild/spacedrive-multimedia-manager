//! Verifies the media fixture generators produce valid, decodable files and
//! that EXIF/ICC metadata round-trips. Video synthesis is exercised only when
//! ffmpeg is available; otherwise it is skipped with a warning.

use sd_core::testing::media_fixtures::{
	self, build_fixture_bytes, default_image_specs, read_exif_orientation, read_icc_profile,
	synthetic_icc_profile, ImageFormatKind,
};
use tracing::warn;

#[test]
fn image_fixtures_decode_with_expected_dimensions() {
	for spec in default_image_specs() {
		let bytes = build_fixture_bytes(&spec).expect("fixture builds");
		assert!(!bytes.is_empty(), "{} produced no bytes", spec.name);
		assert!(
			bytes.len() < 8 * 1024,
			"{} fixture should stay tiny, got {} bytes",
			spec.name,
			bytes.len()
		);

		let decoded = image::load_from_memory(&bytes)
			.unwrap_or_else(|e| panic!("{} should decode: {e}", spec.name));
		assert_eq!(decoded.width(), spec.width, "{} width", spec.name);
		assert_eq!(decoded.height(), spec.height, "{} height", spec.name);
	}
}

#[test]
fn exif_orientation_round_trips() {
	for orientation in [1u16, 3, 6, 8] {
		let jpeg = media_fixtures::encode_image(48, 32, ImageFormatKind::Jpeg).unwrap();
		let tagged = media_fixtures::inject_exif_orientation(&jpeg, orientation);

		// Still a valid JPEG after injection.
		let decoded = image::load_from_memory(&tagged).expect("tagged jpeg decodes");
		assert_eq!(decoded.width(), 48);
		assert_eq!(decoded.height(), 32);

		assert_eq!(
			read_exif_orientation(&tagged),
			Some(orientation),
			"orientation {orientation} should round-trip"
		);
	}

	// A plain JPEG has no orientation tag.
	let plain = media_fixtures::encode_image(16, 16, ImageFormatKind::Jpeg).unwrap();
	assert_eq!(read_exif_orientation(&plain), None);
}

#[test]
fn icc_profile_round_trips() {
	let jpeg = media_fixtures::encode_image(32, 32, ImageFormatKind::Jpeg).unwrap();
	let profile = synthetic_icc_profile();
	let tagged = media_fixtures::inject_icc_profile(&jpeg, &profile);

	let decoded = image::load_from_memory(&tagged).expect("icc-tagged jpeg decodes");
	assert_eq!(decoded.width(), 32);

	assert_eq!(
		read_icc_profile(&tagged).as_deref(),
		Some(profile.as_slice()),
		"embedded ICC profile should round-trip"
	);
	assert_eq!(read_icc_profile(&jpeg), None);
}

#[test]
fn write_image_fixtures_creates_files() {
	let dir = tempfile::tempdir().unwrap();
	let paths = media_fixtures::write_image_fixtures(dir.path()).unwrap();

	assert_eq!(paths.len(), default_image_specs().len());
	for path in &paths {
		let meta = std::fs::metadata(path).unwrap();
		assert!(meta.len() > 0, "{} should be non-empty", path.display());
	}
}

#[test]
fn video_synthesis_when_ffmpeg_available() {
	if !media_fixtures::ffmpeg_available() {
		warn!("ffmpeg not found; skipping video fixture synthesis test");
		return;
	}

	let dir = tempfile::tempdir().unwrap();
	let clip = media_fixtures::synthesize_clip(dir.path(), "h264", 1)
		.expect("h264 clip synthesizes when ffmpeg present");

	let meta = std::fs::metadata(&clip).unwrap();
	assert!(meta.len() > 0, "synthesized clip should be non-empty");
	assert!(
		meta.len() < 512 * 1024,
		"synthesized clip should stay small, got {} bytes",
		meta.len()
	);
}
