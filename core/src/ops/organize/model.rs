use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeTaskStatus {
	Scanning,
	Active,
	Committing,
	Completed,
	Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeItemKind {
	File,
	Directory,
	ReparsePoint,
	Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeOperationState {
	None,
	Pending,
	Running,
	Applied,
	Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum DecisionValue {
	Keep,
	Discard,
	Move { destination: String },
}

impl PartialEq for DecisionValue {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Keep, Self::Keep) | (Self::Discard, Self::Discard) => true,
			(Self::Move { destination: left }, Self::Move { destination: right }) => {
				normalize_windows_destination(left) == normalize_windows_destination(right)
			}
			_ => false,
		}
	}
}

impl Eq for DecisionValue {}

fn normalize_windows_destination(destination: &str) -> String {
	let mut key = destination.replace('/', "\\").to_lowercase();
	if let Some(unc) = key.strip_prefix(r"\\?\unc\") {
		key = format!(r"\\{}", unc);
	} else if let Some(drive) = key.strip_prefix(r"\\?\") {
		if drive.as_bytes().get(1) == Some(&b':') {
			key = drive.to_string();
		}
	}
	let is_drive_root = key.len() == 2 && key.as_bytes().get(1) == Some(&b':');
	if !is_drive_root {
		key = key.trim_end_matches('\\').to_string();
	}
	key
}

impl DecisionValue {
	pub fn keep() -> Self {
		Self::Keep
	}

	pub fn discard() -> Self {
		Self::Discard
	}

	pub fn move_to(destination: impl Into<String>) -> Self {
		Self::Move {
			destination: destination.into(),
		}
	}

	pub(crate) fn priority(&self) -> u8 {
		match self {
			Self::Move { .. } => 0,
			Self::Keep => 1,
			Self::Discard => 2,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeItemDraft {
	pub item_id: Uuid,
	pub parent_item_id: Option<Uuid>,
	pub kind: OrganizeItemKind,
	pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeItemComputed {
	pub item_id: Uuid,
	pub tree_start: i64,
	pub tree_end: i64,
	pub unit_count: u64,
	pub aggregate_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitDecisionRoot {
	pub item_id: Uuid,
	pub tree_start: i64,
	pub tree_end: i64,
	pub unit_count: u64,
	pub aggregate_size_bytes: u64,
	pub decision: DecisionValue,
	pub operation_state: OrganizeOperationState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionTreeState {
	pub nodes: Vec<TreeItemComputed>,
	pub decisions: Vec<ExplicitDecisionRoot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionPatch {
	pub delete_roots: Vec<Uuid>,
	pub upsert_roots: Vec<ExplicitDecisionRoot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeDecisionConflictKind {
	DescendantOverride,
	AncestorSplit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionResolution {
	Apply(DecisionPatch),
	ConfirmationRequired {
		conflict_kind: OrganizeDecisionConflictKind,
		keep_units: u64,
		discard_units: u64,
		move_units: u64,
		unmarked_units: u64,
		affected_bytes: u64,
		conflicting_roots: Vec<Uuid>,
	},
	InheritedNoOp {
		ancestor_item_id: Uuid,
	},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
pub struct OrganizeProgressSummary {
	pub total_units: u64,
	pub processed_units: u64,
	pub keep_units: u64,
	pub discard_units: u64,
	pub move_units: u64,
	pub unmarked_units: u64,
}
