//! Resolve which `ffmpeg` binary to invoke.
//!
//! Operators can install an NVENC-capable build without replacing the distro
//! binary by setting `FFMPEG_PATH` (or, as a fallback, `FFMPEG`). Detection and
//! all encode/proxy jobs should go through this helper so HW acceleration works
//! end-to-end.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Environment variables consulted, in order.
const ENV_KEYS: &[&str] = &["FFMPEG_PATH", "FFMPEG"];

/// Resolved ffmpeg program name or absolute path.
///
/// Cached after the first call so log noise stays quiet and env is stable for
/// the process lifetime.
pub fn ffmpeg_program() -> &'static str {
	static PROGRAM: OnceLock<String> = OnceLock::new();
	PROGRAM.get_or_init(resolve_ffmpeg_program).as_str()
}

/// Resolve without caching (used by unit tests).
fn resolve_ffmpeg_program() -> String {
	for key in ENV_KEYS {
		if let Ok(raw) = std::env::var(key) {
			let trimmed = raw.trim();
			if trimmed.is_empty() {
				continue;
			}
			let path = PathBuf::from(trimmed);
			if path_looks_usable(&path) {
				tracing::info!(
					env = key,
					path = %path.display(),
					"Using ffmpeg from environment override"
				);
				return path.to_string_lossy().into_owned();
			}
			tracing::warn!(
				env = key,
				path = %path.display(),
				"FFMPEG override set but path is not usable; falling back to PATH"
			);
		}
	}
	"ffmpeg".to_string()
}

fn path_looks_usable(path: &Path) -> bool {
	if path.as_os_str().is_empty() {
		return false;
	}
	// Absolute / relative paths: require an existing file.
	if path.is_absolute() || path.components().count() > 1 {
		return path.is_file();
	}
	// Bare program names like "ffmpeg-nvenc" are accepted; existence is checked
	// when the process is spawned via PATH lookup.
	true
}

/// Build a std `Command` pointed at the resolved ffmpeg binary.
pub fn command() -> std::process::Command {
	std::process::Command::new(ffmpeg_program())
}

/// Build a tokio `Command` pointed at the resolved ffmpeg binary.
pub fn tokio_command() -> tokio::process::Command {
	tokio::process::Command::new(ffmpeg_program())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Mutex;

	// Serialize env-mutating tests.
	static ENV_LOCK: Mutex<()> = Mutex::new(());

	#[test]
	fn bare_program_name_is_usable() {
		assert!(path_looks_usable(Path::new("ffmpeg")));
		assert!(path_looks_usable(Path::new("ffmpeg-nvenc")));
	}

	#[test]
	fn empty_path_is_not_usable() {
		assert!(!path_looks_usable(Path::new("")));
	}

	#[test]
	fn missing_absolute_path_is_not_usable() {
		assert!(!path_looks_usable(Path::new(
			"/definitely/does/not/exist/ffmpeg-xyz"
		)));
	}

	#[test]
	fn resolve_falls_back_to_ffmpeg_without_env() {
		let _g = ENV_LOCK.lock().unwrap();
		// Clear overrides for this test process slice.
		std::env::remove_var("FFMPEG_PATH");
		std::env::remove_var("FFMPEG");
		// Cannot assert on cached ffmpeg_program() here (OnceLock); use private resolver.
		assert_eq!(resolve_ffmpeg_program(), "ffmpeg");
	}
}
