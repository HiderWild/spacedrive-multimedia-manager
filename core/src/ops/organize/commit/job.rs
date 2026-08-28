use super::{OrganizeCommitJob, OrganizeCommitOutput, OrganizeCommitPhase, OrganizeMoveGroup, OrganizePlanRoot};
use crate::domain::addressing::SdPathBatch;
use crate::infra::job::prelude::*;
use crate::ops::files::copy::{
	action::FileConflictResolution, input::CopyMethod, CopyOptions, FileCopyJob, MoveMode,
};
use crate::ops::files::delete::job::DeleteJob;
use crate::ops::organize::commit::preflight::preflight_all_roots;
use crate::ops::organize::model::OrganizeOperationState;
use crate::ops::organize::repository::{OperationSettlement, OrganizeRepository};
use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::collections::HashSet;
use uuid::Uuid;

impl Job for OrganizeCommitJob {
	const NAME: &'static str = "organize_commit";
	const RESUMABLE: bool = true;
	const DESCRIPTION: Option<&'static str> = Some("Preflight and execute organize task actions");
}

impl crate::infra::job::traits::DynJob for OrganizeCommitJob {
	fn job_name(&self) -> &'static str {
		Self::NAME
	}
}

#[async_trait::async_trait]
impl JobHandler for OrganizeCommitJob {
	type Output = OrganizeCommitOutput;

	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output> {
		let db = ctx.library_db();
		let task = crate::infra::db::entities::organize_task::Entity::find_by_id(self.task_id)
			.one(db)
			.await
			.map_err(|error| JobError::execution(error.to_string()))?
			.ok_or_else(|| JobError::execution("organize task disappeared"))?;
		let items = crate::infra::db::entities::organize_task_item::Entity::find()
			.filter(crate::infra::db::entities::organize_task_item::Column::TaskId.eq(self.task_id))
			.all(db)
			.await
			.map_err(|error| JobError::execution(error.to_string()))?;

		if matches!(self.checkpoint.phase, OrganizeCommitPhase::Preflight) {
			let report = preflight_all_roots(
				db,
				&task,
				&items,
				&self.plan,
				self.allow_current_subtree_drift,
			)
			.await
			.map_err(|error| JobError::execution(error.to_string()))?;
			if !report.is_ok() {
				let message = report.failure_message();
				self.settle_all(ctx.library_db(), &message, false).await?;
				return Err(JobError::execution(message));
			}
			self.checkpoint.phase = OrganizeCommitPhase::MoveGroups;
			self.checkpoint_with(&ctx).await?;
		}

		if let Some(missing_id) = missing_checkpoint_settlement(&self.checkpoint.completed_root_ids, &self.checkpoint.settlements) {
			return Err(JobError::execution(format!("checkpoint has no settlement for completed root {missing_id}")));
		}
		let mut settlements = self.checkpoint.settlements.clone();
		self.reconcile_active_child(&ctx, &mut settlements).await?;
		for index in self.checkpoint.next_move_group..self.plan.move_groups.len() {
			let group = self.plan.move_groups[index].clone();
			ctx.check_interrupt().await?;
			self.checkpoint.active_child_job_id = None;
			let result = self.run_move_group(&ctx, &group).await;
			match result {
				Ok(()) => settlements.extend(group.roots.iter().map(|root| OperationSettlement {
					item_id: root.item_id,
					state: OrganizeOperationState::Applied,
					last_error: None,
					applied_at: Some(Utc::now()),
				})),
				Err(error) => {
					settlements.extend(group.roots.iter().map(|root| OperationSettlement {
						item_id: root.item_id,
						state: OrganizeOperationState::Failed,
						last_error: Some(error.clone()),
						applied_at: None,
					}))
				}
			}
			self.checkpoint.next_move_group = index + 1;
			self.checkpoint
				.completed_root_ids
				.extend(group.roots.iter().map(|root| root.item_id));
			self.checkpoint.settlements = unique_settlements(settlements.clone());
			self.checkpoint_with(&ctx).await?;
		}

		if !self.plan.discard_roots.is_empty() {
			self.checkpoint.phase = OrganizeCommitPhase::DeleteRoots;
			for root in self
				.plan
				.discard_roots
				.iter()
				.filter(|root| !self.checkpoint.completed_root_ids.contains(&root.item_id))
			.cloned()
			{
				self.checkpoint.delete_dispatched = true;
				self.checkpoint_with(&ctx).await?;
				let result = self.run_delete_root(&ctx, &root).await;
				settlements.push(match result {
					Ok(()) => OperationSettlement { item_id: root.item_id, state: OrganizeOperationState::Applied, last_error: None, applied_at: Some(Utc::now()) },
					Err(error) => OperationSettlement { item_id: root.item_id, state: OrganizeOperationState::Failed, last_error: Some(error), applied_at: None },
				});
				self.checkpoint.completed_root_ids.push(root.item_id);
				self.checkpoint.active_child_job_id = None;
				self.checkpoint.settlements = unique_settlements(settlements.clone());
				self.checkpoint_with(&ctx).await?;
			}
		}

		self.checkpoint.phase = OrganizeCommitPhase::Settle;
		let unique = unique_settlements(settlements);
		let revision = OrganizeRepository::new(db)
			.settle_operation_roots(self.task_id, unique.clone())
			.await
			.map_err(|error| JobError::execution(error.to_string()))?;
		self.checkpoint.completed_root_ids = unique.iter().map(|item| item.item_id).collect();
		self.checkpoint.settlements = unique.clone();
		self.checkpoint_with(&ctx).await?;
		let failed_root_ids = unique
			.iter()
			.filter(|item| item.state == OrganizeOperationState::Failed)
			.map(|item| item.item_id)
			.collect();
		let applied_root_ids = unique
			.iter()
			.filter(|item| item.state == OrganizeOperationState::Applied)
			.map(|item| item.item_id)
			.collect();
		Ok(OrganizeCommitOutput {
			revision,
			applied_root_ids,
			failed_root_ids,
		})
	}

