use crate::{
	context::CoreContext,
	infra::action::{error::ActionError, LibraryAction},
	ops::organize::repository::{
		OrganizeFinishInput as RepoInput, OrganizeFinishOutcome as RepoOutcome, OrganizeRepository,
	},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeFinishInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
	pub confirm_unmarked: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeFinishOutcome {
	Completed {
		revision: i64,
	},
	ConfirmationRequired {
		unmarked_units: u64,
	},
	StaleRevision {
		current_revision: i64,
	},
	RejectedPendingOperations {
		pending: u64,
		running: u64,
		failed: u64,
	},
}
#[derive(Debug, Clone)]
pub struct OrganizeFinishAction {
	input: OrganizeFinishInput,
}
impl LibraryAction for OrganizeFinishAction {
	type Input = OrganizeFinishInput;
	type Output = OrganizeFinishOutcome;
	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(Self { input })
	}
	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		match OrganizeRepository::new(library.db().conn())
			.finish(RepoInput {
				task_id: self.input.task_id,
				expected_revision: self.input.expected_revision,
				confirm_unmarked: self.input.confirm_unmarked,
			})
			.await
			.map_err(|e| ActionError::Database(e.to_string()))?
		{
			RepoOutcome::Completed { revision } => {
				Ok(OrganizeFinishOutcome::Completed { revision })
			}
			RepoOutcome::ConfirmationRequired { unmarked_units } => {
				Ok(OrganizeFinishOutcome::ConfirmationRequired { unmarked_units })
			}
			RepoOutcome::StaleRevision { current_revision } => {
				Ok(OrganizeFinishOutcome::StaleRevision { current_revision })
			}
			RepoOutcome::RejectedPendingOperations {
				pending,
				running,
				failed,
			} => Ok(OrganizeFinishOutcome::RejectedPendingOperations {
				pending,
				running,
				failed,
			}),
		}
	}
	fn action_kind(&self) -> &'static str {
		"organize.finish"
	}
}
crate::register_library_action!(OrganizeFinishAction, "organize.finish");
