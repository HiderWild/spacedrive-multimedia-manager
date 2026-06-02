//! Lossless-intent batch image rotation.
//!
//! Where transcode re-encodes videos and proxy builds preview variants, the
//! rotate system transforms the actual pixels of an image (90° clockwise or
//! counter-clockwise, 180°, or a horizontal/vertical flip) and writes the result
//! back to the source file. After rotating pixels it normalizes the EXIF
//! orientation tag to 1 (Top-Left) and carries the embedded ICC color profile
//! through the decode→rotate→encode round-trip so color rendering is unchanged.
//!
//! It exposes the same two shapes as transcode and proxy: a batch [`RotateJob`]
//! for whole-library runs and a [`RotateAction`] for a single file, both built on
//! the shared [`transform::rotate_file`] helper.

pub mod action;
pub mod config;
mod error;
pub mod job;
mod state;
pub mod transform;

mod jpeg_meta;

pub use action::{RotateAction, RotateInput, RotateOutput};
pub use config::{RotateJobConfig, RotateOp};
pub use error::{RotateError, RotateResult};
pub use job::{RotateJob, RotateJobOutput};
pub use state::{RotatePhase, RotateState};
pub use transform::{rotate_file, RotateInfo};
