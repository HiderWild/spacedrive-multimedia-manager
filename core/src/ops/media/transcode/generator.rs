//! Transcoding engine that drives ffmpeg.
//!
//! [`TranscodeGenerator`] turns a [`TranscodeConfig`] into an ffmpeg invocation
//! and runs it, mirroring the proxy generator's process plumbing (stderr is
//! drained for speed reporting, the child is awaited, and the output size is
//! returned). Encoder choice is delegated to
//! [`crate::ops::media::transcode::encoder`].

use super::{
	config::{TranscodeConfig, TranscodeResolution},
	encoder::{select_encoder, select_encoder_hw, select_encoder_with_hardware, EncoderSelection},
	error::{TranscodeError, TranscodeResult},
	hwaccel::{AvailableEncoders, HwAccel},
};
use crate::ops::media::proxy::{detect_hardware_accel, HardwareAccel};
use serde::{Deserialize, Serialize};
use std::{ffi::OsString, path::Path, process::Stdio, time::Instant};
use tokio::{
	io::{AsyncBufReadExt, BufReader},
	process::Command,
};
use tracing::{debug, info, warn};

/// Information about a completed transcode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodeInfo {
	pub size_bytes: u64,
	pub encoding_time_secs: u64,
	pub average_speed_multiplier: f32,
}

/// True when an explicit (non-Auto, non-None) B-02 backend is requested, which
/// is the only case that needs a capability probe of the local ffmpeg.
fn explicit_backend(hw_accel: HwAccel) -> bool {
	!matches!(hw_accel, HwAccel::Auto | HwAccel::None)
}

/// Transcoder built from a single output config.
pub struct TranscodeGenerator {
	config: TranscodeConfig,
	hardware_accel: Option<HardwareAccel>,
	available_encoders: AvailableEncoders,
}

impl TranscodeGenerator {
	pub fn new(config: TranscodeConfig) -> Self {
		let hardware_accel = if config.use_hardware_accel {
			detect_hardware_accel()
		} else {
			None
		};

		// Probe ffmpeg's encoder list only when an explicit B-02 backend is
		// requested; Auto stays on the B-01 path and None forces CPU.
		let available_encoders = if explicit_backend(config.hw_accel) {
			AvailableEncoders::detect()
		} else {
			AvailableEncoders::default()
		};

		Self {
			config,
			hardware_accel,
			available_encoders,
		}
	}

	#[cfg(test)]
	pub(crate) fn new_with_hardware(
		config: TranscodeConfig,
		hardware_accel: Option<HardwareAccel>,
	) -> Self {
		Self {
			config,
			hardware_accel,
			available_encoders: AvailableEncoders::default(),
		}
	}

	#[cfg(test)]
	pub(crate) fn new_with_available(
		config: TranscodeConfig,
		available_encoders: AvailableEncoders,
	) -> Self {
		Self {
			config,
			hardware_accel: None,
			available_encoders,
		}
	}

	/// Encode `input` to `output` using this generator's config.
	pub async fn generate(
		&self,
		input: impl AsRef<Path>,
		output: impl AsRef<Path>,
	) -> TranscodeResult<TranscodeInfo> {
		let input = input.as_ref();
		let output = output.as_ref();

		self.config.validate()?;

		debug!(
			"Transcoding {:?}/{:?}: {} -> {}",
			self.config.codec,
			self.config.container,
			input.display(),
			output.display()
		);

		if let Some(parent) = output.parent() {
			tokio::fs::create_dir_all(parent).await?;
		}

		let args = self.build_ffmpeg_args(input, output)?;

		match self.run_ffmpeg_args(output, args).await {
			Err(TranscodeError::FFmpegProcessFailed(_)) if self.hardware_accel.is_some() => {
				warn!(
					"Hardware transcode failed for {}; retrying with CPU encoder",
					input.display()
				);
				let _ = tokio::fs::remove_file(output).await;

				let mut cpu_config = self.config.clone();
				cpu_config.use_hardware_accel = false;
				cpu_config.hw_accel = HwAccel::None;
				let cpu = Self {
					config: cpu_config,
					hardware_accel: None,
					available_encoders: AvailableEncoders::default(),
				};
				cpu.run_ffmpeg_args(output, cpu.build_ffmpeg_args(input, output)?)
					.await
			}
			result => result,
		}
	}

