use crate::{
	context::CoreContext,
	infra::{
		db::entities::{organize_task, organize_task_item},
		query::{LibraryQuery, QueryError, QueryResult},
	},
	ops::organize::commit::{
		build_commit_plan, plan::apply_preflight_blockers, preflight::preflight_all_roots,
		OrganizeCommitPlanInput, OrganizeCommitPlanOutput,
	},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::sync::Arc;

pub struct OrganizeCommitPlanQuery {
	input: OrganizeCommitPlanInput,
}

impl LibraryQuery for OrganizeCommitPlanQuery {
	type Input = OrganizeCommitPlanInput;
	type Output = OrganizeCommitPlanOutput;

	fn from_input(input: Self::Input) -> QueryResult<Self> {
		Ok(Self { input })
	}

	async fn execute(
		self,
		context: Arc<CoreContext>,
		session: crate::infra::api::SessionContext,
	) -> QueryResult<Self::Output> {
		let library_id = session
			.current_library_id
			.ok_or_else(|| QueryError::Internal("No library in session".into()))?;
		let library = context
			.libraries()
			.await
			.get_library(library_id)
			.await
			.ok_or_else(|| QueryError::LibraryNotFound(library_id))?;
		let db = library.db().conn();
		let task = organize_task::Entity::find_by_id(self.input.task_id)
			.one(db)
			.await?
			.ok_or_else(|| QueryError::InvalidInput("organize task does not exist".into()))?;
		if task.revision != self.input.expected_revision {
			return Err(QueryError::InvalidInput(format!(
				"stale organize revision, current revision is {}",
				task.revision
			)));
		}
		let items = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(self.input.task_id))
			.all(db)
			.await?;
		let mut plan = build_commit_plan(&task, &items)
			.map_err(|error| QueryError::Internal(error.to_string()))?;
		if plan.can_commit && cfg!(windows) {
			let report = preflight_all_roots(db, &task, &items, &plan, false)
				.await
				.map_err(|error| QueryError::Internal(error.to_string()))?;
			plan = apply_preflight_blockers(plan, &report);
		}
		Ok(plan)
	}
}

crate::register_library_query!(OrganizeCommitPlanQuery, "organize.commit_plan");
