//! Streaming-package engine that drives ffmpeg.
//!
//! [`StreamGenerator`] turns a [`StreamConfig`] into one or more ffmpeg
//! invocations and runs them to produce an on-disk adaptive-streaming package.
//! It mirrors the transcode generator's process plumbing: stderr is drained for
//! progress, the child is awaited, and the produced files are summarized.
//!
//! HLS is packaged one rendition at a time so an interrupted run can skip
//! variant playlists that already exist, after which the master playlist is
//! (re)assembled by hand. DASH is packaged in a single ffmpeg call that emits
//! every rendition into one `.mpd` (the format cannot be appended to
//! incrementally), so its resumable unit is the whole package.

use super::{
	config::{Rendition, SegmentType, StreamConfig, StreamProtocol},
	error::{StreamError, StreamResult},
};
use serde::{Deserialize, Serialize};
use std::{
	ffi::OsString,
	path::{Path, PathBuf},
	process::Stdio,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, info};

/// Audio bitrate (kbps) attached to every rendition that has source audio.
const AUDIO_BITRATE_KBPS: u32 = 128;

/// Summary of a produced streaming package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
	/// Absolute path of the top-level manifest (`master.m3u8` or `manifest.mpd`).
	pub manifest_path: PathBuf,
	/// Number of renditions in the package's ladder.
	pub rendition_count: usize,
	/// Number of media segment files on disk.
	pub segment_count: usize,
	/// Renditions actually (re)encoded this run.
	pub renditions_encoded: usize,
	/// Renditions skipped because their variant playlist already existed (HLS).
	pub renditions_skipped: usize,
	/// Total size of all package files in bytes.
	pub total_size_bytes: u64,
}

/// Streaming packager built from a single output config.
pub struct StreamGenerator {
	config: StreamConfig,
	/// Re-encode renditions/packages even when their outputs already exist.
	regenerate: bool,
}

impl StreamGenerator {
	pub fn new(config: StreamConfig) -> Self {
		Self {
			config,
			regenerate: false,
		}
	}

	/// Build a generator that overwrites existing renditions/packages.
	pub fn new_regenerating(config: StreamConfig) -> Self {
		Self {
			config,
			regenerate: true,
		}
	}

	/// Package `input` into `package_dir` using this generator's config.
	pub async fn generate(
		&self,
		input: impl AsRef<Path>,
		package_dir: impl AsRef<Path>,
	) -> StreamResult<StreamInfo> {
		let input = input.as_ref();
		let package_dir = package_dir.as_ref();

		self.config.validate()?;
		tokio::fs::create_dir_all(package_dir).await?;

		debug!(
			"Packaging {:?} ({} renditions): {} -> {}",
			self.config.protocol,
			self.config.ladder.len(),
			input.display(),
			package_dir.display()
		);

		match self.config.protocol {
			StreamProtocol::Hls => self.generate_hls(input, package_dir).await,
			StreamProtocol::Dash => self.generate_dash(input, package_dir).await,
		}
	}

	/// Stem of the variant playlist for rendition `idx`.
	fn variant_stem(idx: usize) -> String {
		format!("stream_{idx}")
	}

	/// Package an HLS master playlist plus one variant playlist per rendition.
	async fn generate_hls(&self, input: &Path, dir: &Path) -> StreamResult<StreamInfo> {
		let has_audio = probe_has_audio(input);
		let source_dims = probe_dimensions(input);

		let mut encoded = 0usize;
		let mut skipped = 0usize;

		for (idx, rendition) in self.config.ladder.iter().enumerate() {
			let variant = dir.join(format!("{}.m3u8", Self::variant_stem(idx)));

			// Resumable skip: a re-run leaves completed variant playlists in place
			// and only encodes the renditions still missing.
			if variant.exists() && !self.regenerate {
				debug!("Skipping existing rendition {}: {}", idx, variant.display());
				skipped += 1;
				continue;
			}

			let args = self.hls_rendition_args(input, dir, idx, rendition, has_audio);
			self.run_ffmpeg(args, None).await?;
			encoded += 1;
		}

		// Always (re)write the master so it reflects the full ladder, including
		// renditions that were skipped because they already existed.
		let master_path = dir.join(self.config.manifest_name());
		let master = self.build_hls_master(source_dims, has_audio);
		tokio::fs::write(&master_path, master).await?;

		let (segment_count, total_size_bytes) = summarize_package(dir)?;
		info!(
			"HLS package ready: {} renditions ({} encoded, {} skipped), {} segments at {}",
			self.config.ladder.len(),
			encoded,
			skipped,
			segment_count,
			master_path.display()
		);

		Ok(StreamInfo {
			manifest_path: master_path,
			rendition_count: self.config.ladder.len(),
			segment_count,
			renditions_encoded: encoded,
			renditions_skipped: skipped,
			total_size_bytes,
		})
	}

