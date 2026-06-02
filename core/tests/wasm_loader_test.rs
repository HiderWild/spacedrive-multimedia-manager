//! Integration tests for the standalone WASM extension loader (F-01).
//!
//! These tests load the compiled `test-extension` WASM module, run its
//! `counter` job through the host-backed runner, and verify completion,
//! checkpoint/resume safety, and the capability gate.
//!
//! The test extension is built separately with:
//!   cargo build --target wasm32-unknown-unknown --release
//! from `extensions/test-extension/`. If the artifact is missing, the tests
//! skip with a clear message rather than failing.

#![cfg(feature = "wasm")]

use std::path::PathBuf;

use sd_core::infra::extension::{ExtensionManifest, WasmRunner};

/// Locate the compiled test-extension wasm, preferring the fresh build output
/// and falling back to the committed artifact next to the manifest.
fn locate_wasm() -> Option<(ExtensionManifest, Vec<u8>)> {
	let ext_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../extensions/test-extension");

	let manifest_str = std::fs::read_to_string(ext_dir.join("manifest.json")).ok()?;
	let manifest: ExtensionManifest = serde_json::from_str(&manifest_str).ok()?;

	let build_output = ext_dir.join("target/wasm32-unknown-unknown/release/test_extension.wasm");
	let committed = ext_dir.join(&manifest.wasm_file);

	let wasm_path = if build_output.exists() {
		build_output
	} else if committed.exists() {
		committed
	} else {
		return None;
	};

	let bytes = std::fs::read(&wasm_path).ok()?;
	Some((manifest, bytes))
}

fn ctx_json() -> String {
	serde_json::json!({
		"job_id": "11111111-1111-1111-1111-111111111111",
		"library_id": "22222222-2222-2222-2222-222222222222",
	})
	.to_string()
}

#[derive(serde::Deserialize)]
struct CounterState {
	current: u32,
	target: u32,
	processed: Vec<String>,
}

#[test]
fn counter_job_runs_to_completion() {
	let Some((manifest, wasm)) = locate_wasm() else {
		eprintln!(
			"SKIP: test_extension.wasm not found. Build it with \
			 `cargo build --target wasm32-unknown-unknown --release` in \
			 extensions/test-extension/"
		);
		return;
	};

	let mut runner = WasmRunner::from_bytes(manifest, &wasm).expect("instantiate wasm");
	runner.init().expect("plugin_init");

	// The extension registered its counter job during init.
	let jobs = runner.registered_jobs();
	assert!(
		jobs.iter().any(|j| j.name == "counter"),
		"expected a registered 'counter' job, got: {jobs:?}"
	);
	let job = jobs.iter().find(|j| j.name == "counter").unwrap();
	assert_eq!(job.export_fn, "execute_test_counter");
	assert!(job.resumable);

	let target = 25u32;
	let initial =
		serde_json::json!({ "current": 0, "target": target, "processed": [] }).to_string();

	let run = runner
		.run_job(&job.export_fn, &ctx_json(), Some(&initial))
		.expect("run counter job");

	assert!(
		run.completed(),
		"job did not complete: exit={}",
		run.exit_code
	);
	assert_eq!(
		run.items, target as u64,
		"expected {target} items processed"
	);
	assert!(
		(run.last_progress - 1.0).abs() < f32::EPSILON,
		"final progress should be 1.0, got {}",
		run.last_progress
	);

	let final_state: CounterState =
		serde_json::from_slice(&run.checkpoint.expect("final checkpoint")).unwrap();
	assert_eq!(final_state.current, target, "counter did not reach target");
	assert_eq!(final_state.target, target);
	assert_eq!(final_state.processed.len(), target as usize);

	println!(
		"counter completed: current={}/{} items={} progress={}",
		final_state.current, final_state.target, run.items, run.last_progress
	);
}

#[test]
fn counter_job_interrupts_and_resumes() {
	let Some((manifest, wasm)) = locate_wasm() else {
		eprintln!("SKIP: test_extension.wasm not found (see other test).");
		return;
	};

	let mut runner = WasmRunner::from_bytes(manifest, &wasm).expect("instantiate wasm");
	runner.init().expect("plugin_init");
	let export = "execute_test_counter";
	let target = 25u32;

	// Arm interruption after 5 interrupt checks; the job checkpoints and bails.
	runner.arm_interrupt(5);
	let initial =
		serde_json::json!({ "current": 0, "target": target, "processed": [] }).to_string();
	let first = runner
		.run_job(export, &ctx_json(), Some(&initial))
		.expect("run interrupted job");

	assert!(
		first.interrupted(),
		"expected interruption, exit={}",
		first.exit_code
	);
	let saved = first.checkpoint.clone().expect("interrupt checkpoint");
	let mid: CounterState = serde_json::from_slice(&saved).unwrap();
	assert_eq!(mid.current, 5, "checkpoint should capture mid-run counter");
	assert!(mid.current < target);

	// Resume from the saved checkpoint with interruption cleared.
	runner.disarm_interrupt();
	let resume_state = String::from_utf8(saved).unwrap();
	let second = runner
		.run_job(export, &ctx_json(), Some(&resume_state))
		.expect("resume job");

	assert!(
		second.completed(),
		"resumed job did not complete: exit={}",
		second.exit_code
	);
	let final_state: CounterState =
		serde_json::from_slice(&second.checkpoint.expect("final checkpoint")).unwrap();
	assert_eq!(
		final_state.current, target,
		"resume did not finish counting"
	);
	// Only the remaining items were processed in the resumed run.
	assert_eq!(second.items, (target - mid.current) as u64);

	println!(
		"interrupt@{} -> resumed -> completed at {}/{}",
		mid.current, final_state.current, final_state.target
	);
}

#[test]
fn capability_gate_enforced() {
	let Some((manifest, wasm)) = locate_wasm() else {
		eprintln!("SKIP: test_extension.wasm not found (see other test).");
		return;
	};

	let runner = WasmRunner::from_bytes(manifest, &wasm).expect("instantiate wasm");

	// The manifest grants "query:" and "action:" prefixes.
	assert!(runner.check_capability("query:files.list").is_ok());
	assert!(runner.check_capability("action:files.copy").is_ok());

	// Anything outside the declared capabilities is refused.
	assert!(runner.check_capability("credentials.delete").is_err());
	assert!(runner.check_capability("network:fetch").is_err());

	println!("capability gate enforced for granted and denied methods");
}
