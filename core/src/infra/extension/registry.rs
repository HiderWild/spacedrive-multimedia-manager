//! # Runtime Extension Job Registry
//!
//! `core::infra::extension::registry` makes WASM-defined jobs discoverable and
//! invocable the same way native operations are, without faking compile-time
//! registration.
//!
//! ## Why a separate runtime registry
//!
//! Native operations register themselves through the `inventory` crate at
//! compile time: `register_library_action!`/`register_query!` push handlers into
//! the static `ACTIONS`/`QUERIES` maps that the daemon resolves by wire string
//! (`action:files.copy`, `query:network.status`). That mechanism cannot describe
//! a `.wasm` that is discovered and loaded at runtime, so extension jobs live in
//! this registry instead. It mirrors the native surface (list + lookup + invoke
//! by id) but is populated dynamically as extensions load.
//!
//! ## Id / namespacing scheme
//!
//! Each extension job gets a wire-style id `extension:<ext_id>.<job_name>`,
//! matching the native `<kind>:<path>` convention. The `extension:` prefix is a
//! third namespace alongside `action:` and `query:`; a dispatch layer can
//! recognize it and consult this registry after the static lookup misses (see
//! [`ExtensionRegistry::resolve`]).
//!
//! ## Execution
//!
//! Invocation reuses the F-01 [`WasmRunner`]. The registry owns one runner per
//! extension (a `wasmer::Store` is not `Send`, so the registry is intended for
//! headless/in-process use rather than being shared into the async runtime).
//! Checkpoint, progress, and deterministic interruption all delegate to the
//! runner, and a WASM trap surfaces as [`RegistryError::Runner`] rather than
//! unwinding into core.

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;

use super::runner::{JobRun, RunnerError, WasmRunner};

/// Wire-string prefix for extension jobs, parallel to `action:`/`query:`.
pub const EXTENSION_PREFIX: &str = "extension:";

/// Build the wire-style id for an extension job: `extension:<ext_id>.<job>`.
pub fn job_wire_id(extension_id: &str, job_name: &str) -> String {
	format!("{EXTENSION_PREFIX}{extension_id}.{job_name}")
}

#[derive(Error, Debug)]
pub enum RegistryError {
	#[error("extension job '{0}' is not registered")]
	JobNotFound(String),

	#[error("extension '{0}' is not registered")]
	ExtensionNotFound(String),

	#[error("extension '{0}' is already registered")]
	ExtensionAlreadyRegistered(String),

	#[error("extension runner error: {0}")]
	Runner(#[from] RunnerError),
}

/// Discoverable metadata for one registered extension job.
///
/// This is the runtime analogue of a native operation descriptor: it carries
/// the wire id used for lookup plus the information needed to dispatch the call
/// to the owning extension's runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionJobInfo {
	/// Wire id, e.g. `extension:test-extension.counter`.
	pub id: String,
	/// Owning extension id.
	pub extension_id: String,
	/// Logical job name the extension registered (e.g. `counter`).
	pub job_name: String,
	/// WASM export the runner calls (e.g. `execute_test_counter`).
	pub export_fn: String,
	/// Whether the job can resume from a checkpoint.
	pub resumable: bool,
}

/// Owns loaded extensions and surfaces their jobs as wire-addressable ids.
///
/// The registry is the runtime counterpart to the static operation registry:
/// `register_runner` enumerates an extension's jobs and records them, `list` and
/// `get` discover them, and `run_job` invokes one by id through the F-01 runner.
pub struct ExtensionRegistry {
	/// Loaded runners keyed by extension id. Each runner is `!Send`, so it is
	/// guarded by a `Mutex` and the registry stays single-process.
	runners: Mutex<HashMap<String, WasmRunner>>,
	/// Job metadata keyed by wire id.
	jobs: Mutex<HashMap<String, ExtensionJobInfo>>,
}

impl ExtensionRegistry {
	pub fn new() -> Self {
		Self {
			runners: Mutex::new(HashMap::new()),
			jobs: Mutex::new(HashMap::new()),
		}
	}