	/// Build the ffmpeg args for one HLS rendition (variant playlist + segments).
	fn hls_rendition_args(
		&self,
		input: &Path,
		dir: &Path,
		idx: usize,
		rendition: &Rendition,
		has_audio: bool,
	) -> Vec<OsString> {
		let stem = Self::variant_stem(idx);
		let ext = self.config.segment_type.extension();
		let mut args: Vec<OsString> = Vec::new();

		args.push("-y".into());
		args.push("-i".into());
		args.push(input.as_os_str().to_owned());

		args.push("-map".into());
		args.push("0:v:0".into());
		if has_audio {
			args.push("-map".into());
			args.push("0:a:0".into());
		}

		args.push("-vf".into());
		args.push(format!("scale=-2:{}", rendition.height).into());

		self.push_video_codec_args(&mut args, None, rendition);

		if has_audio {
			args.push("-c:a".into());
			args.push("aac".into());
			args.push("-b:a".into());
			args.push(format!("{}k", AUDIO_BITRATE_KBPS).into());
		}

		args.push("-f".into());
		args.push("hls".into());
		args.push("-hls_time".into());
		args.push(self.config.segment_duration.to_string().into());
		args.push("-hls_playlist_type".into());
		args.push("vod".into());
		args.push("-hls_flags".into());
		args.push("independent_segments".into());

		if self.config.segment_type == SegmentType::Fmp4 {
			args.push("-hls_segment_type".into());
			args.push("fmp4".into());
			// Each rendition needs its own init file to avoid collisions in the
			// shared package directory.
			args.push("-hls_fmp4_init_filename".into());
			args.push(format!("{stem}_init.mp4").into());
		}

		args.push("-hls_segment_filename".into());
		args.push(
			dir.join(format!("{stem}_seg_%03d.{ext}"))
				.as_os_str()
				.to_owned(),
		);
		args.push(dir.join(format!("{stem}.m3u8")).as_os_str().to_owned());

		args
	}

	/// Hand-assemble the HLS master playlist referencing every variant.
	fn build_hls_master(&self, source_dims: Option<(u32, u32)>, has_audio: bool) -> String {
		let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
		let audio_kbps = if has_audio { AUDIO_BITRATE_KBPS } else { 0 };

		for (idx, rendition) in self.config.ladder.iter().enumerate() {
			let bandwidth = (rendition.bitrate_kbps + audio_kbps) * 1000;
			out.push_str(&format!("#EXT-X-STREAM-INF:BANDWIDTH={bandwidth}"));

			if let Some((sw, sh)) = source_dims {
				if sh > 0 {
					// Width preserves the source aspect ratio, evened to match the
					// scale filter's `-2`.
					let mut w = (sw as u64 * rendition.height as u64 / sh as u64) as u32;
					w &= !1;
					out.push_str(&format!(",RESOLUTION={}x{}", w.max(2), rendition.height));
				}
			}

			out.push('\n');
			out.push_str(&format!("{}.m3u8\n", Self::variant_stem(idx)));
		}

		out
	}

