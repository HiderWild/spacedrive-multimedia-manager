//! Rotation configuration.
//!
//! [`RotateOp`] enumerates the supported pixel transforms. A batch
//! [`RotateJobConfig`] layers the job-level concern of whether to dispatch a
//! thumbnail regeneration pass for the files it rewrites.

use serde::{Deserialize, Serialize};
use specta::Type;

/// A single pixel transform to apply to an image.
///
/// Variant names carry digits, which serde's `snake_case`/`lowercase` renaming
/// mangles into ugly TypeScript (`Cw90` → `cw_90`). Each variant is therefore
/// pinned with an explicit `#[serde(rename = ...)]` so the generated TS union is
/// exactly `"cw90" | "ccw90" | "rotate180" | "flip_h" | "flip_v"`. Specta reads
/// these serde attributes, so the Rust and TypeScript spellings stay in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum RotateOp {
	/// Rotate 90° clockwise. Swaps width and height.
	#[serde(rename = "cw90")]
	Cw90,
	/// Rotate 90° counter-clockwise. Swaps width and height.
	#[serde(rename = "ccw90")]
	Ccw90,
	/// Rotate 180°. Dimensions are unchanged.
	#[serde(rename = "rotate180")]
	Rotate180,
	/// Mirror horizontally (left↔right).
	#[serde(rename = "flip_h")]
	FlipH,
	/// Mirror vertically (top↔bottom).
	#[serde(rename = "flip_v")]
	FlipV,
}

impl RotateOp {
	/// Whether this operation swaps the image's width and height.
	pub fn swaps_dimensions(self) -> bool {
		matches!(self, RotateOp::Cw90 | RotateOp::Ccw90)
	}
}

/// Batch rotation job configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RotateJobConfig {
	/// Transform applied to every discovered image.
	pub op: RotateOp,
	/// Dispatch a thumbnail regeneration pass for rewritten files when the
	/// `ffmpeg` feature (which owns thumbnailing) is enabled.
	pub regenerate_thumbnails: bool,
}

impl RotateJobConfig {
	pub fn new(op: RotateOp) -> Self {
		Self {
			op,
			regenerate_thumbnails: true,
		}
	}
}

impl Default for RotateJobConfig {
	fn default() -> Self {
		Self::new(RotateOp::Cw90)
	}
}