	async fn run_ffmpeg_args(
		&self,
		output: &Path,
		args: Vec<OsString>,
	) -> TranscodeResult<TranscodeInfo> {
		let mut cmd = Command::new("ffmpeg");
		cmd.args(args)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::piped());

		debug!("Executing ffmpeg command: {:?}", cmd);

		let mut child = cmd.spawn().map_err(|_| TranscodeError::FFmpegNotFound)?;

		let start_time = Instant::now();
		let mut last_speed = 0.0f32;

		if let Some(stderr) = child.stderr.take() {
			let reader = BufReader::new(stderr);
			let mut lines = reader.lines();
			while let Ok(Some(line)) = lines.next_line().await {
				if let Some(speed) = parse_speed_from_line(&line) {
					last_speed = speed;
				}
			}
		}

		let status = child
			.wait()
			.await
			.map_err(|e| TranscodeError::Other(format!("Failed to wait for ffmpeg: {}", e)))?;

		if !status.success() {
			let code = status.code().unwrap_or(-1);
			return Err(TranscodeError::FFmpegProcessFailed(code));
		}

		let encoding_time = start_time.elapsed();
		let metadata = tokio::fs::metadata(output).await?;
		let size_bytes = metadata.len();

		info!(
			"Transcoded {:?}: {} bytes in {:.1}s ({:.1}x realtime)",
			self.config.codec,
			size_bytes,
			encoding_time.as_secs_f32(),
			last_speed
		);

		Ok(TranscodeInfo {
			size_bytes,
			encoding_time_secs: encoding_time.as_secs(),
			average_speed_multiplier: last_speed,
		})
	}

	/// Resolve the encoder for this config.
	///
	/// `hw_accel == Auto` keeps the B-01 behavior (Vulkan when
	/// `use_hardware_accel` detected one, else CPU). An explicit B-02 backend
	/// goes through capability-checked selection and may error when forced
	/// hardware is unavailable; `None` forces CPU.
	fn select(&self) -> TranscodeResult<EncoderSelection> {
		match self.config.hw_accel {
			HwAccel::Auto => Ok(match self.hardware_accel {
				Some(hw) if self.config.use_hardware_accel => select_encoder_with_hardware(
					self.config.codec,
					self.config.quality,
					&self.config.preset,
					Some(hw),
				),
				_ => select_encoder(
					self.config.codec,
					self.config.quality,
					&self.config.preset,
					false,
				),
			}),
			other => select_encoder_hw(
				self.config.codec,
				self.config.quality,
				&self.config.preset,
				other,
				&self.available_encoders,
			),
		}
	}

	/// Build the ffmpeg argument vector for this config.
	fn build_ffmpeg_args(&self, input: &Path, output: &Path) -> TranscodeResult<Vec<OsString>> {
		let mut args: Vec<OsString> = Vec::new();
		let selection = self.select()?;

		args.push("-y".into());
		args.extend(selection.input_args.iter().cloned());
		args.push("-i".into());
		args.push(input.as_os_str().to_owned());

		if let Some(vf) = self.video_filter(selection.video_filter_suffix) {
			args.push("-vf".into());
			args.push(vf.into());
		}

		args.push("-c:v".into());
		args.push(selection.encoder.into());
		args.extend(selection.video_args);

		// Audio: re-encode to a container-appropriate codec. Sources without an
		// audio stream simply produce no audio output.
		args.push("-c:a".into());
		args.push(self.config.container.audio_encoder().into());
		args.push("-b:a".into());
		args.push("128k".into());

		if matches!(
			self.config.container,
			super::config::TranscodeContainer::Mp4
		) {
			args.push("-movflags".into());
			args.push("+faststart".into());
		}

		args.push(output.as_os_str().to_owned());
		Ok(args)
	}

	/// Build the `-vf` scale expression, or `None` to keep the source size.
	fn video_filter(&self, suffix: Option<&str>) -> Option<String> {
		let scale = match self.config.resolution {
			TranscodeResolution::Keep => None,
			TranscodeResolution::MaxDimension(dim) => Some(format!(
				"scale=w='min({dim},iw)':h='min({dim},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
			)),
			TranscodeResolution::Scale { width, height } => {
				Some(format!("scale={width}:{height}"))
			}
		};

		match (scale, suffix) {
			(Some(scale), Some(suffix)) => Some(format!("{scale},{suffix}")),
			(Some(scale), None) => Some(scale),
			(None, Some(suffix)) => Some(suffix.to_string()),
			(None, None) => None,
		}
	}
}

