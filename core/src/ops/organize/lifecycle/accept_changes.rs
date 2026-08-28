use crate::{
	context::CoreContext,
	infra::action::{error::ActionError, LibraryAction},
	ops::organize::repository::{
		OrganizeAcceptChangesInput as RepoInput, OrganizeAcceptChangesOutcome as RepoOutcome,
		OrganizeRepository,
	},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeAcceptChangesInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
	pub include_addition_ids: Vec<Uuid>,
	pub remove_missing_ids: Vec<Uuid>,
	pub refresh_changed_ids: Vec<Uuid>,
	pub preserve_changed_decisions: bool,
	pub confirm_inherited_destructive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeAcceptChangesOutcome {
	Applied {
		revision: i64,
	},
	ConfirmationRequired {
		discard_units: u64,
		move_units: u64,
		affected_bytes: u64,
		conflicting_roots: Vec<Uuid>,
	},
	StaleRevision {
		current_revision: i64,
	},
}

#[derive(Debug, Clone)]
pub struct OrganizeAcceptChangesAction {
	input: OrganizeAcceptChangesInput,
}

impl LibraryAction for OrganizeAcceptChangesAction {
	type Input = OrganizeAcceptChangesInput;
	type Output = OrganizeAcceptChangesOutcome;
	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(Self { input })
	}
	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let input = RepoInput {
			task_id: self.input.task_id,
			expected_revision: self.input.expected_revision,
			include_addition_ids: self.input.include_addition_ids,
			remove_missing_ids: self.input.remove_missing_ids,
			refresh_changed_ids: self.input.refresh_changed_ids,
			preserve_changed_decisions: self.input.preserve_changed_decisions,
			confirm_inherited_destructive: self.input.confirm_inherited_destructive,
		};
		match OrganizeRepository::new(library.db().conn())
			.accept_changes(input)
			.await
			.map_err(|e| ActionError::Database(e.to_string()))?
		{
			RepoOutcome::Applied { revision } => {
				Ok(OrganizeAcceptChangesOutcome::Applied { revision })
			}
			RepoOutcome::ConfirmationRequired {
				discard_units,
				move_units,
				affected_bytes,
				conflicting_roots,
			} => Ok(OrganizeAcceptChangesOutcome::ConfirmationRequired {
				discard_units,
				move_units,
				affected_bytes,
				conflicting_roots,
			}),
			RepoOutcome::StaleRevision { current_revision } => {
				Ok(OrganizeAcceptChangesOutcome::StaleRevision { current_revision })
			}
		}
	}
	fn action_kind(&self) -> &'static str {
		"organize.accept_changes"
	}
}

crate::register_library_action!(OrganizeAcceptChangesAction, "organize.accept_changes");