	/// Package a DASH `.mpd` with every rendition in a single ffmpeg call.
	async fn generate_dash(&self, input: &Path, dir: &Path) -> StreamResult<StreamInfo> {
		let manifest_path = dir.join(self.config.manifest_name());

		// DASH cannot be appended to incrementally, so the package is the
		// resumable unit: skip only when the whole manifest already exists.
		if manifest_path.exists() && !self.regenerate {
			debug!(
				"Skipping existing DASH package: {}",
				manifest_path.display()
			);
			let (segment_count, total_size_bytes) = summarize_package(dir)?;
			return Ok(StreamInfo {
				manifest_path,
				rendition_count: self.config.ladder.len(),
				segment_count,
				renditions_encoded: 0,
				renditions_skipped: self.config.ladder.len(),
				total_size_bytes,
			});
		}

		let has_audio = probe_has_audio(input);
		// ffmpeg's DASH muxer writes segment files relative to the process working
		// directory while only the manifest honours the output path, so run with
		// the package dir as cwd and a relative manifest name to keep segments and
		// the `.mpd` co-located with relative media URLs. The input is made
		// absolute first so the cwd change cannot break the source lookup.
		let input_abs = std::path::absolute(input).unwrap_or_else(|_| input.to_path_buf());
		let manifest_name = PathBuf::from(self.config.manifest_name());
		let args = self.dash_args(&input_abs, &manifest_name, has_audio);
		self.run_ffmpeg(args, Some(dir)).await?;

		let (segment_count, total_size_bytes) = summarize_package(dir)?;
		info!(
			"DASH package ready: {} renditions, {} segments at {}",
			self.config.ladder.len(),
			segment_count,
			manifest_path.display()
		);

		Ok(StreamInfo {
			manifest_path,
			rendition_count: self.config.ladder.len(),
			segment_count,
			renditions_encoded: self.config.ladder.len(),
			renditions_skipped: 0,
			total_size_bytes,
		})
	}

	/// Build the ffmpeg args for a full multi-rendition DASH package.
	fn dash_args(&self, input: &Path, manifest: &Path, has_audio: bool) -> Vec<OsString> {
		let mut args: Vec<OsString> = Vec::new();
		let n = self.config.ladder.len();

		args.push("-y".into());
		args.push("-i".into());
		args.push(input.as_os_str().to_owned());

		if n > 1 {
			// Split the decoded video into N branches and scale each to a rung of
			// the ladder, labelling the outputs [v0]..[vN-1].
			let mut fc = format!("[0:v]split={n}");
			for i in 0..n {
				fc.push_str(&format!("[t{i}]"));
			}
			for (i, rendition) in self.config.ladder.iter().enumerate() {
				fc.push_str(&format!(";[t{i}]scale=-2:{}[v{i}]", rendition.height));
			}
			args.push("-filter_complex".into());
			args.push(fc.into());
			for i in 0..n {
				args.push("-map".into());
				args.push(format!("[v{i}]").into());
			}
		} else {
			args.push("-map".into());
			args.push("0:v:0".into());
			args.push("-vf".into());
			args.push(format!("scale=-2:{}", self.config.ladder[0].height).into());
		}

		for (i, rendition) in self.config.ladder.iter().enumerate() {
			self.push_video_codec_args(&mut args, Some(i), rendition);
		}

		if has_audio {
			args.push("-map".into());
			args.push("0:a:0".into());
			args.push("-c:a".into());
			args.push("aac".into());
			args.push("-b:a".into());
			args.push(format!("{}k", AUDIO_BITRATE_KBPS).into());
		}

		args.push("-use_template".into());
		args.push("1".into());
		args.push("-use_timeline".into());
		args.push("1".into());
		args.push("-seg_duration".into());
		args.push(self.config.segment_duration.to_string().into());
		args.push("-adaptation_sets".into());
		args.push(if has_audio {
			"id=0,streams=v id=1,streams=a".into()
		} else {
			"id=0,streams=v".into()
		});
		args.push("-f".into());
		args.push("dash".into());
		args.push(manifest.as_os_str().to_owned());

		args
	}

	/// Append the libx264 codec + VBV rate-control args for one rendition.
	///
	/// `stream_index` selects a per-output stream specifier (`-c:v:0`) for the
	/// multi-stream DASH call; `None` emits the unqualified `-c:v` used by the
	/// single-stream HLS rendition calls.
	fn push_video_codec_args(
		&self,
		args: &mut Vec<OsString>,
		stream_index: Option<usize>,
		rendition: &Rendition,
	) {
		let suffix = stream_index.map(|i| format!(":{i}")).unwrap_or_default();

		args.push(format!("-c:v{suffix}").into());
		args.push("libx264".into());
		args.push(format!("-preset{suffix}").into());
		args.push(self.config.preset.clone().into());
		args.push(format!("-b:v{suffix}").into());
		args.push(format!("{}k", rendition.bitrate_kbps).into());
		args.push(format!("-maxrate{suffix}").into());
		args.push(format!("{}k", rendition.maxrate_kbps()).into());
		args.push(format!("-bufsize{suffix}").into());
		args.push(format!("{}k", rendition.bufsize_kbps()).into());
		args.push(format!("-pix_fmt{suffix}").into());
		args.push("yuv420p".into());
	}