/// Parse encoding speed from an ffmpeg progress line (`... speed=51.2x ...`).
fn parse_speed_from_line(line: &str) -> Option<f32> {
	if let Some(speed_idx) = line.find("speed=") {
		let speed_str = &line[speed_idx + 6..];
		if let Some(x_idx) = speed_str.find('x') {
			return speed_str[..x_idx].trim().parse::<f32>().ok();
		}
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ops::media::transcode::config::{
		TranscodeCodec, TranscodeContainer, TranscodeQuality,
	};

	fn arg_strings(args: &[OsString]) -> Vec<String> {
		args.iter()
			.map(|a| a.to_string_lossy().into_owned())
			.collect()
	}

	#[test]
	fn h264_args_use_libx264_and_crf() {
		let gen = TranscodeGenerator::new(
			TranscodeConfig::new(TranscodeCodec::H264).with_quality(TranscodeQuality::Crf(20)),
		);
		let args = arg_strings(
			&gen.build_ffmpeg_args(Path::new("in.mp4"), Path::new("out.mp4"))
				.unwrap(),
		);
		assert!(args.windows(2).any(|w| w[0] == "-c:v" && w[1] == "libx264"));
		assert!(args.windows(2).any(|w| w[0] == "-crf" && w[1] == "20"));
		assert!(args.contains(&"+faststart".to_string()));
	}

	#[test]
	fn webm_uses_opus_audio() {
		let gen = TranscodeGenerator::new(
			TranscodeConfig::new(TranscodeCodec::Vp9).with_container(TranscodeContainer::Webm),
		);
		let args = arg_strings(
			&gen.build_ffmpeg_args(Path::new("in.mp4"), Path::new("out.webm"))
				.unwrap(),
		);
		assert!(args.windows(2).any(|w| w[0] == "-c:a" && w[1] == "libopus"));
		assert!(!args.contains(&"+faststart".to_string()));
	}

	#[test]
	fn max_dimension_emits_scale_filter() {
		let gen = TranscodeGenerator::new(
			TranscodeConfig::new(TranscodeCodec::H264)
				.with_resolution(TranscodeResolution::MaxDimension(480)),
		);
		let vf = gen.video_filter(None).unwrap();
		assert!(vf.contains("480"));
		assert!(vf.contains("force_divisible_by=2"));
	}

	#[test]
	fn speed_parsing() {
		let line =
			"frame=10 fps=10 q=28.0 size=8kB time=00:00:01.00 bitrate=65.5kbits/s speed=12.3x";
		assert_eq!(parse_speed_from_line(line), Some(12.3));
	}

	#[test]
	fn hardware_h264_args_use_vulkan_upload_pipeline_when_available() {
		let mut cfg = TranscodeConfig::new(TranscodeCodec::H264)
			.with_resolution(TranscodeResolution::MaxDimension(720))
			.with_quality(TranscodeQuality::Crf(25));
		cfg.use_hardware_accel = true;

		let gen = TranscodeGenerator::new_with_hardware(cfg, Some(HardwareAccel::Vulkan));
		let args = arg_strings(
			&gen.build_ffmpeg_args(Path::new("in.mp4"), Path::new("out.mp4"))
				.unwrap(),
		);

		assert!(args
			.windows(2)
			.any(|w| w[0] == "-init_hw_device" && w[1] == "vulkan=vk"));
		assert!(args
			.windows(2)
			.any(|w| w[0] == "-filter_hw_device" && w[1] == "vk"));
		assert!(args
			.windows(2)
			.any(|w| w[0] == "-c:v" && w[1] == "h264_vulkan"));

		let vf = args
			.windows(2)
			.find(|w| w[0] == "-vf")
			.map(|w| w[1].clone())
			.expect("video filter");
		assert!(vf.contains("format=nv12,hwupload"));
		assert_eq!(vf.matches("hwupload").count(), 1);
	}
}
