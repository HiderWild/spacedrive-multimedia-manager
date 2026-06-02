//! Adaptive-streaming configuration: protocol, segment shape, and the bitrate
//! ladder.
//!
//! [`StreamConfig`] describes how a source video is packaged for adaptive
//! streaming: which protocol to emit ([`StreamProtocol::Hls`] or
//! [`StreamProtocol::Dash`]), how long each media segment runs, the HLS segment
//! container ([`SegmentType`]), and the set of [`Rendition`]s that make up the
//! adaptive bitrate ladder. The job-level [`StreamJobConfig`] layers batch
//! concerns on top (whether to overwrite existing packages and where to write
//! them).

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;

/// Adaptive-streaming protocol to emit.
///
/// Serialized lowercase (`hls|dash`) so the generated TypeScript union stays
/// free of serde digit/word mangling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum StreamProtocol {
	/// HTTP Live Streaming: a master `.m3u8`, per-rendition variant playlists,
	/// and media segments.
	#[default]
	Hls,
	/// MPEG-DASH: an `.mpd` manifest plus templated segment files.
	Dash,
}

impl StreamProtocol {
	/// File name of the top-level manifest a package of this protocol produces.
	pub fn manifest_name(&self) -> &'static str {
		match self {
			Self::Hls => "master.m3u8",
			Self::Dash => "manifest.mpd",
		}
	}
}

/// HLS media-segment container.
///
/// Serialized lowercase (`ts|fmp4`) to keep the TypeScript union clean. DASH
/// always uses fragmented MP4 segments regardless of this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum SegmentType {
	/// MPEG-TS segments (`.ts`). The most broadly compatible HLS container.
	#[default]
	Ts,
	/// Fragmented MP4 segments (`.m4s`).
	Fmp4,
}

impl SegmentType {
	/// File extension used for media segments of this type.
	pub fn extension(&self) -> &'static str {
		match self {
			Self::Ts => "ts",
			Self::Fmp4 => "m4s",
		}
	}
}

/// One rung of the adaptive bitrate ladder.
///
/// The width is derived from the source aspect ratio at encode time (the scale
/// filter uses `-2` for width) so only the target `height` and the video
/// `bitrate_kbps` need to be specified. `name` is a human label used for the
/// per-rendition output file stem (e.g. `1080p`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Rendition {
	/// Human label and output-file stem, e.g. `720p`.
	pub name: String,
	/// Target output height in pixels; width preserves the source aspect ratio.
	pub height: u32,
	/// Target average video bitrate in kilobits per second.
	pub bitrate_kbps: u32,
}

impl Rendition {
	pub fn new(name: impl Into<String>, height: u32, bitrate_kbps: u32) -> Self {
		Self {
			name: name.into(),
			height,
			bitrate_kbps,
		}
	}

	/// VBV peak rate ffmpeg should respect (`-maxrate`), ~7% over the target.
	pub fn maxrate_kbps(&self) -> u32 {
		self.bitrate_kbps * 107 / 100
	}

	/// VBV buffer size ffmpeg should use (`-bufsize`), 1.5x the target bitrate.
	pub fn bufsize_kbps(&self) -> u32 {
		self.bitrate_kbps * 3 / 2
	}
}

/// The default adaptive ladder: 1080p, 720p, and 480p with broadcast-typical
/// H.264 bitrates. Ordered highest-quality first so the HLS master lists the
/// best rendition at the top.
pub fn default_ladder() -> Vec<Rendition> {
	vec![
		Rendition::new("1080p", 1080, 5000),
		Rendition::new("720p", 720, 2800),
		Rendition::new("480p", 480, 1400),
	]
}

/// Full description of one streaming package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct StreamConfig {
	/// Protocol to emit.
	pub protocol: StreamProtocol,
	/// Target media-segment duration in seconds.
	pub segment_duration: u32,
	/// Adaptive bitrate ladder. Each entry becomes one rendition in the package.
	pub ladder: Vec<Rendition>,
	/// HLS segment container. Ignored for DASH (always fragmented MP4).
	pub segment_type: SegmentType,
	/// libx264 speed preset used for every rendition.
	pub preset: String,
}

