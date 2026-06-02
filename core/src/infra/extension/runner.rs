//! Standalone WASM extension runner.
//!
//! Loads a compiled extension `.wasm`, instantiates it with the `spacedrive`
//! host import module, and drives a registered job export to completion with
//! real checkpoint, progress, interrupt, and capability enforcement.
//!
//! Unlike [`super::manager::PluginManager`], this runner has no dependency on a
//! live [`crate::Core`]: the host functions are backed by an in-memory
//! execution context instead of the Wire operation registry. That keeps the
//! loader self-contained so extensions can be loaded and executed in isolation
//! (used by integration tests and headless tooling) without standing up a
//! database or library. Execution is fully sandboxed: a WASM trap or panic
//! surfaces as a [`RunnerError`] rather than unwinding into core.

use std::sync::{Arc, Mutex};

use thiserror::Error;
use wasmer::{
	imports, Function, FunctionEnv, FunctionEnvMut, Instance, Memory, MemoryView, Module, Store,
	TypedFunction, WasmPtr,
};

use super::permissions::ExtensionPermissions;
use super::types::ExtensionManifest;

#[derive(Error, Debug)]
pub enum RunnerError {
	#[error("failed to compile WASM module: {0}")]
	Compile(String),

	#[error("failed to instantiate WASM module: {0}")]
	Instantiate(String),

	#[error("missing required export '{0}': {1}")]
	MissingExport(String, String),

	#[error("WASM trap during '{0}': {1}")]
	Trap(String, String),

	#[error("WASM memory access error: {0}")]
	Memory(String),

	#[error("capability denied: {0}")]
	CapabilityDenied(String),

	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
}

/// A job export an extension registered during `plugin_init()`.
#[derive(Debug, Clone)]
pub struct RegisteredJob {
	/// Logical job name (e.g. "counter").
	pub name: String,
	/// WASM export function the host must call (e.g. "execute_test_counter").
	pub export_fn: String,
	/// Whether the job can be resumed from a checkpoint.
	pub resumable: bool,
}

/// Result of running a single job export to completion (or interruption).
#[derive(Debug, Clone)]
pub struct JobRun {
	/// Exit code from the job ABI: 0 = completed, 1 = interrupted, 2 = failed.
	pub exit_code: i32,
	/// Latest checkpoint bytes the job persisted (JSON state), if any.
	pub checkpoint: Option<Vec<u8>>,
	/// Total items the job reported via `increment_items`.
	pub items: u64,
	/// Last progress fraction the job reported (0.0..=1.0).
	pub last_progress: f32,
	/// Captured log lines as (level, message).
	pub logs: Vec<(u32, String)>,
}

impl JobRun {
	/// True if the job ran to completion.
	pub fn completed(&self) -> bool {
		self.exit_code == 0
	}

	/// True if the job stopped early because it was interrupted.
	pub fn interrupted(&self) -> bool {
		self.exit_code == 1
	}
}

/// Shared execution state, mutated by host functions during a job call.
#[derive(Default)]
struct ExecState {
	/// Latest state bytes saved via `job_checkpoint`.
	checkpoint: Option<Vec<u8>>,
	/// When armed, `job_check_interrupt` returns true once `interrupt_after`
	/// checks have elapsed. This makes mid-run interruption deterministic.
	interrupt_armed: bool,
	interrupt_after: u32,
	interrupt_checks: u32,
	/// Last progress fraction reported by the job.
	last_progress: f32,
	/// Running total of items reported by the job.
	items: u64,
	/// Captured log lines (level, message).
	logs: Vec<(u32, String)>,
	/// Jobs registered during `plugin_init()`.
	jobs: Vec<RegisteredJob>,
}

/// Environment shared with every host function call.
struct HostEnv {
	extension_id: String,
	permissions: ExtensionPermissions,
	/// Instance memory, set after instantiation.
	memory: Option<Memory>,
	state: Arc<Mutex<ExecState>>,
}

/// Loads and runs a single WASM extension in isolation.
pub struct WasmRunner {
	store: Store,
	instance: Instance,
	env: FunctionEnv<HostEnv>,
	state: Arc<Mutex<ExecState>>,
	extension_id: String,
	permissions: ExtensionPermissions,
}

