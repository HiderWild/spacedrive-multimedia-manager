#[cfg(target_os = "macos")]
use std::process::Command;

fn is_release_tauri_bundle() -> bool {
	is_release_tauri_bundle_with(
		std::env::var_os("TAURI_CONFIG").is_some(),
		std::env::var("TAURI_ENV_DEBUG").ok().as_deref(),
	)
}

fn is_release_tauri_bundle_with(has_tauri_config: bool, tauri_env_debug: Option<&str>) -> bool {
	has_tauri_config && matches!(tauri_env_debug, Some("false") | Some("0"))
}

fn main() {
	// Compile .icon to Assets.car on macOS
	#[cfg(target_os = "macos")]
	{
		let project_root = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
		let icon_source = format!("{}/../Spacedrive.icon", project_root);
		let gen_dir = format!("{}/gen", project_root);

		// Create gen directory
		std::fs::create_dir_all(&gen_dir).expect("Failed to create gen directory");

		// Check if .icon file exists
		if std::path::Path::new(&icon_source).exists() {
			println!("cargo:rerun-if-changed={}", icon_source);

			// Run actool to compile .icon to Assets.car
			let output = Command::new("xcrun")
				.args([
					"actool",
					&icon_source,
					"--compile",
					&gen_dir,
					"--output-format",
					"human-readable-text",
					"--notices",
					"--warnings",
					"--errors",
					"--output-partial-info-plist",
					&format!("{}/partial.plist", gen_dir),
					"--app-icon",
					"Spacedrive",
					"--include-all-app-icons",
					"--enable-on-demand-resources",
					"NO",
					"--development-region",
					"en",
					"--target-device",
					"mac",
					"--minimum-deployment-target",
					"11.0",
					"--platform",
					"macosx",
				])
				.output()
				.expect("Failed to execute actool");

			if !output.status.success() {
				eprintln!("actool failed: {}", String::from_utf8_lossy(&output.stderr));
			} else {
				println!("Successfully compiled Spacedrive.icon to Assets.car");
			}
		} else {
			println!("cargo:warning=Spacedrive.icon not found at {}", icon_source);
		}
	}

	// Create target-suffixed daemon binary for Tauri bundler
	// Tauri's externalBin expects binaries with target triple suffix
	let target_triple = std::env::var("TARGET").expect("TARGET not set");

	// Expose target triple to runtime code for daemon binary resolution
	println!("cargo:rustc-env=SD_TARGET_TRIPLE={}", target_triple);
	let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
	let workspace_dir = std::env::var("CARGO_WORKSPACE_DIR")
		.or_else(|_| std::env::var("CARGO_MANIFEST_DIR").map(|d| format!("{}/../../..", d)))
		.expect("Could not find workspace directory");

	let exe_ext = if target_triple.contains("windows") {
		".exe"
	} else {
		""
	};

	for source_profile in [profile.as_str(), "release"] {
		let daemon_source = format!(
			"{}/target/{}/sd-daemon{}",
			workspace_dir, source_profile, exe_ext
		);
		let daemon_target = format!(
			"{}/target/{}/sd-daemon-{}{}",
			workspace_dir, source_profile, target_triple, exe_ext
		);

		if std::path::Path::new(&daemon_source).exists() {
			let _ = std::fs::remove_file(&daemon_target);

			if let Err(e) = std::fs::copy(&daemon_source, &daemon_target) {
				eprintln!("Warning: Failed to copy daemon: {}", e);
			}
		}
	}

	let daemon_available = [profile.as_str(), "release"].iter().any(|source_profile| {
		let daemon_source = format!(
			"{}/target/{}/sd-daemon{}",
			workspace_dir, source_profile, exe_ext
		);
		std::path::Path::new(&daemon_source).exists()
	});
	let release_bundle = is_release_tauri_bundle();

	if release_bundle && !daemon_available {
		panic!(
			"Cannot create a release bundle without the daemon at target/release/sd-daemon{}",
			exe_ext
		);
	}

	// Windows needs the generated common-controls manifest even for debug runs;
	// without it, comctl32 resolves the v6-only TaskDialogIndirect import against
	// the legacy v5 API set and the executable fails before `main` starts.
	if release_bundle || cfg!(target_os = "windows") {
		tauri_build::build();
	} else {
		println!("cargo:warning=Skipping Tauri resource processing outside a release bundle");
	}
}

#[cfg(test)]
mod tests {
	use super::is_release_tauri_bundle_with;

	#[test]
	fn only_release_tauri_invocations_require_the_daemon() {
		assert!(is_release_tauri_bundle_with(true, Some("false")));
		assert!(is_release_tauri_bundle_with(true, Some("0")));
		assert!(!is_release_tauri_bundle_with(true, Some("true")));
		assert!(!is_release_tauri_bundle_with(true, None));
		assert!(!is_release_tauri_bundle_with(false, Some("false")));
	}
}