impl StreamConfig {
	/// Build a config for `protocol` with the default ladder, 6-second segments,
	/// MPEG-TS HLS segments, and the `veryfast` preset.
	pub fn new(protocol: StreamProtocol) -> Self {
		Self {
			protocol,
			segment_duration: 6,
			ladder: default_ladder(),
			segment_type: SegmentType::default(),
			preset: "veryfast".to_string(),
		}
	}

	pub fn with_ladder(mut self, ladder: Vec<Rendition>) -> Self {
		self.ladder = ladder;
		self
	}

	pub fn with_segment_duration(mut self, seconds: u32) -> Self {
		self.segment_duration = seconds;
		self
	}

	pub fn with_segment_type(mut self, segment_type: SegmentType) -> Self {
		self.segment_type = segment_type;
		self
	}

	pub fn with_preset(mut self, preset: impl Into<String>) -> Self {
		self.preset = preset.into();
		self
	}

	/// Name of the top-level manifest this package produces.
	pub fn manifest_name(&self) -> &'static str {
		self.protocol.manifest_name()
	}

	/// Reject configurations the generator cannot package.
	pub fn validate(&self) -> Result<(), super::error::StreamError> {
		if self.ladder.is_empty() {
			return Err(super::error::StreamError::InvalidConfig(
				"rendition ladder must contain at least one rendition".into(),
			));
		}
		if self.segment_duration == 0 {
			return Err(super::error::StreamError::InvalidConfig(
				"segment_duration must be greater than zero".into(),
			));
		}
		Ok(())
	}
}

impl Default for StreamConfig {
	fn default() -> Self {
		Self::new(StreamProtocol::Hls)
	}
}

/// Configuration for a batch streaming-package job.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct StreamJobConfig {
	/// Protocol/ladder/segment shape to produce for each discovered video.
	pub output: StreamConfig,
	/// Re-package even when a manifest already exists on disk.
	pub regenerate: bool,
	/// Directory for output packages. When `None` the job writes under
	/// `<library>/streams`.
	pub output_dir: Option<PathBuf>,
}

impl StreamJobConfig {
	pub fn new(output: StreamConfig) -> Self {
		Self {
			output,
			regenerate: false,
			output_dir: None,
		}
	}
}

impl Default for StreamJobConfig {
	fn default() -> Self {
		Self::new(StreamConfig::default())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_ladder_is_descending() {
		let ladder = default_ladder();
		assert_eq!(ladder.len(), 3);
		assert_eq!(ladder[0].height, 1080);
		assert!(ladder[0].bitrate_kbps > ladder[2].bitrate_kbps);
	}

	#[test]
	fn manifest_names_match_protocol() {
		assert_eq!(StreamProtocol::Hls.manifest_name(), "master.m3u8");
		assert_eq!(StreamProtocol::Dash.manifest_name(), "manifest.mpd");
	}

	#[test]
	fn empty_ladder_is_rejected() {
		let cfg = StreamConfig::new(StreamProtocol::Hls).with_ladder(Vec::new());
		assert!(cfg.validate().is_err());
	}

	#[test]
	fn config_round_trips_messagepack() {
		let cfg = StreamConfig::new(StreamProtocol::Dash)
			.with_segment_duration(4)
			.with_ladder(vec![Rendition::new("720p", 720, 2500)]);
		let bytes = rmp_serde::to_vec_named(&cfg).unwrap();
		let back: StreamConfig = rmp_serde::from_slice(&bytes).unwrap();
		assert_eq!(cfg, back);
	}

	#[test]
	fn protocol_serializes_lowercase() {
		assert_eq!(
			serde_json::to_string(&StreamProtocol::Hls).unwrap(),
			"\"hls\""
		);
		assert_eq!(
			serde_json::to_string(&StreamProtocol::Dash).unwrap(),
			"\"dash\""
		);
		assert_eq!(
			serde_json::to_string(&SegmentType::Fmp4).unwrap(),
			"\"fmp4\""
		);
	}
}
