use crate::domain::addressing::SdPath;
use crate::infra::job::handle::JobReceipt;
use crate::infra::job::prelude::*;
use crate::infra::job::types::JobId;
use crate::ops::files::copy::action::FileConflictResolution;
use crate::ops::organize::model::OrganizeTaskStatus;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

pub mod action;
pub mod job;
pub mod plan;
pub mod preflight;

pub use plan::build_commit_plan;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeCommitPlanInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizePlanRoot {
	pub item_id: Uuid,
	pub source: SdPath,
	pub units: u64,
	pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeMoveGroup {
	pub destination: SdPath,
	pub roots: Vec<OrganizePlanRoot>,
	pub units: u64,
	pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeTopologyConflict {
	pub item_id: Uuid,
	pub source: SdPath,
	pub destination: SdPath,
	pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeCommitBlockReason {
	TaskNotActive {
		status: OrganizeTaskStatus,
	},
	PendingAdditions {
		count: u64,
	},
	ChangedOrMissing {
		item_ids: Vec<Uuid>,
	},
	UnsafeTopology {
		conflicts: Vec<OrganizeTopologyConflict>,
	},
	NoPhysicalOperations,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeCommitPlanOutput {
	pub revision: i64,
	pub move_groups: Vec<OrganizeMoveGroup>,
	pub discard_roots: Vec<OrganizePlanRoot>,
	pub keep_units: u64,
	pub unmarked_units: u64,
	pub pending_addition_count: u64,
	pub changed_or_missing_roots: Vec<Uuid>,
	pub failed_operation_roots: Vec<Uuid>,
	pub unsafe_conflicts: Vec<OrganizeTopologyConflict>,
	pub can_commit: bool,
	pub blocking_reasons: Vec<OrganizeCommitBlockReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeCommitInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
	pub permanent_delete_confirmed: bool,
	pub move_conflict_policy: FileConflictResolution,
	#[serde(default)]
	pub allow_current_subtree_drift: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum OrganizeCommitOutcome {
	Started {
		job: JobReceipt,
	},
	StaleRevision {
		current_revision: i64,
	},
	RejectedState {
		status: OrganizeTaskStatus,
	},
	RejectedPermanentConfirmation,
	RejectedBlockedPlan {
		reasons: Vec<OrganizeCommitBlockReason>,
	},
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeCommitOutput {
	pub revision: i64,
	pub applied_root_ids: Vec<Uuid>,
	pub failed_root_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeCommitPhase {
	Preflight,
	MoveGroups,
	DeleteRoots,
	Reconcile,
	Settle,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeCommitCheckpoint {
	pub phase: OrganizeCommitPhase,
	pub next_move_group: usize,
	pub active_child_job_id: Option<JobId>,
	pub delete_dispatched: bool,
	pub completed_root_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Job)]
pub struct OrganizeCommitJob {
	pub task_id: Uuid,
	pub locked_revision: i64,
	pub plan: OrganizeCommitPlanOutput,
	pub move_conflict_policy: FileConflictResolution,
	pub allow_current_subtree_drift: bool,
	pub checkpoint: OrganizeCommitCheckpoint,
}
