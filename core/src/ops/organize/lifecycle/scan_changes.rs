use crate::{
	context::CoreContext,
	infra::{
		action::{error::ActionError, LibraryAction},
		job::{handle::JobReceipt, prelude::*},
	},
	ops::organize::{repository::OrganizeRepository, snapshot::scan_windows_snapshot},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeScanChangesInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Job)]
pub struct OrganizeChangeScanJob {
	pub task_id: Uuid,
	pub expected_revision: i64,
	pub root_path: PathBuf,
}

impl Job for OrganizeChangeScanJob {
	const NAME: &'static str = "organize_change_scan";
	const RESUMABLE: bool = false;
	const DESCRIPTION: Option<&'static str> =
		Some("Compare a task snapshot with the current Windows tree");
}

impl crate::infra::job::traits::DynJob for OrganizeChangeScanJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
}

#[async_trait::async_trait]
impl JobHandler for OrganizeChangeScanJob {
	type Output = JobOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		ctx.check_interrupt().await?;
		let scan = scan_windows_snapshot(self.root_path.clone())
			.await
			.map_err(|error| JobError::execution(error.to_string()))?;
		OrganizeRepository::new(ctx.library_db())
			.store_snapshot_change_scan(self.task_id, self.expected_revision, scan.clone())
			.await
			.map_err(|error| JobError::execution(error.to_string()))?;
		Ok(JobOutput::custom(serde_json::json!({
			"entries": scan.totals.total_entries,
			"scan_issues": scan.totals.scan_issue_count,
		})))
	}
}

#[derive(Debug, Clone)]
pub struct OrganizeScanChangesAction {
	input: OrganizeScanChangesInput,
}

impl LibraryAction for OrganizeScanChangesAction {
	type Input = OrganizeScanChangesInput;
	type Output = JobReceipt;

	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(Self { input })
	}

	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let task = OrganizeRepository::new(library.db().conn())
			.get_task(self.input.task_id)
			.await
			.map_err(|error| ActionError::Database(error.to_string()))?;
		let placeholder = crate::infra::job::types::JobId::new();
		OrganizeRepository::new(library.db().conn())
			.begin_change_scan(
				self.input.task_id,
				self.input.expected_revision,
				placeholder,
			)
			.await
			.map_err(|error| ActionError::Database(error.to_string()))?;
		let handle = match library
			.jobs()
			.dispatch(OrganizeChangeScanJob {
				task_id: self.input.task_id,
				expected_revision: self.input.expected_revision,
				root_path: task.task.root_path.into(),
			})
			.await
		{
			Ok(handle) => handle,
			Err(error) => {
				let _ = OrganizeRepository::new(library.db().conn())
					.fail_change_scan(
						self.input.task_id,
						self.input.expected_revision,
						error.to_string(),
					)
					.await;
				return Err(ActionError::Job(error));
			}
		};
		let _ = OrganizeRepository::new(library.db().conn())
			.attach_change_scan_job(self.input.task_id, self.input.expected_revision, handle.id)
			.await;
		Ok(handle.into())
	}

	fn action_kind(&self) -> &'static str {
		"organize.scan_changes"
	}
}

crate::register_library_action!(OrganizeScanChangesAction, "organize.scan_changes");