	/// Spawn ffmpeg with `args`, drain stderr, and await completion.
	///
	/// `cwd` sets the child's working directory. The DASH muxer resolves segment
	/// filenames relative to it, so DASH packaging passes the package directory;
	/// HLS uses absolute segment paths and passes `None`.
	async fn run_ffmpeg(&self, args: Vec<OsString>, cwd: Option<&Path>) -> StreamResult<()> {
		let mut cmd = crate::ops::media::ffmpeg_bin::tokio_command();
		cmd.args(args)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::piped());

		if let Some(dir) = cwd {
			cmd.current_dir(dir);
		}

		debug!("Executing ffmpeg command: {:?}", cmd);

		let mut child = cmd.spawn().map_err(|_| StreamError::FFmpegNotFound)?;

		if let Some(stderr) = child.stderr.take() {
			let reader = BufReader::new(stderr);
			let mut lines = reader.lines();
			while let Ok(Some(_line)) = lines.next_line().await {
				// Drained so the child never blocks on a full stderr pipe.
			}
		}

		let status = child
			.wait()
			.await
			.map_err(|e| StreamError::Other(format!("Failed to wait for ffmpeg: {}", e)))?;

		if !status.success() {
			return Err(StreamError::FFmpegProcessFailed(
				status.code().unwrap_or(-1),
			));
		}

		Ok(())
	}
}

/// Count media segments and total package size by scanning `dir` (flat layout).
fn summarize_package(dir: &Path) -> StreamResult<(usize, u64)> {
	let mut segments = 0usize;
	let mut total = 0u64;

	for entry in std::fs::read_dir(dir)? {
		let entry = entry?;
		let path = entry.path();
		if !path.is_file() {
			continue;
		}
		total += entry.metadata().map(|m| m.len()).unwrap_or(0);
		if matches!(
			path.extension().and_then(|e| e.to_str()),
			Some("ts") | Some("m4s")
		) {
			segments += 1;
		}
	}

	Ok((segments, total))
}

/// True when the input has at least one audio stream (ffprobe).
fn probe_has_audio(input: &Path) -> bool {
	let output = std::process::Command::new("ffprobe")
		.args([
			"-v",
			"error",
			"-select_streams",
			"a",
			"-show_entries",
			"stream=index",
			"-of",
			"csv=p=0",
		])
		.arg(input)
		.output();

	match output {
		Ok(o) => o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
		Err(_) => false,
	}
}

