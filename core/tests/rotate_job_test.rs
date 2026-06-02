//! Exercises the in-place image rotation engine against Wave-0 fixtures.
//!
//! Writes a JPEG fixture with a known size, a non-trivial EXIF orientation, and
//! an embedded ICC profile, rotates it 90° clockwise, then verifies the three
//! guarantees task B-04 promises: the dimensions swap (W×H → H×W), the EXIF
//! orientation is normalized to Top-Left (1), and the ICC profile survives the
//! decode→rotate→encode round-trip.

use sd_core::ops::media::rotate::{rotate_file, RotateOp};
use sd_core::testing::media_fixtures::{
	self, read_exif_orientation, read_icc_profile, synthetic_icc_profile, ImageFormatKind,
};

#[test]
fn rotate_cw90_swaps_dimensions_normalizes_orientation_and_keeps_icc() {
	let dir = tempfile::tempdir().expect("tempdir");
	let path = dir.path().join("rotate_me.jpg");

	// Source: a 64x48 JPEG tagged with EXIF orientation 6 (Right-Top) plus an
	// embedded ICC profile, built with the Wave-0 fixture helpers.
	let (src_w, src_h) = (64u32, 48u32);
	let mut bytes = media_fixtures::encode_image(src_w, src_h, ImageFormatKind::Jpeg).unwrap();
	bytes = media_fixtures::inject_exif_orientation(&bytes, 6);
	let profile = synthetic_icc_profile();
	bytes = media_fixtures::inject_icc_profile(&bytes, &profile);
	std::fs::write(&path, &bytes).unwrap();

	// Sanity: the source really does carry orientation 6 and the ICC profile.
	let src_bytes = std::fs::read(&path).unwrap();
	assert_eq!(read_exif_orientation(&src_bytes), Some(6));
	assert_eq!(
		read_icc_profile(&src_bytes).as_deref(),
		Some(profile.as_slice())
	);

	let info = rotate_file(&path, RotateOp::Cw90).expect("rotation succeeds");

	// (a) Dimensions swapped: 64x48 -> 48x64.
	assert_eq!(info.width, src_h, "rotated width should be source height");
	assert_eq!(info.height, src_w, "rotated height should be source width");

	let out_bytes = std::fs::read(&path).unwrap();
	let decoded = image::load_from_memory(&out_bytes).expect("output decodes");
	assert_eq!(decoded.width(), src_h);
	assert_eq!(decoded.height(), src_w);

	// (b) EXIF orientation normalized to 1 (Top-Left).
	assert!(info.orientation_normalized);
	assert_eq!(
		read_exif_orientation(&out_bytes),
		Some(1),
		"orientation should be normalized to Top-Left"
	);

	// (c) ICC profile preserved through the round-trip.
	assert!(info.icc_preserved, "ICC should be reported as preserved");
	assert_eq!(
		read_icc_profile(&out_bytes).as_deref(),
		Some(profile.as_slice()),
		"ICC profile should survive rotation"
	);
}

#[test]
fn rotate_180_keeps_dimensions() {
	let dir = tempfile::tempdir().expect("tempdir");
	let path = dir.path().join("flip.png");

	let (w, h) = (40u32, 24u32);
	let bytes = media_fixtures::encode_image(w, h, ImageFormatKind::Png).unwrap();
	std::fs::write(&path, &bytes).unwrap();

	let info = rotate_file(&path, RotateOp::Rotate180).expect("rotation succeeds");
	assert_eq!(info.width, w);
	assert_eq!(info.height, h);
}
