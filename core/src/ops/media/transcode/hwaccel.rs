//! GPU-accelerated encoder selection (task B-02).
//!
//! Where the B-01 encoder picks a CPU encoder (`libx264` and friends), this
//! layer adds runtime capability detection for the common hardware encoder
//! families and resolves a requested [`HwAccel`] preference into a concrete
//! ffmpeg encoder, falling back to the CPU encoder when no accelerator is
//! available.
//!
//! Detection is kept injectable: [`AvailableEncoders`] can be built from a real
//! `ffmpeg -encoders` probe ([`AvailableEncoders::detect`]) or from an explicit
//! set of names ([`AvailableEncoders::from_names`]) so selection is deterministic
//! and testable without a GPU on the host.

use super::config::TranscodeCodec;
use super::error::{TranscodeError, TranscodeResult};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashSet;
use std::ffi::OsString;
use tracing::{debug, warn};

/// Requested hardware acceleration backend.
///
/// `Auto` (the default) picks the best available accelerator for the codec and
/// silently falls back to CPU when none is present. `None` forces the CPU
/// encoder. A specific family forces that backend and errors when it is not
/// available (see [`resolve_hw_encoder`]).
///
/// Serialized lowercase (`none|nvenc|qsv|amf|videotoolbox|auto`) so the generated
/// TypeScript stays free of serde digit-mangling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum HwAccel {
	/// Force the CPU encoder.
	None,
	/// NVIDIA NVENC (`*_nvenc`).
	Nvenc,
	/// Intel QuickSync (`*_qsv`).
	Qsv,
	/// AMD AMF (`*_amf`).
	Amf,
	/// Apple VideoToolbox (`*_videotoolbox`).
	Videotoolbox,
	/// Pick the best available accelerator, else CPU.
	#[default]
	Auto,
}

/// A concrete hardware encoder family (no `None`/`Auto` placeholders).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwFamily {
	Nvenc,
	Qsv,
	Amf,
	Videotoolbox,
}

impl HwFamily {
	/// Priority order tried under [`HwAccel::Auto`]. NVENC first (broadest codec
	/// support and quality), then QSV, AMF, and VideoToolbox last.
	const AUTO_ORDER: [HwFamily; 4] = [
		HwFamily::Nvenc,
		HwFamily::Qsv,
		HwFamily::Amf,
		HwFamily::Videotoolbox,
	];

	/// ffmpeg encoder name for this family and codec, or `None` when the family
	/// has no hardware encoder for the codec.
	pub fn encoder_name(self, codec: TranscodeCodec) -> Option<&'static str> {
		match (self, codec) {
			(HwFamily::Nvenc, TranscodeCodec::H264) => Some("h264_nvenc"),
			(HwFamily::Nvenc, TranscodeCodec::Hevc) => Some("hevc_nvenc"),
			(HwFamily::Nvenc, TranscodeCodec::Av1) => Some("av1_nvenc"),
			(HwFamily::Qsv, TranscodeCodec::H264) => Some("h264_qsv"),
			(HwFamily::Qsv, TranscodeCodec::Hevc) => Some("hevc_qsv"),
			(HwFamily::Qsv, TranscodeCodec::Av1) => Some("av1_qsv"),
			(HwFamily::Amf, TranscodeCodec::H264) => Some("h264_amf"),
			(HwFamily::Amf, TranscodeCodec::Hevc) => Some("hevc_amf"),
			(HwFamily::Videotoolbox, TranscodeCodec::H264) => Some("h264_videotoolbox"),
			(HwFamily::Videotoolbox, TranscodeCodec::Hevc) => Some("hevc_videotoolbox"),
			// VP9 has no widely-shipped hardware encoder; AMF/VideoToolbox have no
			// AV1 encode. Those combinations fall back to CPU.
			_ => None,
		}
	}
}

impl HwAccel {
	/// The concrete family for an explicit request, or `None` for `Auto`/`None`.
	fn explicit_family(self) -> Option<HwFamily> {
		match self {
			HwAccel::Nvenc => Some(HwFamily::Nvenc),
			HwAccel::Qsv => Some(HwFamily::Qsv),
			HwAccel::Amf => Some(HwFamily::Amf),
			HwAccel::Videotoolbox => Some(HwFamily::Videotoolbox),
			HwAccel::None | HwAccel::Auto => None,
		}
	}
}

/// The set of encoder names this ffmpeg build advertises.
///
/// Built from `ffmpeg -encoders` output (real probe) or an explicit name set
/// (tests), so resolution never depends on the host GPU.
#[derive(Debug, Clone, Default)]
pub struct AvailableEncoders {
	names: HashSet<String>,
}

impl AvailableEncoders {
	/// Build from an explicit set of encoder names. Used in tests to inject a
	/// deterministic capability set.
	pub fn from_names<I, S>(names: I) -> Self
	where
		I: IntoIterator<Item = S>,
		S: Into<String>,
	{
		Self {
			names: names.into_iter().map(Into::into).collect(),
		}
	}

