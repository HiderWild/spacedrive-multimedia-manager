//! File delete action handler

use super::input::FileDeleteInput;
use super::job::{DeleteJob, DeleteMode, DeleteOptions};
use crate::{
	context::CoreContext,
	domain::addressing::{SdPath, SdPathBatch},
	infra::{
		action::{error::ActionError, LibraryAction},
		job::handle::JobHandle,
	},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDeleteAction {
	pub targets: SdPathBatch,
	pub options: DeleteOptions,
}

impl FileDeleteAction {
	/// Create a new file delete action
	pub fn new(targets: SdPathBatch, options: DeleteOptions) -> Self {
		Self { targets, options }
	}

	/// Create a delete action with default options
	pub fn with_defaults(targets: SdPathBatch) -> Self {
		Self::new(targets, DeleteOptions::default())
	}

	fn into_job(self) -> DeleteJob {
		if self.options.permanent {
			// Permanent deletes must carry explicit confirmation or the job rejects them.
			DeleteJob::permanent(self.targets, true)
		} else {
			DeleteJob::trash(self.targets)
		}
	}
}

// Implement the unified LibraryAction
impl LibraryAction for FileDeleteAction {
	type Input = FileDeleteInput;
	type Output = crate::infra::job::handle::JobReceipt;

	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(FileDeleteAction {
			targets: input.targets,
			options: DeleteOptions {
				permanent: input.permanent,
				recursive: input.recursive,
			},
		})
	}

	async fn execute(
		self,
		library: std::sync::Arc<crate::library::Library>,
		context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let job_handle = library
			.jobs()
			.dispatch(self.into_job())
			.await
			.map_err(ActionError::Job)?;

		Ok(job_handle.into())
	}

	fn action_kind(&self) -> &'static str {
		"files.delete"
	}

	async fn validate(
		&self,
		_library: &std::sync::Arc<crate::library::Library>,
		_context: std::sync::Arc<crate::context::CoreContext>,
	) -> Result<crate::infra::action::ValidationResult, ActionError> {
		// Validate targets
		if self.targets.paths.is_empty() {
			return Err(ActionError::Validation {
				field: "targets".to_string(),
				message: "At least one target file must be specified".to_string(),
			});
		}

		Ok(crate::infra::action::ValidationResult::Success { metadata: None })
	}
}

// Register this action with the new registry
crate::register_library_action!(FileDeleteAction, "files.delete");

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn permanent_actions_dispatch_confirmed_jobs() {
		let job = FileDeleteAction::new(
			SdPathBatch::default(),
			DeleteOptions {
				permanent: true,
				recursive: true,
			},
		)
		.into_job();

		assert!(matches!(job.mode, DeleteMode::Permanent));
		assert!(job.confirm_permanent);
	}

	#[test]
	fn trash_actions_dispatch_trash_jobs() {
		let job = FileDeleteAction::with_defaults(SdPathBatch::default()).into_job();

		assert!(matches!(job.mode, DeleteMode::Trash));
		assert!(!job.confirm_permanent);
	}
}