	async fn on_cancel(&mut self, ctx: &JobContext<'_>) -> JobResult<()> {
		self.settle_all(ctx.library_db(), "organize commit cancelled", false)
			.await
	}
}

impl OrganizeCommitJob {
	async fn reconcile_active_child(
		&mut self,
		ctx: &JobContext<'_>,
		settlements: &mut Vec<OperationSettlement>,
	) -> JobResult<()> {
		let Some(child_id) = self.checkpoint.active_child_job_id else {
			return Ok(());
		};
		let status = if let Some(handle) = ctx.library().jobs().get_job(child_id).await {
			handle.wait().await.map(|_| JobStatus::Completed).unwrap_or(JobStatus::Failed)
		} else {
			let status = ctx.library().jobs().get_job_info(child_id.0).await.map_err(|error| JobError::execution(error.to_string()))?.ok_or_else(|| JobError::execution(format!("active organize child job {child_id} disappeared")))?.status;
			if !status.is_terminal() {
				return Err(JobError::execution(format!("active organize child job {child_id} is not terminal: {status:?}")));
			}
			status
		};
		let succeeded = child_succeeded(status);
		let roots: Vec<Uuid> = match self.checkpoint.phase {
			OrganizeCommitPhase::MoveGroups => self.plan.move_groups.get(self.checkpoint.next_move_group).map(|group| group.roots.iter().map(|root| root.item_id).collect()).unwrap_or_default(),
			OrganizeCommitPhase::DeleteRoots => self.plan.discard_roots.iter().find(|root| !self.checkpoint.completed_root_ids.contains(&root.item_id)).map(|root| vec![root.item_id]).unwrap_or_default(),
			_ => Vec::new(),
		};
		for item_id in roots {
			settlements.push(OperationSettlement { item_id, state: if succeeded { OrganizeOperationState::Applied } else { OrganizeOperationState::Failed }, last_error: (!succeeded).then(|| format!("child job {child_id} ended with {status:?}")), applied_at: succeeded.then_some(Utc::now()) });
			self.checkpoint.completed_root_ids.push(item_id);
		}
		if matches!(self.checkpoint.phase, OrganizeCommitPhase::MoveGroups) && !self.plan.move_groups.is_empty() {
			self.checkpoint.next_move_group += 1;
		}
		self.checkpoint.active_child_job_id = None;
		self.checkpoint.delete_dispatched = false;
		self.checkpoint.settlements = unique_settlements(settlements.clone());
		self.checkpoint_with(ctx).await
	}

