use crate::{
	context::CoreContext,
	infra::action::{error::ActionError, LibraryAction},
	ops::organize::repository::{OrganizeLifecycleOutcome as RepoOutcome, OrganizeRepository},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeReopenInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeReopenOutcome {
	Applied { revision: i64 },
	StaleRevision { current_revision: i64 },
}
#[derive(Debug, Clone)]
pub struct OrganizeReopenAction {
	input: OrganizeReopenInput,
}
impl LibraryAction for OrganizeReopenAction {
	type Input = OrganizeReopenInput;
	type Output = OrganizeReopenOutcome;
	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(Self { input })
	}
	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		match OrganizeRepository::new(library.db().conn())
			.reopen(self.input.task_id, self.input.expected_revision)
			.await
			.map_err(|e| ActionError::Database(e.to_string()))?
		{
			RepoOutcome::Applied { revision } => Ok(OrganizeReopenOutcome::Applied { revision }),
			RepoOutcome::StaleRevision { current_revision } => {
				Ok(OrganizeReopenOutcome::StaleRevision { current_revision })
			}
		}
	}
	fn action_kind(&self) -> &'static str {
		"organize.reopen"
	}
}
crate::register_library_action!(OrganizeReopenAction, "organize.reopen");
