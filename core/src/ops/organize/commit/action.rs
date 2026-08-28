use super::{build_commit_plan, OrganizeCommitInput, OrganizeCommitJob, OrganizeCommitOutcome};
use crate::context::CoreContext;
use crate::infra::action::{error::ActionError, LibraryAction};
use crate::infra::db::entities::{organize_task, organize_task_item};
use crate::infra::job::types::JobId;
use crate::ops::organize::error::OrganizeError;
use crate::ops::organize::repository::OrganizeRepository;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OrganizeCommitAction {
	input: OrganizeCommitInput,
}

impl LibraryAction for OrganizeCommitAction {
	type Input = OrganizeCommitInput;
	type Output = OrganizeCommitOutcome;

	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(Self { input })
	}

	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let db = library.db().conn();
		let task = organize_task::Entity::find_by_id(self.input.task_id)
			.one(db)
			.await?
			.ok_or_else(|| ActionError::Validation {
				field: "task_id".into(),
				message: "organize task does not exist".into(),
			})?;
		if task.revision != self.input.expected_revision {
			return Ok(OrganizeCommitOutcome::StaleRevision {
				current_revision: task.revision,
			});
		}
		let status = parse_status(&task.status).map_err(database_error)?;
		if status != crate::ops::organize::model::OrganizeTaskStatus::Active {
			return Ok(OrganizeCommitOutcome::RejectedState { status });
		}
		let items = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(self.input.task_id))
			.all(db)
			.await?;
		let plan = build_commit_plan(&task, &items).map_err(database_error)?;
		if !plan.can_commit {
			return Ok(OrganizeCommitOutcome::RejectedBlockedPlan {
				reasons: plan.blocking_reasons,
			});
		}
		if !plan.discard_roots.is_empty() && !self.input.permanent_delete_confirmed {
			return Ok(OrganizeCommitOutcome::RejectedPermanentConfirmation);
		}

		let job_id = JobId::new();
		let locked_revision = OrganizeRepository::new(db)
			.lock_for_commit(self.input.task_id, self.input.expected_revision, job_id)
			.await
			.map_err(database_error)?;
		let job = OrganizeCommitJob {
			task_id: self.input.task_id,
			locked_revision,
			plan,
			move_conflict_policy: self.input.move_conflict_policy,
			allow_current_subtree_drift: self.input.allow_current_subtree_drift,
			checkpoint: super::OrganizeCommitCheckpoint {
				phase: super::OrganizeCommitPhase::Preflight,
				next_move_group: 0,
				active_child_job_id: None,
				delete_dispatched: false,
				completed_root_ids: Vec::new(),
				settlements: Vec::new(),
			},
		};
		let handle = match library.jobs().dispatch_with_id(job_id, job).await {
			Ok(handle) => handle,
			Err(error) => {
				let message = error.to_string();
				let _ = OrganizeRepository::new(db)
					.fail_commit(self.input.task_id, locked_revision, message.clone())
					.await;
				return Err(ActionError::Job(error));
			}
		};
		if !OrganizeRepository::new(db)
			.attach_commit_job(self.input.task_id, locked_revision, job_id, handle.id())
			.await
			.map_err(database_error)?
		{
			let message = "organize commit job could not be attached".to_string();
			let _ = OrganizeRepository::new(db)
				.fail_commit(self.input.task_id, locked_revision, message.clone())
				.await;
			return Err(ActionError::Database(message));
		}
		Ok(OrganizeCommitOutcome::Started {
			job: handle.to_receipt(),
		})
	}

	fn action_kind(&self) -> &'static str {
		"organize.commit"
	}
}

fn parse_status(
	value: &str,
) -> Result<crate::ops::organize::model::OrganizeTaskStatus, OrganizeError> {
	match value {
		"scanning" => Ok(crate::ops::organize::model::OrganizeTaskStatus::Scanning),
		"active" => Ok(crate::ops::organize::model::OrganizeTaskStatus::Active),
		"committing" => Ok(crate::ops::organize::model::OrganizeTaskStatus::Committing),
		"completed" => Ok(crate::ops::organize::model::OrganizeTaskStatus::Completed),
		"failed" => Ok(crate::ops::organize::model::OrganizeTaskStatus::Failed),
		status => Err(OrganizeError::InvalidTaskState(status.into())),
	}
}

fn database_error(error: impl std::fmt::Display) -> ActionError {
	ActionError::Database(error.to_string())
}

crate::register_library_action!(OrganizeCommitAction, "organize.commit");