impl WasmRunner {
	/// Load and instantiate an extension from a manifest and the directory that
	/// contains its `.wasm` file.
	pub fn from_manifest_dir(
		manifest: ExtensionManifest,
		dir: &std::path::Path,
	) -> Result<Self, RunnerError> {
		let wasm_path = dir.join(&manifest.wasm_file);
		let wasm_bytes = std::fs::read(&wasm_path)?;
		Self::from_bytes(manifest, &wasm_bytes)
	}

	/// Load and instantiate an extension from raw WASM bytes plus its manifest.
	pub fn from_bytes(manifest: ExtensionManifest, wasm_bytes: &[u8]) -> Result<Self, RunnerError> {
		let mut store = Store::default();

		let module =
			Module::new(&store, wasm_bytes).map_err(|e| RunnerError::Compile(e.to_string()))?;

		let permissions =
			ExtensionPermissions::from_manifest(manifest.id.clone(), &manifest.permissions);
		let state = Arc::new(Mutex::new(ExecState::default()));

		let host_env = HostEnv {
			extension_id: manifest.id.clone(),
			permissions: permissions.clone(),
			memory: None,
			state: state.clone(),
		};
		let env = FunctionEnv::new(&mut store, host_env);

		let import_object = imports! {
			"spacedrive" => {
				"spacedrive_call" => Function::new_typed_with_env(&mut store, &env, host_spacedrive_call),
				"spacedrive_log" => Function::new_typed_with_env(&mut store, &env, host_log),
				"register_job" => Function::new_typed_with_env(&mut store, &env, host_register_job),
				"job_report_progress" => Function::new_typed_with_env(&mut store, &env, host_report_progress),
				"job_checkpoint" => Function::new_typed_with_env(&mut store, &env, host_checkpoint),
				"job_check_interrupt" => Function::new_typed_with_env(&mut store, &env, host_check_interrupt),
				"job_add_warning" => Function::new_typed_with_env(&mut store, &env, host_add_warning),
				"job_increment_bytes" => Function::new_typed_with_env(&mut store, &env, host_increment_bytes),
				"job_increment_items" => Function::new_typed_with_env(&mut store, &env, host_increment_items),
			}
		};

		let instance = Instance::new(&mut store, &module, &import_object)
			.map_err(|e| RunnerError::Instantiate(e.to_string()))?;

		let memory = instance
			.exports
			.get_memory("memory")
			.map_err(|e| RunnerError::Instantiate(format!("missing memory export: {e}")))?
			.clone();
		env.as_mut(&mut store).memory = Some(memory);

		Ok(Self {
			store,
			instance,
			env,
			state,
			extension_id: manifest.id,
			permissions,
		})
	}

	/// Run the extension's `plugin_init()`, which registers its jobs.
	pub fn init(&mut self) -> Result<(), RunnerError> {
		let init: TypedFunction<(), i32> = self
			.instance
			.exports
			.get_typed_function(&self.store, "plugin_init")
			.map_err(|e| RunnerError::MissingExport("plugin_init".into(), e.to_string()))?;

		let code = init
			.call(&mut self.store)
			.map_err(|e| RunnerError::Trap("plugin_init".into(), e.to_string()))?;

		if code != 0 {
			return Err(RunnerError::Trap(
				"plugin_init".into(),
				format!("returned non-zero code {code}"),
			));
		}
		Ok(())
	}

	/// Jobs the extension registered during `init()`.
	pub fn registered_jobs(&self) -> Vec<RegisteredJob> {
		self.state.lock().unwrap().jobs.clone()
	}

	/// Capability gate: reject host operations the manifest did not grant.
	pub fn check_capability(&self, method: &str) -> Result<(), RunnerError> {
		if self.permissions.can_call(method) {
			Ok(())
		} else {
			Err(RunnerError::CapabilityDenied(format!(
				"extension '{}' is not permitted to call '{}'",
				self.extension_id, method
			)))
		}
	}

	/// Arm deterministic interruption: `job_check_interrupt` returns true after
	/// `after_checks` checks during the next job run.
	pub fn arm_interrupt(&self, after_checks: u32) {
		let mut st = self.state.lock().unwrap();
		st.interrupt_armed = true;
		st.interrupt_after = after_checks;
		st.interrupt_checks = 0;
	}

	/// Clear any armed interruption.
	pub fn disarm_interrupt(&self) {
		let mut st = self.state.lock().unwrap();
		st.interrupt_armed = false;
		st.interrupt_checks = 0;
	}

