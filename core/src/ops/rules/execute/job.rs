//! Resumable batch macro execution job (task E-02).
//!
//! Wraps the [`executor`](super::executor) in the shared job framework so a
//! whole-library macro run is resumable. Discovery builds the work list once and
//! stores it in the job state; processing dispatches one file at a time and
//! checkpoints after each, so an interrupted run resumes at the next unprocessed
//! file without re-evaluating rules or re-applying completed work. Per-file
//! failures are logged and skipped, matching the synchronous
//! [`run_macro`](super::executor::run_macro) path.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::dispatch::LibraryMacroDispatcher;
use super::executor::{apply_file, discover_matches};
use super::plan::MacroFilePlan;
use crate::infra::job::{prelude::*, traits::DynJob};
use crate::ops::rules::Rule;

/// Execution phase of a macro job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MacroPhase {
	Discovery,
	Processing,
	Complete,
}

/// Resumable state for a batch macro run.
///
/// `plan` is populated during discovery, then `processed` advances one file at a
/// time. The job checkpoints after every file, so a resume picks up at the next
/// unprocessed entry without re-running completed work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroJobState {
	pub phase: MacroPhase,
	/// Whether to perform actions (`false`) or only report the plan (`true`).
	pub dry_run: bool,
	/// Files matched during discovery and the actions queued for each.
	pub plan: Vec<MacroFilePlan>,
	/// Number of files processed so far.
	pub processed: usize,
	/// Count of planned actions recorded (dry-run only).
	pub planned: usize,
	/// Count of actions that ran successfully.
	pub succeeded: usize,
	/// Count of actions that failed and were skipped.
	pub failed: usize,
	/// Failure log lines accumulated across the batch.
	pub failures: Vec<String>,
}

impl MacroJobState {
	fn new(dry_run: bool) -> Self {
		Self {
			phase: MacroPhase::Discovery,
			dry_run,
			plan: Vec::new(),
			processed: 0,
			planned: 0,
			succeeded: 0,
			failed: 0,
			failures: Vec::new(),
		}
	}
}

/// Batch macro execution job.
#[derive(Serialize, Deserialize)]
pub struct MacroExecutionJob {
	rules: Vec<Rule>,
	state: MacroJobState,
}

impl MacroExecutionJob {
	/// Create a job that runs `rules`, performing actions when `dry_run` is false.
	pub fn new(rules: Vec<Rule>, dry_run: bool) -> Self {
		Self {
			state: MacroJobState::new(dry_run),
			rules,
		}
	}
}

impl Job for MacroExecutionJob {
	const NAME: &'static str = "macro_execution";
	const RESUMABLE: bool = true;
	const DESCRIPTION: Option<&'static str> =
		Some("Run a rule set, dispatching its actions over matching files in batches");
}

impl DynJob for MacroExecutionJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
}

/// Summary of a completed macro job.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MacroExecutionJobOutput {
	pub dry_run: bool,
	pub matched_files: usize,
	pub planned: usize,
	pub succeeded: usize,
	pub failed: usize,
}

impl From<MacroExecutionJobOutput> for JobOutput {
	fn from(output: MacroExecutionJobOutput) -> Self {
		JobOutput::Custom(serde_json::json!({
			"type": "macro_execution",
			"dry_run": output.dry_run,
			"matched_files": output.matched_files,
			"planned": output.planned,
			"succeeded": output.succeeded,
			"failed": output.failed,
		}))
	}
}

#[async_trait::async_trait]
impl JobHandler for MacroExecutionJob {
	type Output = MacroExecutionJobOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		if self.state.phase == MacroPhase::Discovery {
			ctx.log("Starting macro discovery phase");
			let plan = discover_matches(ctx.library_db(), &self.rules)
				.await
				.map_err(|e| JobError::execution(format!("Discovery failed: {}", e)))?;
			ctx.log(format!("Discovery matched {} files", plan.len()));
			self.state.plan = plan;
			self.state.phase = MacroPhase::Processing;
		}

		// Real runs dispatch through the real library actions; this requires the
		// library's core context, which is reachable from the job's library.
		let dispatcher = LibraryMacroDispatcher::new(
			ctx.library_arc(),
			ctx.library().core_context().clone(),
		);

		let total = self.state.plan.len();
		while self.state.processed < total {
			ctx.check_interrupt().await?;

			let file = self.state.plan[self.state.processed].clone();
			let outcome = apply_file(&dispatcher, &file, self.state.dry_run).await;

			self.state.planned += outcome.planned.len();
			self.state.succeeded += outcome.succeeded;
			self.state.failed += outcome.failed;
			self.state.failures.extend(outcome.failures);
			self.state.processed += 1;

			ctx.progress(Progress::Count {
				current: self.state.processed,
				total,
			});

			// Checkpoint after each file so resume restarts at the next entry.
			ctx.checkpoint().await?;
		}

		self.state.phase = MacroPhase::Complete;
		ctx.log(format!(
			"Macro complete: {} matched, {} succeeded, {} failed (dry_run={})",
			total, self.state.succeeded, self.state.failed, self.state.dry_run
		));

		Ok(MacroExecutionJobOutput {
			dry_run: self.state.dry_run,
			matched_files: total,
			planned: self.state.planned,
			succeeded: self.state.succeeded,
			failed: self.state.failed,
		})
	}

	async fn on_resume(&mut self, ctx: &JobContext<'_>) -> JobResult {
		ctx.log(format!(
			"Resuming macro job at {}/{}",
			self.state.processed,
			self.state.plan.len()
		));
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::infra::job::traits::SerializableJob;
	use crate::ops::rules::{ActionRef, Condition};
	use uuid::Uuid;

	fn sample_rule() -> Rule {
		Rule {
			name: "tag mp4".to_string(),
			condition: Condition::Extension {
				value: "mp4".to_string(),
			},
			actions: vec![ActionRef {
				action: "tags.apply".to_string(),
				params: serde_json::json!({ "tags": ["video"] }),
			}],
		}
	}

	#[test]
	fn job_state_round_trips_for_resume() {
		// Resumability rides on the shared Job trait: state serializes to
		// MessagePack via SerializableJob and back identically, which is exactly
		// what the job manager checkpoints and restores mid-batch.
		let mut job = MacroExecutionJob::new(vec![sample_rule()], false);
		job.state.phase = MacroPhase::Processing;
		job.state.plan = vec![
			MacroFilePlan {
				entry_uuid: Uuid::nil(),
				path: "clips/a.mp4".to_string(),
				actions: sample_rule().actions,
			},
			MacroFilePlan {
				entry_uuid: Uuid::nil(),
				path: "clips/b.mp4".to_string(),
				actions: sample_rule().actions,
			},
		];
		job.state.processed = 1;
		job.state.succeeded = 1;

		let bytes = job.serialize_state().expect("serialize");
		let restored = MacroExecutionJob::deserialize_state(&bytes).expect("deserialize");

		assert_eq!(restored.state.phase, MacroPhase::Processing);
		assert_eq!(restored.state.processed, 1);
		assert_eq!(restored.state.succeeded, 1);
		assert_eq!(restored.state.plan.len(), 2);
		assert!(!restored.state.dry_run);
		assert_eq!(restored.rules.len(), 1);
	}
}
