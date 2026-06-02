//! Macro execution action (task E-02).
//!
//! Runs a rule set over the library in a single RPC call. The wire name is
//! `rules.execute`. With `dry_run` set, it reports the planned actions without
//! mutating anything; otherwise it dispatches each action through the real
//! library actions via [`LibraryMacroDispatcher`] and returns the run summary.
//! Per-item failures are recorded in the result, never propagated, so one bad
//! file never aborts the batch.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::dispatch::LibraryMacroDispatcher;
use super::executor::run_macro;
use super::plan::MacroExecutionResult;
use crate::context::CoreContext;
use crate::infra::action::{error::ActionError, LibraryAction};
use crate::library::Library;
use crate::ops::rules::{validate_rule, Rule};

/// Input for [`MacroExecuteAction`].
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MacroExecuteInput {
	/// Rules to evaluate; matching files receive the rules' actions.
	pub rules: Vec<Rule>,
	/// When true, report planned actions without performing them.
	pub dry_run: bool,
}

/// Run a rule set over the library, optionally as a dry run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroExecuteAction {
	input: MacroExecuteInput,
}

impl MacroExecuteAction {
	pub fn new(input: MacroExecuteInput) -> Self {
		Self { input }
	}
}

impl LibraryAction for MacroExecuteAction {
	type Input = MacroExecuteInput;
	type Output = MacroExecutionResult;

	fn from_input(input: MacroExecuteInput) -> Result<Self, String> {
		for rule in &input.rules {
			validate_rule(rule).map_err(|e| e.to_string())?;
		}
		Ok(Self::new(input))
	}

	async fn execute(
		self,
		library: Arc<Library>,
		context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let dispatcher = LibraryMacroDispatcher::new(library.clone(), context);
		let result = run_macro(
			library.db().conn(),
			&dispatcher,
			&self.input.rules,
			self.input.dry_run,
		)
		.await
		.map_err(|e| ActionError::Database(e.to_string()))?;
		Ok(result)
	}

	fn action_kind(&self) -> &'static str {
		"rules.execute"
	}
}

crate::register_library_action!(MacroExecuteAction, "rules.execute");
