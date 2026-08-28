use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
	context::CoreContext,
	domain::addressing::SdPath,
	infra::action::{error::ActionError, LibraryAction},
	ops::organize::{
		model::{DecisionValue, OrganizeDecisionConflictKind, OrganizeTaskStatus},
		repository::{
			DecisionTransactionRequest, OrganizeRepository, OrganizeRepositoryError,
			OrganizeSelectionInput, OrganizeTaskSummary,
		},
		OrganizeError,
	},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum OrganizeDecisionInput {
	Keep,
	Discard,
	Move { destination: SdPath },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeSetDecisionInput {
	pub task_id: Uuid,
	pub selection: OrganizeSelectionInput,
	pub decision: Option<OrganizeDecisionInput>,
	pub expected_revision: i64,
	pub confirm_descendant_override: bool,
	pub confirm_ancestor_split: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeDecisionOutcome {
	Applied {
		revision: i64,
		task_summary: OrganizeTaskSummary,
		affected_roots: Vec<Uuid>,
	},
	ConfirmationRequired {
		conflict_kind: OrganizeDecisionConflictKind,
		keep_units: u64,
		discard_units: u64,
		move_units: u64,
		unmarked_units: u64,
		affected_bytes: u64,
		conflicting_roots: Vec<Uuid>,
	},
	StaleRevision {
		current_revision: i64,
	},
	InheritedNoOp {
		revision: i64,
		ancestor_item_id: Uuid,
	},
	RejectedImmutable {
		applied_root_item_id: Uuid,
	},
	RejectedState {
		status: OrganizeTaskStatus,
	},
}

#[derive(Debug, Clone)]
pub struct OrganizeSetDecisionAction {
	input: OrganizeSetDecisionInput,
}

impl OrganizeSetDecisionAction {
	pub fn new(input: OrganizeSetDecisionInput) -> Self {
		Self { input }
	}
}

impl LibraryAction for OrganizeSetDecisionAction {
	type Input = OrganizeSetDecisionInput;
	type Output = OrganizeDecisionOutcome;

	fn from_input(input: Self::Input) -> Result<Self, String> {
		Ok(Self::new(input))
	}

	async fn execute(
		self,
		library: Arc<crate::library::Library>,
		_context: Arc<CoreContext>,
	) -> Result<Self::Output, ActionError> {
		let task_id = self.input.task_id;
		let expected_revision = self.input.expected_revision;
		let request = DecisionTransactionRequest {
			task_id,
			selection: self.input.selection,
			decision: decision_value(self.input.decision)?,
			expected_revision,
			confirm_descendant_override: self.input.confirm_descendant_override,
			confirm_ancestor_split: self.input.confirm_ancestor_split,
		};
		let repository = OrganizeRepository::new(library.db().conn());
		match repository.apply_decision(request).await {
			Ok(crate::ops::organize::repository::OrganizeDecisionOutcome::Applied {
				revision,
				affected_roots,
			}) => {
				let task_summary = repository
					.get_task(task_id)
					.await
					.map_err(|error| ActionError::Database(error.to_string()))?
					.task;
				Ok(OrganizeDecisionOutcome::Applied {
					revision,
					task_summary,
					affected_roots,
				})
			}
			Ok(
				crate::ops::organize::repository::OrganizeDecisionOutcome::ConfirmationRequired {
					conflict_kind,
					keep_units,
					discard_units,
					move_units,
					unmarked_units,
					affected_bytes,
					conflicting_roots,
				},
			) => Ok(OrganizeDecisionOutcome::ConfirmationRequired {
				conflict_kind,
				keep_units,
				discard_units,
				move_units,
				unmarked_units,
				affected_bytes,
				conflicting_roots,
			}),
			Ok(crate::ops::organize::repository::OrganizeDecisionOutcome::StaleRevision {
				current_revision,
			}) => Ok(OrganizeDecisionOutcome::StaleRevision { current_revision }),
			Ok(crate::ops::organize::repository::OrganizeDecisionOutcome::InheritedNoOp {
				ancestor_item_id,
			}) => Ok(OrganizeDecisionOutcome::InheritedNoOp {
				revision: expected_revision,
				ancestor_item_id,
			}),
			Err(OrganizeRepositoryError::Organize(OrganizeError::AppliedDecisionImmutable(
				applied_root_item_id,
			))) => Ok(OrganizeDecisionOutcome::RejectedImmutable {
				applied_root_item_id,
			}),
			Err(OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(state))) => {
				if let Some(status) = parse_task_status(&state) {
					Ok(OrganizeDecisionOutcome::RejectedState { status })
				} else {
					Err(ActionError::Database(format!(
						"invalid organize task state: {state}"
					)))
				}
			}
			Err(error) => Err(ActionError::Database(error.to_string())),
		}
	}

	fn action_kind(&self) -> &'static str {
		"organize.set_decision"
	}
}

fn decision_value(
	decision: Option<OrganizeDecisionInput>,
) -> Result<Option<DecisionValue>, ActionError> {
	decision
		.map(|decision| match decision {
			OrganizeDecisionInput::Keep => Ok(DecisionValue::keep()),
			OrganizeDecisionInput::Discard => Ok(DecisionValue::discard()),
			OrganizeDecisionInput::Move { destination } => {
				let (_, path) = destination.as_physical().ok_or_else(|| {
					ActionError::InvalidInput(
						"organize move destination must be a physical path".into(),
					)
				})?;
				if path.as_os_str().is_empty() {
					return Err(ActionError::InvalidInput(
						"organize move destination cannot be empty".into(),
					));
				}
				Ok(DecisionValue::move_to(path.to_string_lossy().into_owned()))
			}
		})
		.transpose()
}

fn parse_task_status(value: &str) -> Option<OrganizeTaskStatus> {
	match value {
		"scanning" => Some(OrganizeTaskStatus::Scanning),
		"active" => Some(OrganizeTaskStatus::Active),
		"committing" => Some(OrganizeTaskStatus::Committing),
		"completed" => Some(OrganizeTaskStatus::Completed),
		"failed" => Some(OrganizeTaskStatus::Failed),
		_ => None,
	}
}

crate::register_library_action!(OrganizeSetDecisionAction, "organize.set_decision");

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn move_input_requires_a_physical_destination() {
		let rejected = decision_value(Some(OrganizeDecisionInput::Move {
			destination: SdPath::content(Uuid::new_v4()),
		}));
		assert!(matches!(rejected, Err(ActionError::InvalidInput(_))));
	}

	#[test]
	fn wire_outcomes_keep_externally_tagged_confirmation_fields() {
		let outcome = OrganizeDecisionOutcome::ConfirmationRequired {
			conflict_kind: OrganizeDecisionConflictKind::DescendantOverride,
			keep_units: 2,
			discard_units: 1,
			move_units: 3,
			unmarked_units: 4,
			affected_bytes: 700,
			conflicting_roots: vec![Uuid::nil()],
		};
		let value = serde_json::to_value(outcome).expect("serialize decision outcome");
		assert_eq!(
			value["ConfirmationRequired"]["unmarked_units"],
			serde_json::json!(4)
		);
	}
}
