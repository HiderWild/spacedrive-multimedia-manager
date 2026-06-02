//! Adaptive-streaming package generation (HLS and DASH).
//!
//! Where the transcode system re-encodes a video to a single output file, the
//! stream system packages a source video into an adaptive-streaming bundle: an
//! HLS master playlist with per-rendition variant playlists and media segments,
//! or a DASH `.mpd` manifest with templated segments. It exposes the same two
//! shapes as transcode, a batch [`StreamJob`] for whole-library runs and a
//! [`StreamAction`] for a single file, both built on the shared
//! [`StreamGenerator`].

pub mod action;
pub mod config;
mod error;
pub mod generator;
pub mod job;
mod state;

pub use action::{StreamAction, StreamInput, StreamOutput};
pub use config::{
	default_ladder, Rendition, SegmentType, StreamConfig, StreamJobConfig, StreamProtocol,
};
pub use error::{StreamError, StreamResult};
pub use generator::{StreamGenerator, StreamInfo};
pub use job::{StreamJob, StreamJobOutput};
pub use state::{StreamPhase, StreamState};