	async fn run_delete_root(&mut self, ctx: &JobContext<'_>, root: &OrganizePlanRoot) -> Result<(), String> {
		let handle = ctx.library().jobs().dispatch(DeleteJob::permanent(SdPathBatch::new(vec![root.source.clone()]), true)).await.map_err(|error| error.to_string())?;
		self.checkpoint.active_child_job_id = Some(handle.id());
		self.checkpoint_with(ctx).await.map_err(|error| error.to_string())?;
		handle.wait().await.map(|_| ()).map_err(|error| error.to_string())
	}

	async fn run_move_group(
		&mut self,
		ctx: &JobContext<'_>,
		group: &OrganizeMoveGroup,
	) -> Result<(), String> {
		let sources = group.roots.iter().map(|root| root.source.clone()).collect();
		let job = FileCopyJob::new(SdPathBatch::new(sources), group.destination.clone())
			.with_options(CopyOptions {
				overwrite: self.move_conflict_policy == FileConflictResolution::Overwrite,
				verify_checksum: false,
				preserve_timestamps: true,
				delete_after_copy: true,
				move_mode: Some(MoveMode::Move),
				copy_method: CopyMethod::Auto,
				conflict_resolution: Some(self.move_conflict_policy),
			});
		let handle = ctx
			.library()
			.jobs()
			.dispatch(job)
			.await
			.map_err(|error| error.to_string())?;
		self.checkpoint.active_child_job_id = Some(handle.id());
		self.checkpoint_with(ctx)
			.await
			.map_err(|error| error.to_string())?;
		handle
			.wait()
			.await
			.map(|_| ())
			.map_err(|error| error.to_string())
	}

	async fn checkpoint_with(&self, ctx: &JobContext<'_>) -> JobResult<()> {
		ctx.checkpoint_with_state(self).await
	}

	async fn settle_all(
		&self,
		db: &sea_orm::DatabaseConnection,
		message: &str,
		applied: bool,
	) -> JobResult<()> {
		let mut roots = self
			.plan
			.move_groups
			.iter()
			.flat_map(|group| group.roots.iter())
			.chain(self.plan.discard_roots.iter())
			.map(|root| OperationSettlement {
				item_id: root.item_id,
				state: if applied {
					OrganizeOperationState::Applied
				} else {
					OrganizeOperationState::Failed
				},
				last_error: (!applied).then(|| message.to_string()),
				applied_at: applied.then_some(Utc::now()),
			})
			.collect::<Vec<_>>();
		roots = unique_settlements(roots);
		OrganizeRepository::new(db)
			.settle_operation_roots(self.task_id, roots)
			.await
			.map(|_| ())
			.map_err(|error| JobError::execution(error.to_string()))
	}
}

fn unique_settlements(values: Vec<OperationSettlement>) -> Vec<OperationSettlement> {
	let mut seen = HashSet::new();
	values
		.into_iter()
		.filter(|value| seen.insert(value.item_id))
		.collect()
}

fn child_succeeded(status: JobStatus) -> bool {
	status == JobStatus::Completed
}

fn missing_checkpoint_settlement(completed_root_ids: &[Uuid], settlements: &[OperationSettlement]) -> Option<Uuid> {
	completed_root_ids.iter().copied().find(|id| !settlements.iter().any(|settlement| settlement.item_id == *id))
}

impl From<OrganizeCommitOutput> for JobOutput {
	fn from(output: OrganizeCommitOutput) -> Self {
		JobOutput::custom(serde_json::json!({
			"revision": output.revision,
			"applied_root_ids": output.applied_root_ids,
			"failed_root_ids": output.failed_root_ids,
		}))
	}
}

#[cfg(test)]
mod tests {
	use super::{child_succeeded, missing_checkpoint_settlement, OperationSettlement};
	use crate::infra::job::JobStatus;
	use crate::ops::organize::model::OrganizeOperationState;
	use chrono::Utc;
	use uuid::Uuid;

	#[test]
	fn only_completed_child_jobs_are_reconciled_as_applied() {
		assert!(child_succeeded(JobStatus::Completed));
		assert!(!child_succeeded(JobStatus::Failed));
		assert!(!child_succeeded(JobStatus::Cancelled));
	}

	#[test]
	fn refuses_old_checkpoint_that_cannot_prove_completed_root_outcome() {
		let completed = Uuid::from_u128(1);
		let settlement = OperationSettlement { item_id: Uuid::from_u128(2), state: OrganizeOperationState::Applied, last_error: None, applied_at: Some(Utc::now()) };
		assert_eq!(missing_checkpoint_settlement(&[completed], &[settlement]), Some(completed));
	}
}