	/// Parse the names column from `ffmpeg -hide_banner -encoders` output.
	///
	/// Each encoder line looks like ` V....D h264_nvenc  NVIDIA NVENC H.264`.
	/// The second whitespace-delimited token is the encoder name.
	pub fn from_ffmpeg_encoders_output(output: &str) -> Self {
		let mut names = HashSet::new();
		for line in output.lines() {
			let trimmed = line.trim_start();
			// Encoder rows start with the capability flag column (e.g. "V....D");
			// skip the header lines ("Encoders:", "------", legends).
			let mut tokens = trimmed.split_whitespace();
			let Some(flags) = tokens.next() else {
				continue;
			};
			if !flags.chars().all(|c| c == '.' || c.is_ascii_uppercase()) || flags.len() < 6 {
				continue;
			}
			if let Some(name) = tokens.next() {
				names.insert(name.to_string());
			}
		}
		Self { names }
	}

	/// Probe the local ffmpeg for its encoder list. Returns an empty set when the
	/// probe fails so resolution falls back to CPU.
	pub fn detect() -> Self {
		match crate::ops::media::ffmpeg_bin::command()
			.args(["-hide_banner", "-encoders"])
			.output()
		{
			Ok(out) if out.status.success() => {
				Self::from_ffmpeg_encoders_output(&String::from_utf8_lossy(&out.stdout))
			}
			Ok(_) => {
				warn!("ffmpeg -encoders returned a non-zero status; assuming no hardware encoders");
				Self::default()
			}
			Err(e) => {
				warn!("failed to probe ffmpeg encoders: {e}; assuming no hardware encoders");
				Self::default()
			}
		}
	}

	pub fn contains(&self, name: &str) -> bool {
		self.names.contains(name)
	}
}

/// A resolved hardware encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedHwEncoder {
	pub family: HwFamily,
	pub encoder: &'static str,
}

/// Resolve a hardware preference into a concrete encoder.
///
/// Precedence:
/// - [`HwAccel::None`] -> `Ok(None)` (caller uses the CPU encoder).
/// - [`HwAccel::Auto`] -> first available family for the codec, else `Ok(None)`
///   (CPU fallback). Never errors.
/// - explicit family -> the family's encoder when available, otherwise
///   `Err(TranscodeError::HardwareAccelUnavailable)`. Forcing a backend that the
///   build cannot satisfy is treated as a hard error rather than a silent CPU
///   downgrade, so callers who explicitly demand hardware learn it is missing.
pub fn resolve_hw_encoder(
	codec: TranscodeCodec,
	preference: HwAccel,
	available: &AvailableEncoders,
) -> TranscodeResult<Option<ResolvedHwEncoder>> {
	match preference {
		HwAccel::None => Ok(None),
		HwAccel::Auto => {
			for family in HwFamily::AUTO_ORDER {
				if let Some(encoder) = family.encoder_name(codec) {
					if available.contains(encoder) {
						debug!("Auto-selected hardware encoder {encoder} for {codec:?}");
						return Ok(Some(ResolvedHwEncoder { family, encoder }));
					}
				}
			}
			debug!("No hardware encoder available for {codec:?}; using CPU encoder");
			Ok(None)
		}
		_ => {
			let family = preference
				.explicit_family()
				.expect("None/Auto handled above");
			match family.encoder_name(codec) {
				Some(encoder) if available.contains(encoder) => {
					Ok(Some(ResolvedHwEncoder { family, encoder }))
				}
				Some(encoder) => Err(TranscodeError::HardwareAccelUnavailable(format!(
					"{encoder} ({preference:?}) is not available in this ffmpeg build"
				))),
				None => Err(TranscodeError::HardwareAccelUnavailable(format!(
					"{preference:?} has no hardware encoder for {codec:?}"
				))),
			}
		}
	}
}

fn push(args: &mut Vec<OsString>, value: impl Into<OsString>) {
	args.push(value.into());
}

