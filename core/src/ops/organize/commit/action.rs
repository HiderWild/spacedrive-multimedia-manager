use super::{
	build_commit_plan, plan::apply_preflight_blockers, preflight::preflight_all_roots,
	OrganizeCommitBlockReason, OrganizeCommitInput, OrganizeCommitJob, OrganizeCommitOutcome,
	OrganizeCommitPlanOutput,
};
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
		let mut plan = build_commit_plan(&task, &items).map_err(database_error)?;
		if plan.can_commit && cfg!(windows) {
			let report = preflight_all_roots(db, &task, &items, &plan, false)
				.await
				.map_err(database_error)?;
			plan = apply_preflight_blockers(plan, &report);
		}
		if !blocked_plan_can_start(&plan, self.input.allow_current_subtree_drift) {
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

fn blocked_plan_can_start(
	plan: &OrganizeCommitPlanOutput,
	allow_current_subtree_drift: bool,
) -> bool {
	(plan.can_commit && plan.blocking_reasons.is_empty())
		|| (allow_current_subtree_drift
			&& !plan.blocking_reasons.is_empty()
			&& plan.blocking_reasons.iter().all(|reason| {
				matches!(
					reason,
					OrganizeCommitBlockReason::CurrentSubtreeDrift { .. }
				)
			}))
}

crate::register_library_action!(OrganizeCommitAction, "organize.commit");

#[cfg(test)]
mod tests {
	use super::*;

	fn blocked_plan(reasons: Vec<OrganizeCommitBlockReason>) -> OrganizeCommitPlanOutput {
		OrganizeCommitPlanOutput {
			revision: 1,
			move_groups: Vec::new(),
			discard_roots: Vec::new(),
			keep_units: 0,
			unmarked_units: 0,
			pending_addition_count: 0,
			changed_or_missing_roots: Vec::new(),
			failed_operation_roots: Vec::new(),
			unsafe_conflicts: Vec::new(),
			can_commit: false,
			blocking_reasons: reasons,
		}
	}

	#[test]
	fn explicit_confirmation_allows_only_current_subtree_drift() {
		let plan = blocked_plan(vec![OrganizeCommitBlockReason::CurrentSubtreeDrift {
			item_ids: vec![Uuid::new_v4()],
		}]);

		assert!(blocked_plan_can_start(&plan, true));
		assert!(!blocked_plan_can_start(&plan, false));
	}

	#[test]
	fn confirmation_does_not_override_other_blockers_or_mixed_reasons() {
		for reason in [
			OrganizeCommitBlockReason::TaskNotActive {
				status: crate::ops::organize::model::OrganizeTaskStatus::Scanning,
			},
			OrganizeCommitBlockReason::PendingAdditions { count: 1 },
			OrganizeCommitBlockReason::ChangedOrMissing {
				item_ids: vec![Uuid::new_v4()],
			},
			OrganizeCommitBlockReason::UnsafeTopology {
				conflicts: Vec::new(),
			},
			OrganizeCommitBlockReason::NoPhysicalOperations,
		] {
			let plan = blocked_plan(vec![reason]);
			assert!(!blocked_plan_can_start(&plan, true));
		}

		let mixed = blocked_plan(vec![
			OrganizeCommitBlockReason::CurrentSubtreeDrift {
				item_ids: vec![Uuid::new_v4()],
			},
			OrganizeCommitBlockReason::PendingAdditions { count: 1 },
		]);
		assert!(!blocked_plan_can_start(&mixed, true));
	}

	#[test]
	fn inconsistent_can_commit_flag_does_not_bypass_blockers() {
		let mut plan = blocked_plan(vec![OrganizeCommitBlockReason::PendingAdditions {
			count: 1,
		}]);
		plan.can_commit = true;

		assert!(!blocked_plan_can_start(&plan, true));
	}
}