	/// Register a freshly loaded (uninitialized) extension runner.
	///
	/// The registry drives `plugin_init()` itself so it owns the lifecycle, then
	/// enumerates the jobs the extension registered and records each under its
	/// `extension:<ext_id>.<job_name>` id. Returns the discovered jobs.
	///
	/// Pass a runner straight from [`WasmRunner::from_bytes`] /
	/// [`WasmRunner::from_manifest_dir`] without calling `init()` first;
	/// initializing twice would double-register the extension's jobs.
	pub fn register_runner(
		&self,
		extension_id: impl Into<String>,
		mut runner: WasmRunner,
	) -> Result<Vec<ExtensionJobInfo>, RegistryError> {
		let extension_id = extension_id.into();

		{
			let runners = self.runners.lock().unwrap();
			if runners.contains_key(&extension_id) {
				return Err(RegistryError::ExtensionAlreadyRegistered(extension_id));
			}
		}

		runner.init()?;

		let infos: Vec<ExtensionJobInfo> = runner
			.registered_jobs()
			.into_iter()
			.map(|job| ExtensionJobInfo {
				id: job_wire_id(&extension_id, &job.name),
				extension_id: extension_id.clone(),
				job_name: job.name,
				export_fn: job.export_fn,
				resumable: job.resumable,
			})
			.collect();

		{
			let mut jobs = self.jobs.lock().unwrap();
			for info in &infos {
				jobs.insert(info.id.clone(), info.clone());
			}
		}
		self.runners.lock().unwrap().insert(extension_id, runner);

		Ok(infos)
	}

	/// All registered extension jobs across every loaded extension.
	pub fn list(&self) -> Vec<ExtensionJobInfo> {
		self.jobs.lock().unwrap().values().cloned().collect()
	}

	/// Look up a job by its wire id.
	pub fn get(&self, job_id: &str) -> Option<ExtensionJobInfo> {
		self.jobs.lock().unwrap().get(job_id).cloned()
	}

	/// Whether a wire id resolves to a registered extension job.
	pub fn has_job(&self, job_id: &str) -> bool {
		self.jobs.lock().unwrap().contains_key(job_id)
	}

	/// Dispatch hook for the wire layer: returns the job descriptor only when the
	/// id carries the `extension:` prefix and is registered. The daemon can call
	/// this after its static `action:`/`query:` lookup misses, keeping native and
	/// extension dispatch on one code path.
	pub fn resolve(&self, wire_id: &str) -> Option<ExtensionJobInfo> {
		if !wire_id.starts_with(EXTENSION_PREFIX) {
			return None;
		}
		self.get(wire_id)
	}

	/// Run a registered extension job by wire id through its owning runner.
	///
	/// `ctx_json` carries the job/library identifiers; `state_json` is the
	/// initial job state (`None` starts from the job's default, or pass a prior
	/// checkpoint's bytes to resume). A trap or panic inside the WASM module is
	/// contained as [`RegistryError::Runner`].
	pub fn run_job(
		&self,
		job_id: &str,
		ctx_json: &str,
		state_json: Option<&str>,
	) -> Result<JobRun, RegistryError> {
		let info = self
			.get(job_id)
			.ok_or_else(|| RegistryError::JobNotFound(job_id.to_string()))?;

		let mut runners = self.runners.lock().unwrap();
		let runner = runners
			.get_mut(&info.extension_id)
			.ok_or_else(|| RegistryError::ExtensionNotFound(info.extension_id.clone()))?;

		Ok(runner.run_job(&info.export_fn, ctx_json, state_json)?)
	}

	/// Arm deterministic interruption for the extension that owns `extension_id`:
	/// the next `run_job` stops after `after_checks` interrupt checks. Delegates
	/// to the F-01 runner so a job can checkpoint and bail mid-run.
	pub fn arm_interrupt(
		&self,
		extension_id: &str,
		after_checks: u32,
	) -> Result<(), RegistryError> {
		let runners = self.runners.lock().unwrap();
		let runner = runners
			.get(extension_id)
			.ok_or_else(|| RegistryError::ExtensionNotFound(extension_id.to_string()))?;
		runner.arm_interrupt(after_checks);
		Ok(())
	}

	/// Clear any armed interruption for an extension.
	pub fn disarm_interrupt(&self, extension_id: &str) -> Result<(), RegistryError> {
		let runners = self.runners.lock().unwrap();
		let runner = runners
			.get(extension_id)
			.ok_or_else(|| RegistryError::ExtensionNotFound(extension_id.to_string()))?;
		runner.disarm_interrupt();
		Ok(())
	}

	/// Remove an extension and all of its jobs (e.g. on unload).
	pub fn unregister(&self, extension_id: &str) -> usize {
		self.runners.lock().unwrap().remove(extension_id);
		let mut jobs = self.jobs.lock().unwrap();
		let before = jobs.len();
		jobs.retain(|_, info| info.extension_id != extension_id);
		before - jobs.len()
	}
}

impl Default for ExtensionRegistry {
	fn default() -> Self {
		Self::new()
	}
}