/// Probe the source `(width, height)` of the first video stream (ffprobe).
fn probe_dimensions(input: &Path) -> Option<(u32, u32)> {
	let output = std::process::Command::new("ffprobe")
		.args([
			"-v",
			"error",
			"-select_streams",
			"v:0",
			"-show_entries",
			"stream=width,height",
			"-of",
			"csv=p=0",
		])
		.arg(input)
		.output()
		.ok()?;

	if !output.status.success() {
		return None;
	}

	let text = String::from_utf8_lossy(&output.stdout);
	let line = text.trim();
	let mut parts = line.split(',');
	let w = parts.next()?.trim().parse::<u32>().ok()?;
	let h = parts.next()?.trim().parse::<u32>().ok()?;
	Some((w, h))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ops::media::stream::config::Rendition;

	fn arg_strings(args: &[OsString]) -> Vec<String> {
		args.iter()
			.map(|a| a.to_string_lossy().into_owned())
			.collect()
	}

	fn ladder() -> Vec<Rendition> {
		vec![
			Rendition::new("720p", 720, 2800),
			Rendition::new("480p", 480, 1400),
		]
	}

	#[test]
	fn hls_rendition_args_use_hls_muxer_and_segment_filename() {
		let gen =
			StreamGenerator::new(StreamConfig::new(StreamProtocol::Hls).with_ladder(ladder()));
		let args = arg_strings(&gen.hls_rendition_args(
			Path::new("in.mp4"),
			Path::new("pkg"),
			0,
			&ladder()[0],
			false,
		));

		assert!(args.windows(2).any(|w| w[0] == "-f" && w[1] == "hls"));
		assert!(args
			.windows(2)
			.any(|w| w[0] == "-hls_playlist_type" && w[1] == "vod"));
		assert!(args.iter().any(|a| a.contains("stream_0_seg_%03d.ts")));
		assert!(args.iter().any(|a| a.ends_with("stream_0.m3u8")));
		assert!(args.windows(2).any(|w| w[0] == "-c:v" && w[1] == "libx264"));
		assert!(args.windows(2).any(|w| w[0] == "-b:v" && w[1] == "2800k"));
	}

	#[test]
	fn hls_args_add_audio_mapping_only_when_present() {
		let gen =
			StreamGenerator::new(StreamConfig::new(StreamProtocol::Hls).with_ladder(ladder()));
		let no_audio = arg_strings(&gen.hls_rendition_args(
			Path::new("in.mp4"),
			Path::new("pkg"),
			0,
			&ladder()[0],
			false,
		));
		assert!(!no_audio
			.windows(2)
			.any(|w| w[0] == "-map" && w[1] == "0:a:0"));

		let with_audio = arg_strings(&gen.hls_rendition_args(
			Path::new("in.mp4"),
			Path::new("pkg"),
			0,
			&ladder()[0],
			true,
		));
		assert!(with_audio
			.windows(2)
			.any(|w| w[0] == "-map" && w[1] == "0:a:0"));
		assert!(with_audio
			.windows(2)
			.any(|w| w[0] == "-c:a" && w[1] == "aac"));
	}

	#[test]
	fn dash_args_use_dash_muxer_and_adaptation_sets() {
		let gen =
			StreamGenerator::new(StreamConfig::new(StreamProtocol::Dash).with_ladder(ladder()));
		let args =
			arg_strings(&gen.dash_args(Path::new("in.mp4"), Path::new("pkg/manifest.mpd"), false));

		assert!(args.windows(2).any(|w| w[0] == "-f" && w[1] == "dash"));
		assert!(args
			.windows(2)
			.any(|w| w[0] == "-adaptation_sets" && w[1] == "id=0,streams=v"));
		assert!(args.iter().any(|a| a.starts_with("[0:v]split=2")));
		assert!(args
			.windows(2)
			.any(|w| w[0] == "-c:v:0" && w[1] == "libx264"));
		assert!(args
			.windows(2)
			.any(|w| w[0] == "-c:v:1" && w[1] == "libx264"));
	}

	#[test]
	fn dash_audio_adds_second_adaptation_set() {
		let gen =
			StreamGenerator::new(StreamConfig::new(StreamProtocol::Dash).with_ladder(ladder()));
		let args =
			arg_strings(&gen.dash_args(Path::new("in.mp4"), Path::new("pkg/manifest.mpd"), true));
		assert!(args
			.windows(2)
			.any(|w| w[0] == "-adaptation_sets" && w[1] == "id=0,streams=v id=1,streams=a"));
	}

	#[test]
	fn master_playlist_lists_every_variant() {
		let gen =
			StreamGenerator::new(StreamConfig::new(StreamProtocol::Hls).with_ladder(ladder()));
		let master = gen.build_hls_master(Some((1920, 1080)), false);
		assert!(master.starts_with("#EXTM3U"));
		assert!(master.contains("stream_0.m3u8"));
		assert!(master.contains("stream_1.m3u8"));
		assert!(master.contains("BANDWIDTH=2800000"));
		assert!(master.contains("RESOLUTION="));
	}

	#[test]
	fn master_bandwidth_includes_audio_when_present() {
		let gen =
			StreamGenerator::new(StreamConfig::new(StreamProtocol::Hls).with_ladder(ladder()));
		let master = gen.build_hls_master(None, true);
		// 2800 + 128 = 2928 kbps -> 2_928_000 bps.
		assert!(master.contains("BANDWIDTH=2928000"));
		assert!(!master.contains("RESOLUTION="));
	}
}