	/// Execute a job export to completion (or interruption).
	///
	/// `ctx_json` carries the job/library identifiers; `state_json` is the
	/// initial job state (pass `None` to start from the job's default state, or
	/// a previous checkpoint's bytes to resume). Per-run counters are reset so
	/// each call reports its own progress/items.
	pub fn run_job(
		&mut self,
		export_fn: &str,
		ctx_json: &str,
		state_json: Option<&str>,
	) -> Result<JobRun, RunnerError> {
		// Reset per-run accumulators while preserving interrupt arming.
		{
			let mut st = self.state.lock().unwrap();
			st.checkpoint = None;
			st.last_progress = 0.0;
			st.items = 0;
			st.logs.clear();
		}

		let (ctx_ptr, ctx_len) = self.write_bytes(ctx_json.as_bytes())?;
		let (state_ptr, state_len) = match state_json {
			Some(s) => self.write_bytes(s.as_bytes())?,
			None => (0u32, 0u32),
		};

		let func: TypedFunction<(u32, u32, u32, u32), i32> = self
			.instance
			.exports
			.get_typed_function(&self.store, export_fn)
			.map_err(|e| RunnerError::MissingExport(export_fn.into(), e.to_string()))?;

		let exit_code = func
			.call(&mut self.store, ctx_ptr, ctx_len, state_ptr, state_len)
			.map_err(|e| RunnerError::Trap(export_fn.into(), e.to_string()))?;

		let st = self.state.lock().unwrap();
		Ok(JobRun {
			exit_code,
			checkpoint: st.checkpoint.clone(),
			items: st.items,
			last_progress: st.last_progress,
			logs: st.logs.clone(),
		})
	}

	/// Allocate guest memory via the extension's `wasm_alloc` and copy `data` in.
	fn write_bytes(&mut self, data: &[u8]) -> Result<(u32, u32), RunnerError> {
		if data.is_empty() {
			return Ok((0, 0));
		}

		let alloc: TypedFunction<i32, i32> = self
			.instance
			.exports
			.get_typed_function(&self.store, "wasm_alloc")
			.map_err(|e| RunnerError::MissingExport("wasm_alloc".into(), e.to_string()))?;

		let ptr = alloc
			.call(&mut self.store, data.len() as i32)
			.map_err(|e| RunnerError::Trap("wasm_alloc".into(), e.to_string()))?;
		if ptr == 0 {
			return Err(RunnerError::Memory("wasm_alloc returned null".into()));
		}

		let memory = self
			.env
			.as_ref(&self.store)
			.memory
			.clone()
			.ok_or_else(|| RunnerError::Memory("instance memory unavailable".into()))?;
		let view = memory.view(&self.store);
		let wptr = WasmPtr::<u8>::new(ptr as u32);
		wptr.slice(&view, data.len() as u32)
			.and_then(|slice| slice.write_slice(data))
			.map_err(|e| RunnerError::Memory(format!("{e:?}")))?;

		Ok((ptr as u32, data.len() as u32))
	}
}

// === Host functions ===

fn read_string(view: &MemoryView, ptr: WasmPtr<u8>, len: u32) -> Option<String> {
	let bytes = ptr
		.slice(view, len)
		.and_then(|slice| slice.read_to_vec())
		.ok()?;
	String::from_utf8(bytes).ok()
}

/// Generic Wire RPC bridge. The standalone runner has no operation registry, so
/// this only enforces the capability gate and reports that the call is
/// unavailable. It never grants access outside the manifest's declared methods.
fn host_spacedrive_call(
	mut env: FunctionEnvMut<HostEnv>,
	method_ptr: WasmPtr<u8>,
	method_len: u32,
	_library_id_ptr: u32,
	_payload_ptr: WasmPtr<u8>,
	_payload_len: u32,
) -> u32 {
	let (host, store) = env.data_and_store_mut();
	let memory = match host.memory.clone() {
		Some(m) => m,
		None => return 0,
	};
	let view = memory.view(&store);
	let method = read_string(&view, method_ptr, method_len).unwrap_or_default();

	if !host.permissions.can_call(&method) {
		tracing::warn!(
			extension = %host.extension_id,
			method = %method,
			"Capability denied: extension attempted an ungranted operation"
		);
		return 0;
	}

	tracing::debug!(
		extension = %host.extension_id,
		method = %method,
		"spacedrive_call is unavailable in the standalone runner"
	);
	0
}

