//! Integration tests for the runtime extension job registry (F-02).
//!
//! These tests load the compiled `test-extension` WASM module, register it with
//! the runtime [`ExtensionRegistry`], assert the registry lists the extension's
//! `counter` job under its `extension:<ext_id>.<job_name>` wire id, and then
//! invoke the job by that id through the registry (which reuses the F-01
//! [`WasmRunner`]) and verifies it runs to completion.
//!
//! The test extension is built separately with:
//!   cargo build --target wasm32-unknown-unknown --release
//! from `extensions/test-extension/`. If the artifact is missing, the tests
//! skip with a clear message rather than failing.

#![cfg(feature = "wasm")]

use std::path::PathBuf;

use sd_core::infra::extension::{
	job_wire_id, ExtensionManifest, ExtensionRegistry, WasmRunner, EXTENSION_PREFIX,
};

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
fn registry_lists_and_invokes_extension_job() {
	let Some((manifest, wasm)) = locate_wasm() else {
		eprintln!(
			"SKIP: test_extension.wasm not found. Build it with \
			 `cargo build --target wasm32-unknown-unknown --release` in \
			 extensions/test-extension/"
		);
		return;
	};

	let extension_id = manifest.id.clone();
	let expected_id = job_wire_id(&extension_id, "counter");

	// Build a fresh (uninitialized) runner; the registry drives plugin_init.
	let runner = WasmRunner::from_bytes(manifest, &wasm).expect("instantiate wasm");

	let registry = ExtensionRegistry::new();
	let registered = registry
		.register_runner(&extension_id, runner)
		.expect("register extension runner");

	// The extension's counter job is discoverable under the wire id.
	assert!(
		registered.iter().any(|j| j.id == expected_id),
		"register_runner should return the counter job, got: {registered:?}"
	);

	let listed = registry.list();
	let counter = listed
		.iter()
		.find(|j| j.id == expected_id)
		.expect("registry should list the counter job by its wire id");
	assert!(
		counter.id.starts_with(EXTENSION_PREFIX),
		"job id should carry the extension: wire prefix"
	);
	assert_eq!(counter.extension_id, extension_id);
	assert_eq!(counter.job_name, "counter");
	assert_eq!(counter.export_fn, "execute_test_counter");
	assert!(counter.resumable);

	// Lookup by id (the path a dispatcher would use after a native miss).
	assert!(registry.has_job(&expected_id));
	assert!(registry.resolve(&expected_id).is_some());
	assert!(
		registry.resolve("action:files.copy").is_none(),
		"resolve must ignore non-extension wire ids"
	);

	// Invoke the job by its wire id through the registry.
	let target = 25u32;
	let initial =
		serde_json::json!({ "current": 0, "target": target, "processed": [] }).to_string();
	let run = registry
		.run_job(&expected_id, &ctx_json(), Some(&initial))
		.expect("run extension job by id");

	assert!(
		run.completed(),
		"extension job did not complete: exit={}",
		run.exit_code
	);
	assert_eq!(
		run.items, target as u64,
		"expected {target} items processed"
	);

	let final_state: CounterState =
		serde_json::from_slice(&run.checkpoint.expect("final checkpoint")).unwrap();
	assert_eq!(final_state.current, target, "counter did not reach target");
	assert_eq!(final_state.target, target);
	assert_eq!(final_state.processed.len(), target as usize);

	println!(
		"registry invoked {expected_id}: current={}/{} items={} progress={}",
		final_state.current, final_state.target, run.items, run.last_progress
	);
}

#[test]
fn registry_honors_interrupt_through_runner() {
	let Some((manifest, wasm)) = locate_wasm() else {
		eprintln!("SKIP: test_extension.wasm not found (see other test).");
		return;
	};

	let extension_id = manifest.id.clone();
	let job_id = job_wire_id(&extension_id, "counter");
	let runner = WasmRunner::from_bytes(manifest, &wasm).expect("instantiate wasm");

	let registry = ExtensionRegistry::new();
	registry
		.register_runner(&extension_id, runner)
		.expect("register extension runner");

	let target = 25u32;
	let initial =
		serde_json::json!({ "current": 0, "target": target, "processed": [] }).to_string();

	// Arm interruption after 5 checks; the job checkpoints and bails mid-run.
	registry
		.arm_interrupt(&extension_id, 5)
		.expect("arm interrupt");
	let first = registry
		.run_job(&job_id, &ctx_json(), Some(&initial))
		.expect("run interrupted job");
	assert!(
		first.interrupted(),
		"expected interruption, exit={}",
		first.exit_code
	);
	let saved = first.checkpoint.clone().expect("interrupt checkpoint");
	let mid: CounterState = serde_json::from_slice(&saved).unwrap();
	assert_eq!(mid.current, 5, "checkpoint should capture mid-run counter");

	// Resume from the saved checkpoint with interruption cleared.
	registry
		.disarm_interrupt(&extension_id)
		.expect("disarm interrupt");
	let resume_state = String::from_utf8(saved).unwrap();
	let second = registry
		.run_job(&job_id, &ctx_json(), Some(&resume_state))
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

	println!(
		"registry interrupt@{} -> resumed -> completed at {}/{}",
		mid.current, final_state.current, final_state.target
	);
}

#[test]
fn registry_rejects_unknown_job_id() {
	let registry = ExtensionRegistry::new();
	let err = registry
		.run_job("extension:missing.nope", "{}", None)
		.expect_err("running an unregistered job must fail, not panic");
	let msg = err.to_string();
	assert!(msg.contains("not registered"), "unexpected error: {msg}");
	assert!(registry.list().is_empty());
	println!("registry rejected unknown job id with: {msg}");
}
