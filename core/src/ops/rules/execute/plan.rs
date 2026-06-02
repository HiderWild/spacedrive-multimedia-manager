//! Data shapes for macro execution (task E-02).
//!
//! A [`MacroFilePlan`] is the unit of work the executor produces during
//! discovery: one matched file plus the actions every matching rule asked for.
//! [`MacroPlanItem`] flattens a single (file, action) pair for reporting, and
//! [`MacroExecutionResult`] is the run summary returned to callers (the planned
//! items in dry-run mode, success/failure counts and logs in a real run).

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::ops::rules::ActionRef;

/// One matched file and the actions queued for it.
///
/// Built during discovery and stored in the resumable job state, so a restarted
/// run replays exactly the work the original discovery found without
/// re-evaluating rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MacroFilePlan {
	/// UUID of the matched entry.
	pub entry_uuid: Uuid,
	/// Best-effort display path of the entry (for logs and the dry-run report).
	pub path: String,
	/// Actions to dispatch for this file, in order.
	pub actions: Vec<ActionRef>,
}

/// A single planned (file, action) pair surfaced in a dry-run report.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct MacroPlanItem {
	/// UUID of the entry the action targets.
	pub entry_uuid: Uuid,
	/// Best-effort display path of the entry.
	pub path: String,
	/// Wire name of the action (e.g. `tags.apply`).
	pub action: String,
	/// Opaque action parameters, mirroring the rule's [`ActionRef::params`].
	pub params: serde_json::Value,
}

/// Outcome of executing a macro over a library.
///
/// In dry-run mode `planned` lists every action that *would* run and the
/// success/failure counts stay zero. In a real run `planned` is empty and the
/// counts plus `failures` describe what happened. Per-item failures are recorded
/// here, never propagated, so one bad file never aborts the batch.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct MacroExecutionResult {
	/// Whether this was a dry run (no mutations performed).
	pub dry_run: bool,
	/// Number of files matched by at least one rule.
	pub matched_files: usize,
	/// Planned actions, populated only in dry-run mode.
	pub planned: Vec<MacroPlanItem>,
	/// Count of actions that ran successfully (real runs only).
	pub succeeded: usize,
	/// Count of actions that failed and were skipped (real runs only).
	pub failed: usize,
	/// Human-readable log line for each failed action.
	pub failures: Vec<String>,
}

impl MacroExecutionResult {
	/// Create an empty result flagged with `dry_run`.
	pub fn new(dry_run: bool) -> Self {
		Self {
			dry_run,
			matched_files: 0,
			planned: Vec::new(),
			succeeded: 0,
			failed: 0,
			failures: Vec::new(),
		}
	}
}

/// Per-file outcome accumulated while applying one [`MacroFilePlan`].
///
/// The executor folds these into a [`MacroExecutionResult`]; the job folds them
/// into its resumable state. Splitting the per-file result out keeps both
/// callers from duplicating the accumulation logic.
#[derive(Debug, Default, Clone)]
pub struct FileOutcome {
	/// Planned items for this file (dry-run only).
	pub planned: Vec<MacroPlanItem>,
	/// Actions that succeeded for this file.
	pub succeeded: usize,
	/// Actions that failed for this file.
	pub failed: usize,
	/// Failure log lines for this file.
	pub failures: Vec<String>,
}
