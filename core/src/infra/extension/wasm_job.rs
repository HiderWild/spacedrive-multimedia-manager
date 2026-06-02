//! WASM Job Executor
//!
//! Native job adapter that executes a WASM extension job through the F-01
//! [`WasmRunner`]. The runner's `wasmer::Store` is not `Send`, so execution runs
//! on a blocking task built from owned manifest + WASM bytes; the async job
//! future never holds a non-`Send` value across an await point.
//!
//! [`WasmRunner`]: super::runner::WasmRunner

use serde::{Deserialize, Serialize};

use super::runner::WasmRunner;
use crate::infra::job::prelude::*;

/// Generic job for executing WASM extension jobs
#[derive(Debug, Serialize, Deserialize, Job)]
pub struct WasmJob {
	/// Extension ID
	pub extension_id: String,

	/// WASM export function name (e.g., "execute_test_counter")
	pub export_fn: String,

	/// Job state as JSON string
	pub state_json: String,

	/// For resumability - track if this is a resumed job
	#[serde(skip)]
	pub is_resuming: bool,
}

impl Job for WasmJob {
	const NAME: &'static str = "wasm_job";
	const RESUMABLE: bool = true;
	const VERSION: u32 = 1;
	const DESCRIPTION: Option<&'static str> = Some("Execute WASM extension job");
}

impl crate::infra::job::traits::DynJob for WasmJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
}

// ErasedJob implementation - uses Job derive macro like other jobs

#[async_trait::async_trait]
impl JobHandler for WasmJob {
	type Output = JobOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		tracing::info!(
			job_id = %ctx.id(),
			extension = %self.extension_id,
			export_fn = %self.export_fn,
			"Executing WASM job"
		);

		// Get PluginManager through Library → CoreContext.
		let pm = ctx
			.library()
			.core_context()
			.get_plugin_manager()
			.await
			.ok_or_else(|| {
				crate::infra::job::error::JobError::ExecutionFailed(
					"PluginManager not initialized".into(),
				)
			})?;

		// Pull owned, Send manifest + wasm bytes so the runner can be built on a
		// blocking task without dragging the manager's non-Send Store across awaits.
		let (manifest, wasm_bytes) = {
			let pm = pm.read().await;
			pm.load_extension_wasm(&self.extension_id).ok_or_else(|| {
				crate::infra::job::error::JobError::ExecutionFailed(format!(
					"Extension '{}' is not loaded or its WASM is unavailable",
					self.extension_id
				))
			})?
		};

		let ctx_json = serde_json::json!({
			"job_id": ctx.id().to_string(),
			"library_id": ctx.library().id().to_string(),
		})
		.to_string();

		let export_fn = self.export_fn.clone();
		let state_json = self.state_json.clone();

		ctx.log(&format!(
			"Running WASM job {}::{}",
			self.extension_id, self.export_fn
		));

		// Execute the WASM job on a blocking task. A trap/panic inside the module
		// is contained as a RunnerError and mapped to a typed job error, never
		// unwinding into core.
		let run = tokio::task::spawn_blocking(move || {
			let mut runner =
				WasmRunner::from_bytes(manifest, &wasm_bytes).map_err(|e| e.to_string())?;
			runner.init().map_err(|e| e.to_string())?;
			let state = if state_json.is_empty() {
				None
			} else {
				Some(state_json.as_str())
			};
			runner
				.run_job(&export_fn, &ctx_json, state)
				.map_err(|e| e.to_string())
		})
		.await
		.map_err(|e| {
			crate::infra::job::error::JobError::ExecutionFailed(format!("join error: {e}"))
		})?
		.map_err(crate::infra::job::error::JobError::ExecutionFailed)?;

		if !run.completed() {
			return Err(crate::infra::job::error::JobError::ExecutionFailed(
				format!(
					"WASM job exited with code {} (1 = interrupted, 2 = failed)",
					run.exit_code
				),
			));
		}

		ctx.log(&format!(
			"✓ WASM job completed: items={} progress={}",
			run.items, run.last_progress
		));

		Ok(JobOutput::Success)
	}

	async fn on_resume(&mut self, ctx: &JobContext<'_>) -> JobResult<()> {
		self.is_resuming = true;
		ctx.log("Resuming WASM job");
		Ok(())
	}

	fn is_resuming(&self) -> bool {
		self.is_resuming
	}
}

// Don't register automatically - will be registered when needed
// WasmJob is a special case, not auto-loaded like regular jobs
