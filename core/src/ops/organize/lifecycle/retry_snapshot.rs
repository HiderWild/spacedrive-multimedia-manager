use crate::{
	context::CoreContext,
	infra::{
		action::{error::ActionError, LibraryAction},
		job::{handle::JobReceipt, types::JobId},
	},
	ops::organize::{repository::OrganizeRepository, snapshot::OrganizeSnapshotJob},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeRetrySnapshotInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeRetrySnapshotOutcome {
	Started {
		revision: i64,
		snapshot_job: JobReceipt,
	},
	StaleRevision {
		current_revision: i64,
	},
}
#[derive(Debug, Clone)]
pub struct OrganizeRetrySnapshotAction {
	input: OrganizeRetrySnapshotInput,
}
impl LibraryAction for OrganizeRetrySnapshotAction {
	type Input = OrganizeRetrySnapshotInput;
	type Output = OrganizeRetrySnapshotOutcome;
	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(Self { input })
	}
	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let repo = OrganizeRepository::new(library.db().conn());
		let task = repo
			.get_task(self.input.task_id)
			.await
			.map_err(|e| ActionError::Database(e.to_string()))?;
		let job_id = JobId::new();
		let revision = repo
			.reset_snapshot_for_retry(self.input.task_id, self.input.expected_revision, job_id)
			.await
			.map_err(|e| ActionError::Database(e.to_string()))?;
		let job = match library
			.jobs()
			.dispatch(OrganizeSnapshotJob {
				task_id: self.input.task_id,
				root_path: task.task.root_path.into(),
				device_slug: task.task.device_slug.clone(),
			})
			.await
		{
			Ok(job) => job,
			Err(error) => {
				let _ = repo
					.fail_snapshot(self.input.task_id, error.to_string())
					.await;
				return Err(ActionError::Job(error));
			}
		};
		let _ = repo.attach_scan_job(self.input.task_id, job.id()).await;
		Ok(OrganizeRetrySnapshotOutcome::Started {
			revision,
			snapshot_job: job.into(),
		})
	}
	fn action_kind(&self) -> &'static str {
		"organize.retry_snapshot"
	}
}
crate::register_library_action!(OrganizeRetrySnapshotAction, "organize.retry_snapshot");