/// Rate-control arguments for a hardware encoder family.
///
/// Hardware encoders do not share libx264's `-crf`/`-b:v` semantics, so CRF and
/// bitrate are mapped onto each family's native rate-control flags:
/// - NVENC: CRF -> `-rc vbr -cq N -b:v 0`; bitrate -> `-rc vbr -b:v Nk`.
/// - QSV: CRF -> `-global_quality N`; bitrate -> `-b:v Nk`.
/// - AMF: CRF -> `-rc cqp -qp_i N -qp_p N -qp_b N`; bitrate -> `-rc vbr_latency -b:v Nk`.
/// - VideoToolbox: CRF -> `-q:v N` (approximate, VT lacks true CRF); bitrate -> `-b:v Nk`.
pub fn hw_rate_control_args(
	family: HwFamily,
	quality: super::config::TranscodeQuality,
) -> Vec<OsString> {
	use super::config::TranscodeQuality;

	let mut args = Vec::new();
	match (family, quality) {
		(HwFamily::Nvenc, TranscodeQuality::Crf(crf)) => {
			push(&mut args, "-rc");
			push(&mut args, "vbr");
			push(&mut args, "-cq");
			push(&mut args, crf.min(51).to_string());
			push(&mut args, "-b:v");
			push(&mut args, "0");
		}
		(HwFamily::Nvenc, TranscodeQuality::Bitrate(kbps)) => {
			push(&mut args, "-rc");
			push(&mut args, "vbr");
			push(&mut args, "-b:v");
			push(&mut args, format!("{kbps}k"));
		}
		(HwFamily::Qsv, TranscodeQuality::Crf(crf)) => {
			push(&mut args, "-global_quality");
			push(&mut args, crf.min(51).to_string());
		}
		(HwFamily::Qsv, TranscodeQuality::Bitrate(kbps)) => {
			push(&mut args, "-b:v");
			push(&mut args, format!("{kbps}k"));
		}
		(HwFamily::Amf, TranscodeQuality::Crf(crf)) => {
			let qp = crf.min(51).to_string();
			push(&mut args, "-rc");
			push(&mut args, "cqp");
			push(&mut args, "-qp_i");
			push(&mut args, qp.clone());
			push(&mut args, "-qp_p");
			push(&mut args, qp.clone());
			push(&mut args, "-qp_b");
			push(&mut args, qp);
		}
		(HwFamily::Amf, TranscodeQuality::Bitrate(kbps)) => {
			push(&mut args, "-rc");
			push(&mut args, "vbr_latency");
			push(&mut args, "-b:v");
			push(&mut args, format!("{kbps}k"));
		}
		(HwFamily::Videotoolbox, TranscodeQuality::Crf(crf)) => {
			push(&mut args, "-q:v");
			push(&mut args, crf.min(100).to_string());
		}
		(HwFamily::Videotoolbox, TranscodeQuality::Bitrate(kbps)) => {
			push(&mut args, "-b:v");
			push(&mut args, format!("{kbps}k"));
		}
	}
	push(&mut args, "-pix_fmt");
	push(&mut args, "yuv420p");
	args
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ops::media::transcode::config::TranscodeQuality;

	fn nvenc_only() -> AvailableEncoders {
		AvailableEncoders::from_names(["libx264", "h264_nvenc", "hevc_nvenc"])
	}

	#[test]
	fn parses_encoder_names_from_ffmpeg_output() {
		let output = "Encoders:\n V..... = Video\n ------\n V....D libx264              libx264 H.264\n V....D h264_nvenc           NVIDIA NVENC H.264\n";
		let av = AvailableEncoders::from_ffmpeg_encoders_output(output);
		assert!(av.contains("libx264"));
		assert!(av.contains("h264_nvenc"));
		assert!(!av.contains("Video"));
	}

	#[test]
	fn auto_picks_nvenc_when_available_for_h264() {
		let resolved =
			resolve_hw_encoder(TranscodeCodec::H264, HwAccel::Auto, &nvenc_only()).unwrap();
		assert_eq!(
			resolved.map(|r| r.encoder),
			Some("h264_nvenc"),
			"Auto must select h264_nvenc when present"
		);
	}

	#[test]
	fn auto_falls_back_to_cpu_when_no_hardware() {
		let cpu_only = AvailableEncoders::from_names(["libx264", "libx265"]);
		let resolved = resolve_hw_encoder(TranscodeCodec::H264, HwAccel::Auto, &cpu_only).unwrap();
		assert!(resolved.is_none(), "Auto with no hardware must yield CPU");
	}

	#[test]
	fn forced_unavailable_backend_errors() {
		let cpu_only = AvailableEncoders::from_names(["libx264"]);
		let err = resolve_hw_encoder(TranscodeCodec::H264, HwAccel::Nvenc, &cpu_only).unwrap_err();
		assert!(matches!(err, TranscodeError::HardwareAccelUnavailable(_)));
	}

	#[test]
	fn none_preference_uses_cpu() {
		let resolved =
			resolve_hw_encoder(TranscodeCodec::H264, HwAccel::None, &nvenc_only()).unwrap();
		assert!(resolved.is_none());
	}

	#[test]
	fn nvenc_crf_maps_to_cq() {
		let args: Vec<String> = hw_rate_control_args(HwFamily::Nvenc, TranscodeQuality::Crf(28))
			.iter()
			.map(|a| a.to_string_lossy().into_owned())
			.collect();
		assert!(args.windows(2).any(|w| w[0] == "-cq" && w[1] == "28"));
	}

	#[test]
	fn qsv_crf_maps_to_global_quality() {
		let args: Vec<String> = hw_rate_control_args(HwFamily::Qsv, TranscodeQuality::Crf(24))
			.iter()
			.map(|a| a.to_string_lossy().into_owned())
			.collect();
		assert!(args
			.windows(2)
			.any(|w| w[0] == "-global_quality" && w[1] == "24"));
	}
}
