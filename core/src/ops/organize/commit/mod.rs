use crate::domain::addressing::SdPath;
use crate::ops::organize::model::OrganizeTaskStatus;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

pub mod plan;

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