fn host_log(mut env: FunctionEnvMut<HostEnv>, level: u32, msg_ptr: WasmPtr<u8>, msg_len: u32) {
	let (host, store) = env.data_and_store_mut();
	let memory = match host.memory.clone() {
		Some(m) => m,
		None => return,
	};
	let view = memory.view(&store);
	let message = read_string(&view, msg_ptr, msg_len).unwrap_or_default();

	match level {
		0 => tracing::debug!(extension = %host.extension_id, "{}", message),
		2 => tracing::warn!(extension = %host.extension_id, "{}", message),
		3 => tracing::error!(extension = %host.extension_id, "{}", message),
		_ => tracing::info!(extension = %host.extension_id, "{}", message),
	}

	host.state.lock().unwrap().logs.push((level, message));
}

fn host_register_job(
	mut env: FunctionEnvMut<HostEnv>,
	name_ptr: WasmPtr<u8>,
	name_len: u32,
	export_ptr: WasmPtr<u8>,
	export_len: u32,
	resumable: u32,
) -> i32 {
	let (host, store) = env.data_and_store_mut();
	let memory = match host.memory.clone() {
		Some(m) => m,
		None => return 1,
	};
	let view = memory.view(&store);
	let name = match read_string(&view, name_ptr, name_len) {
		Some(n) => n,
		None => return 1,
	};
	let export_fn = match read_string(&view, export_ptr, export_len) {
		Some(e) => e,
		None => return 1,
	};

	tracing::info!(extension = %host.extension_id, job = %name, export = %export_fn, "Registered extension job");
	host.state.lock().unwrap().jobs.push(RegisteredJob {
		name,
		export_fn,
		resumable: resumable != 0,
	});
	0
}

fn host_report_progress(
	mut env: FunctionEnvMut<HostEnv>,
	_job_id_ptr: WasmPtr<u8>,
	progress: f32,
	msg_ptr: WasmPtr<u8>,
	msg_len: u32,
) {
	let (host, store) = env.data_and_store_mut();
	let message = host
		.memory
		.clone()
		.and_then(|m| read_string(&m.view(&store), msg_ptr, msg_len))
		.unwrap_or_default();

	tracing::debug!(extension = %host.extension_id, progress, "{}", message);
	host.state.lock().unwrap().last_progress = progress;
}

fn host_checkpoint(
	mut env: FunctionEnvMut<HostEnv>,
	_job_id_ptr: WasmPtr<u8>,
	state_ptr: WasmPtr<u8>,
	state_len: u32,
) -> i32 {
	let (host, store) = env.data_and_store_mut();
	let memory = match host.memory.clone() {
		Some(m) => m,
		None => return 1,
	};
	let view = memory.view(&store);
	let bytes = match state_ptr
		.slice(&view, state_len)
		.and_then(|slice| slice.read_to_vec())
	{
		Ok(b) => b,
		Err(e) => {
			tracing::error!(extension = %host.extension_id, "Failed to read checkpoint: {e:?}");
			return 1;
		}
	};

	tracing::debug!(extension = %host.extension_id, bytes = bytes.len(), "Checkpoint saved");
	host.state.lock().unwrap().checkpoint = Some(bytes);
	0
}

fn host_check_interrupt(mut env: FunctionEnvMut<HostEnv>, _job_id_ptr: WasmPtr<u8>) -> i32 {
	let host = env.data_mut();
	let mut st = host.state.lock().unwrap();
	if st.interrupt_armed {
		st.interrupt_checks += 1;
		if st.interrupt_checks > st.interrupt_after {
			return 1;
		}
	}
	0
}

fn host_add_warning(
	mut env: FunctionEnvMut<HostEnv>,
	_job_id_ptr: WasmPtr<u8>,
	msg_ptr: WasmPtr<u8>,
	msg_len: u32,
) {
	let (host, store) = env.data_and_store_mut();
	let message = host
		.memory
		.clone()
		.and_then(|m| read_string(&m.view(&store), msg_ptr, msg_len))
		.unwrap_or_default();
	tracing::warn!(extension = %host.extension_id, "Job warning: {}", message);
}

fn host_increment_bytes(mut env: FunctionEnvMut<HostEnv>, _job_id_ptr: WasmPtr<u8>, _bytes: u64) {
	let _ = env.data_mut();
}

fn host_increment_items(mut env: FunctionEnvMut<HostEnv>, _job_id_ptr: WasmPtr<u8>, count: u64) {
	let host = env.data_mut();
	host.state.lock().unwrap().items += count;
}
