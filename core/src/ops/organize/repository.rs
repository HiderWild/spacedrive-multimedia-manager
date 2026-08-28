//! Transactional persistence for the two local organize-task tables.

use chrono::{DateTime, Utc};
use sea_orm::{
	entity::prelude::*, ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection,
	DatabaseTransaction, DbErr, EntityTrait, NotSet, PaginatorTrait, QueryFilter, QueryOrder, Set,
	TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::addressing::SdPath;
use crate::infra::db::entities::{organize_task, organize_task_item};
use crate::infra::job::types::JobId;
use crate::ops::organize::error::OrganizeError;
use crate::ops::organize::model::{
	DecisionResolution, DecisionTreeState, DecisionValue, ExplicitDecisionRoot,
	OrganizeDecisionConflictKind, OrganizeItemKind, OrganizeOperationState,
	OrganizeProgressSummary, OrganizeTaskStatus, TreeItemComputed,
};
use crate::ops::organize::path::paths_overlap;
use crate::ops::organize::tree::{
	compact_operation_roots, normalize_selection, reduce_progress, resolve_set_decision,
};

/// Errors returned by the organize persistence boundary.
#[derive(Debug, Error)]
pub enum OrganizeRepositoryError {
	#[error(transparent)]
	Organize(#[from] OrganizeError),
	#[error(transparent)]
	Database(#[from] DbErr),
}

static TASK_INSERT_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn is_sqlite_busy(error: &DbErr) -> bool {
	let message = error.to_string().to_ascii_lowercase();
	message.contains("database is locked") || message.contains("database is busy")
}

fn map_overlap_error(error: DbErr) -> OrganizeRepositoryError {
	if is_sqlite_busy(&error) {
		OrganizeRepositoryError::Organize(OrganizeError::UnsafeTopology(
			"another organize task is being created for an overlapping root".into(),
		))
	} else {
		OrganizeRepositoryError::Database(error)
	}
}

async fn task_in_state(
	txn: &DatabaseTransaction,
	task_id: Uuid,
	allowed: &[OrganizeTaskStatus],
) -> Result<organize_task::Model> {
	let task = organize_task::Entity::find_by_id(task_id)
		.one(txn)
		.await?
		.ok_or_else(|| OrganizeError::InvalidTaskState("organize task does not exist".into()))?;
	if !allowed
		.iter()
		.copied()
		.any(|status| task.status == task_status(status))
	{
		return Err(OrganizeError::InvalidTaskState(task.status).into());
	}
	Ok(task)
}

async fn find_overlapping_active_on<C: ConnectionTrait>(
	connection: &C,
	root_path_key: &str,
) -> Result<Option<Uuid>> {
	let candidates = organize_task::Entity::find()
		.filter(organize_task::Column::Status.is_in([
			task_status(OrganizeTaskStatus::Scanning),
			task_status(OrganizeTaskStatus::Active),
			task_status(OrganizeTaskStatus::Committing),
		]))
		.all(connection)
		.await?;
	Ok(candidates
		.into_iter()
		.find(|candidate| paths_overlap(&candidate.root_path_key, root_path_key))
		.map(|candidate| candidate.id))
}

async fn ensure_no_decisions(txn: &DatabaseTransaction, task_id: Uuid) -> Result<()> {
	let has_decision = organize_task_item::Entity::find()
		.filter(organize_task_item::Column::TaskId.eq(task_id))
		.filter(organize_task_item::Column::DecisionKind.is_not_null())
		.one(txn)
		.await?
		.is_some();
	if has_decision {
		return Err(OrganizeError::InvalidTaskState(
			"organize snapshot already has decisions".into(),
		)
		.into());
	}
	Ok(())
}

async fn ensure_no_applied_items(
	txn: &DatabaseTransaction,
	task_id: Uuid,
	item_ids: &[Uuid],
) -> Result<()> {
	for item_id in item_ids {
		if let Some(item) = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.filter(organize_task_item::Column::Uuid.eq(*item_id))
			.one(txn)
			.await?
		{
			if item.operation_state == operation_state(OrganizeOperationState::Applied) {
				return Err(OrganizeError::AppliedDecisionImmutable(item.uuid).into());
			}
		}
	}
	Ok(())
}

async fn mark_external_state(
	txn: &DatabaseTransaction,
	task_id: Uuid,
	item_ids: &[Uuid],
	state: &str,
) -> Result<()> {
	let items = organize_task_item::Entity::find()
		.filter(organize_task_item::Column::TaskId.eq(task_id))
		.all(txn)
		.await?;
	for item_id in item_ids {
		if let Some(item) = items.iter().find(|item| item.uuid == *item_id) {
			if !is_covered_by_applied_destructive_root(&items, item) {
				let mut active: organize_task_item::ActiveModel = item.clone().into();
				active.external_state = Set(state.to_string());
				active.updated_at = Set(Utc::now());
				active.update(txn).await?;
			}
		}
	}
	Ok(())
}

async fn validate_accept_ids(
	txn: &DatabaseTransaction,
	task_id: Uuid,
	item_ids: &[Uuid],
	membership_state: &str,
	external_state: &str,
) -> Result<()> {
	for item_id in item_ids.iter().copied().collect::<HashSet<_>>() {
		let item = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.filter(organize_task_item::Column::Uuid.eq(item_id))
			.one(txn)
			.await?
			.ok_or_else(|| OrganizeError::InvalidTaskState("accept item is not in task".into()))?;
		if item.operation_state == operation_state(OrganizeOperationState::Applied) {
			return Err(OrganizeError::AppliedDecisionImmutable(item.uuid).into());
		}
		if item.membership_state != membership_state || item.external_state != external_state {
			return Err(OrganizeError::InvalidTaskState(format!(
				"accept item {} must be {membership_state}/{external_state}",
				item.uuid
			))
			.into());
		}
	}
	Ok(())
}

fn selected_items(
	items: &[organize_task_item::Model],
	item_ids: &[Uuid],
) -> Vec<organize_task_item::Model> {
	items
		.iter()
		.filter(|item| item_ids.contains(&item.uuid))
		.cloned()
		.collect()
}

fn explicit_ancestor<'a>(
	items: &'a [organize_task_item::Model],
	item: &organize_task_item::Model,
) -> Option<&'a organize_task_item::Model> {
	let by_id = items
		.iter()
		.map(|candidate| (candidate.id, candidate))
		.collect::<HashMap<_, _>>();
	let mut parent_id = item.parent_id;
	while let Some(id) = parent_id {
		let candidate = by_id.get(&id).copied()?;
		if candidate.membership_state == "included" && candidate.decision_kind.is_some() {
			return Some(candidate);
		}
		parent_id = candidate.parent_id;
	}
	None
}

fn effective_decision<'a>(
	items: &'a [organize_task_item::Model],
	item: &organize_task_item::Model,
) -> Option<&'a organize_task_item::Model> {
	if item.membership_state == "included" && item.decision_kind.is_some() {
		items.iter().find(|candidate| candidate.uuid == item.uuid)
	} else {
		explicit_ancestor(items, item)
	}
}

fn is_covered_by_applied_destructive_root(
	items: &[organize_task_item::Model],
	item: &organize_task_item::Model,
) -> bool {
	let by_id = items
		.iter()
		.map(|candidate| (candidate.id, candidate))
		.collect::<HashMap<_, _>>();
	let mut current = Some(item);
	while let Some(candidate) = current {
		if candidate.operation_state == operation_state(OrganizeOperationState::Applied)
			&& matches!(candidate.decision_kind.as_deref(), Some("discard" | "move"))
		{
			return true;
		}
		current = candidate
			.parent_id
			.and_then(|parent_id| by_id.get(&parent_id).copied());
	}
	false
}

fn applied_descendant_of(items: &[organize_task_item::Model], root_ids: &[Uuid]) -> Option<Uuid> {
	let by_id = items
		.iter()
		.map(|candidate| (candidate.id, candidate))
		.collect::<HashMap<_, _>>();
	items.iter().find_map(|candidate| {
		if candidate.operation_state != operation_state(OrganizeOperationState::Applied) {
			return None;
		}
		let mut current = Some(candidate);
		while let Some(item) = current {
			if root_ids.contains(&item.uuid) {
				return Some(candidate.uuid);
			}
			current = item
				.parent_id
				.and_then(|parent_id| by_id.get(&parent_id).copied());
		}
		None
	})
}

async fn rebuild_included_tree(
	txn: &DatabaseTransaction,
	task_id: Uuid,
) -> Result<(i64, i64, i64)> {
	let items = organize_task_item::Entity::find()
		.filter(organize_task_item::Column::TaskId.eq(task_id))
		.filter(organize_task_item::Column::MembershipState.eq("included"))
		.all(txn)
		.await?;
	let by_id = items
		.iter()
		.map(|item| (item.id, item))
		.collect::<std::collections::HashMap<_, _>>();
	let mut children = std::collections::HashMap::<i32, Vec<i32>>::new();
	let mut roots = Vec::new();
	for item in &items {
		if let Some(parent_id) = item.parent_id {
			if !by_id.contains_key(&parent_id) {
				return Err(OrganizeError::InvalidTree(
					"included item refers to a missing parent".into(),
				)
				.into());
			}
			children.entry(parent_id).or_default().push(item.id);
		} else {
			roots.push(item.id);
		}
	}
	if !items.is_empty() && roots.len() != 1 {
		return Err(OrganizeError::InvalidTree(
			"included tree must contain exactly one root".into(),
		)
		.into());
	}
	if items.iter().any(|item| {
		item.kind != "directory"
			&& children
				.get(&item.id)
				.is_some_and(|child_ids| !child_ids.is_empty())
	}) {
		return Err(
			OrganizeError::InvalidTree("only directories may contain children".into()).into(),
		);
	}
	for child_ids in children.values_mut() {
		child_ids.sort_by_key(|id| {
			items
				.iter()
				.find(|item| item.id == *id)
				.map(|item| item.relative_path_key.as_str())
				.unwrap_or("")
		});
	}
	roots.sort_by_key(|id| {
		items
			.iter()
			.find(|item| item.id == *id)
			.map(|item| item.relative_path_key.as_str())
			.unwrap_or("")
	});
	let mut counter = 0_i64;
	let mut updates = Vec::new();
	let total_units = if let Some(root_id) = roots.first().copied() {
		let (units, _) = visit_tree(root_id, &children, &by_id, &mut counter, &mut updates)?;
		units
	} else {
		0
	};
	if updates.len() != items.len() {
		return Err(OrganizeError::InvalidTree(
			"included tree contains a cycle or unreachable item".into(),
		)
		.into());
	}
	let mut temporary_tree_start = items
		.iter()
		.filter_map(|item| item.tree_end)
		.max()
		.unwrap_or(0)
		.saturating_add(1);
	for item in &items {
		let current = organize_task_item::Entity::find_by_id(item.id)
			.one(txn)
			.await?
			.ok_or_else(|| OrganizeError::InvalidTree("tree item disappeared".into()))?;
		let mut active: organize_task_item::ActiveModel = current.into();
		active.tree_start = Set(Some(temporary_tree_start));
		active.tree_end = Set(Some(temporary_tree_start.saturating_add(1)));
		active.unit_count = Set(Some(1));
		active.updated_at = Set(Utc::now());
		active.update(txn).await?;
		temporary_tree_start = temporary_tree_start.saturating_add(1);
	}
	for &(id, tree_start, tree_end, unit_count, aggregate_size_bytes) in &updates {
		let item = organize_task_item::Entity::find_by_id(id)
			.one(txn)
			.await?
			.ok_or_else(|| OrganizeError::InvalidTree("tree item disappeared".into()))?;
		let mut active: organize_task_item::ActiveModel = item.into();
		active.tree_start = Set(Some(tree_start));
		active.tree_end = Set(Some(tree_end));
		active.unit_count = Set(Some(unit_count));
		active.aggregate_size_bytes = Set(aggregate_size_bytes);
		active.updated_at = Set(Utc::now());
		active.update(txn).await?;
	}
	let total_entries = items.len() as i64;
	let total_bytes = items
		.iter()
		.filter(|item| item.kind == "file")
		.map(|item| item.size_bytes)
		.sum();
	Ok((total_entries, total_units, total_bytes))
}

fn visit_tree(
	id: i32,
	children: &std::collections::HashMap<i32, Vec<i32>>,
	by_id: &std::collections::HashMap<i32, &organize_task_item::Model>,
	counter: &mut i64,
	updates: &mut Vec<(i32, i64, i64, i64, i64)>,
) -> Result<(i64, i64)> {
	let item = by_id
		.get(&id)
		.ok_or_else(|| OrganizeError::InvalidTree("tree item disappeared".into()))?;
	let start = *counter;
	*counter += 1;
	let mut child_units = 0_i64;
	let mut child_bytes = 0_i64;
	for child_id in children.get(&id).into_iter().flatten() {
		let (units, bytes) = visit_tree(*child_id, children, by_id, counter, updates)?;
		child_units += units;
		child_bytes += bytes;
	}
	let end = *counter;
	let has_children = children
		.get(&id)
		.is_some_and(|children| !children.is_empty());
	let units = if has_children { child_units } else { 1 };
	let bytes = if has_children {
		child_bytes
	} else {
		item.size_bytes
	};
	updates.push((id, start, end, units, bytes));
	Ok((units, bytes))
}

pub type Result<T> = std::result::Result<T, OrganizeRepositoryError>;

/// Values needed to create the task header before its snapshot is available.
#[derive(Debug, Clone)]
pub struct NewOrganizeTask {
	pub id: Uuid,
	pub name: String,
	pub root_path: String,
	pub root_path_key: String,
	pub device_slug: String,
	pub volume_id: Option<i32>,
	pub root_entry_uuid: Option<Uuid>,
	pub status: OrganizeTaskStatus,
	pub revision: i64,
	pub snapshot_version: i32,
	pub total_entries: i64,
	pub total_units: i64,
	pub total_bytes: i64,
	pub scan_issue_count: i64,
	pub pending_addition_count: i64,
	pub scan_job_id: Option<JobId>,
	pub commit_job_id: Option<JobId>,
	pub last_error: Option<String>,
	pub completed_at: Option<DateTime<Utc>>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

fn task_active_model(draft: NewOrganizeTask) -> organize_task::ActiveModel {
	organize_task::ActiveModel {
		id: Set(draft.id),
		name: Set(draft.name),
		root_path: Set(draft.root_path),
		root_path_key: Set(draft.root_path_key),
		device_slug: Set(draft.device_slug),
		volume_id: Set(draft.volume_id),
		root_entry_uuid: Set(draft.root_entry_uuid),
		status: Set(task_status(draft.status)),
		revision: Set(draft.revision),
		snapshot_version: Set(draft.snapshot_version),
		total_entries: Set(draft.total_entries),
		total_units: Set(draft.total_units),
		total_bytes: Set(draft.total_bytes),
		scan_issue_count: Set(draft.scan_issue_count),
		pending_addition_count: Set(draft.pending_addition_count),
		scan_job_id: Set(draft.scan_job_id.map(Into::into)),
		commit_job_id: Set(draft.commit_job_id.map(Into::into)),
		last_error: Set(draft.last_error),
		created_at: Set(draft.created_at),
		updated_at: Set(draft.updated_at),
		completed_at: Set(draft.completed_at),
	}
}

/// One manifest row supplied by a snapshot or change-scan job.
#[derive(Debug, Clone)]
pub struct SnapshotItemDraft {
	pub id: Option<i32>,
	pub uuid: Uuid,
	pub task_id: Uuid,
	pub parent_id: Option<i32>,
	pub entry_uuid: Option<Uuid>,
	pub relative_path: String,
	pub relative_path_key: String,
	pub name: String,
	pub extension: Option<String>,
	pub kind: OrganizeItemKind,
	pub size_bytes: i64,
	pub aggregate_size_bytes: i64,
	pub modified_at_100ns: i64,
	pub metadata_signature: String,
	pub tree_start: Option<i64>,
	pub tree_end: Option<i64>,
	pub unit_count: Option<i64>,
	pub membership_state: String,
	pub external_state: String,
	pub decision_kind: Option<DecisionValue>,
	pub move_destination: Option<String>,
	pub operation_state: OrganizeOperationState,
	pub last_error: Option<String>,
	pub applied_at: Option<DateTime<Utc>>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotTotals {
	pub total_entries: i64,
	pub total_units: i64,
	pub total_bytes: i64,
	pub scan_issue_count: i64,
}

#[derive(Debug, Clone)]
pub enum OrganizeSelectionInput {
	Items {
		item_ids: Vec<Uuid>,
	},
	DirectChildren {
		parent_item_id: Uuid,
		filter: SelectionFilter,
		excluded_item_ids: Vec<Uuid>,
	},
}

pub struct DecisionTransactionRequest {
	pub task_id: Uuid,
	pub selection: OrganizeSelectionInput,
	pub decision: Option<DecisionValue>,
	pub expected_revision: i64,
	pub confirm_descendant_override: bool,
	pub confirm_ancestor_split: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizeDecisionOutcome {
	Applied {
		revision: i64,
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
		ancestor_item_id: Uuid,
	},
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRecord {
	pub item_id: Uuid,
	pub decision_kind: Option<String>,
	pub move_destination: Option<String>,
	pub operation_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskSummary {
	pub id: Uuid,
	pub name: String,
	pub root_path: String,
	pub root_sd_path: SdPath,
	pub status: OrganizeTaskStatus,
	pub revision: i64,
	pub snapshot_version: i32,
	pub total_entries: u64,
	pub total_bytes: u64,
	pub progress: OrganizeProgressSummary,
	pub scan_issue_count: u64,
	pub pending_addition_count: u64,
	pub failed_operation_count: u64,
	pub changed_count: u64,
	pub missing_count: u64,
	pub scan_job_id: Option<JobId>,
	pub commit_job_id: Option<JobId>,
	pub last_error: Option<String>,
	pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct OrganizeListInput {
	pub statuses: Option<Vec<OrganizeTaskStatus>>,
	pub cursor: Option<String>,
	pub limit: u32,
}

#[derive(Debug, Clone)]
pub struct OrganizeListOutput {
	pub tasks: Vec<OrganizeTaskSummary>,
	pub next_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OrganizeGetInput {
	pub task_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct OrganizeGetOutput {
	pub task: OrganizeTaskSummary,
	pub root_item_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganizeItemFilter {
	All,
	Unmarked,
	Keep,
	Discard,
	Move,
	Failed,
	Changed,
	Missing,
}

pub type SelectionFilter = OrganizeItemFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganizeItemSort {
	Name,
	Modified,
	Size,
	Progress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganizeSortDirection {
	Asc,
	Desc,
}

#[derive(Debug, Clone)]
pub struct OrganizeChildrenInput {
	pub task_id: Uuid,
	pub parent_item_id: Uuid,
	pub cursor: Option<String>,
	pub limit: u32,
	pub sort: OrganizeItemSort,
	pub direction: OrganizeSortDirection,
	pub filter: OrganizeItemFilter,
}

#[derive(Debug, Clone)]
pub struct OrganizeChildrenOutput {
	pub revision: i64,
	pub items: Vec<organize_task_item::Model>,
	pub next_cursor: Option<String>,
	pub matching_child_count: u64,
}

#[derive(Debug, Clone)]
pub struct ChangeScanResult {
	pub additions: Vec<SnapshotItemDraft>,
	pub changed_ids: Vec<Uuid>,
	pub missing_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct OrganizeAcceptChangesInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
	pub include_addition_ids: Vec<Uuid>,
	pub remove_missing_ids: Vec<Uuid>,
	pub refresh_changed_ids: Vec<Uuid>,
	pub preserve_changed_decisions: bool,
	pub confirm_inherited_destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
pub struct OperationSettlement {
	pub item_id: Uuid,
	pub state: OrganizeOperationState,
	pub last_error: Option<String>,
	pub applied_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct OrganizeFinishInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
	pub confirm_unmarked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizeLifecycleOutcome {
	Applied { revision: i64 },
	StaleRevision { current_revision: i64 },
}

/// Provides all SQL access used by organize task operations.
pub struct OrganizeRepository<'db> {
	db: &'db DatabaseConnection,
}

impl<'db> OrganizeRepository<'db> {
	pub fn new(db: &'db DatabaseConnection) -> Self {
		Self { db }
	}

	pub async fn insert_scanning_task(
		&self,
		draft: NewOrganizeTask,
	) -> Result<organize_task::Model> {
		let _insert_guard = TASK_INSERT_LOCK
			.get_or_init(|| tokio::sync::Mutex::new(()))
			.lock()
			.await;
		let txn = self.db.begin().await.map_err(map_overlap_error)?;
		let existing = find_overlapping_active_on(&txn, &draft.root_path_key)
			.await
			.map_err(|error| match error {
				OrganizeRepositoryError::Database(error) => map_overlap_error(error),
				other => other,
			})?;
		if let Some(existing_id) = existing {
			txn.rollback().await?;
			return Err(OrganizeError::UnsafeTopology(format!(
				"organize task root overlaps active task {existing_id}"
			))
			.into());
		}
		let model = match task_active_model(draft).insert(&txn).await {
			Ok(model) => model,
			Err(error) if is_sqlite_busy(&error) => {
				let _ = txn.rollback().await;
				return Err(map_overlap_error(error).into());
			}
			Err(error) => {
				let _ = txn.rollback().await;
				return Err(error.into());
			}
		};
		txn.commit().await?;
		Ok(model)
	}

	pub async fn find_overlapping_active(&self, root_path_key: &str) -> Result<Option<Uuid>> {
		find_overlapping_active_on(self.db, root_path_key).await
	}

	pub async fn replace_included_snapshot(
		&self,
		task_id: Uuid,
		items: Vec<SnapshotItemDraft>,
		totals: SnapshotTotals,
	) -> Result<i64> {
		let txn = self.db.begin().await?;
		let task = task_in_state(&txn, task_id, &[OrganizeTaskStatus::Scanning]).await?;
		ensure_no_decisions(&txn, task_id).await?;
		organize_task_item::Entity::delete_many()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.exec(&txn)
			.await?;

		for draft in items {
			item_active_model(draft).insert(&txn).await?;
		}

		let revision = task.revision + 1;
		let mut active: organize_task::ActiveModel = task.into();
		active.total_entries = Set(totals.total_entries);
		active.total_units = Set(totals.total_units);
		active.total_bytes = Set(totals.total_bytes);
		active.scan_issue_count = Set(totals.scan_issue_count);
		active.pending_addition_count = Set(0);
		active.status = Set(task_status(OrganizeTaskStatus::Active));
		active.revision = Set(revision);
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(revision)
	}

	pub async fn fail_snapshot(&self, task_id: Uuid, message: String) -> Result<()> {
		let txn = self.db.begin().await?;
		let task = task_in_state(&txn, task_id, &[OrganizeTaskStatus::Scanning]).await?;
		ensure_no_decisions(&txn, task_id).await?;
		let mut active: organize_task::ActiveModel = task.into();
		active.status = Set(task_status(OrganizeTaskStatus::Failed));
		active.last_error = Set(Some(message));
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(())
	}

	pub async fn get_task_revision(&self, task_id: Uuid) -> Result<i64> {
		Ok(organize_task::Entity::find_by_id(task_id)
			.one(self.db)
			.await?
			.ok_or_else(|| OrganizeError::InvalidTaskState("organize task does not exist".into()))?
			.revision)
	}

	pub async fn explicit_decision_ids(&self, task_id: Uuid) -> Result<Vec<Uuid>> {
		Ok(organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.filter(organize_task_item::Column::DecisionKind.is_not_null())
			.order_by_asc(organize_task_item::Column::TreeStart)
			.all(self.db)
			.await?
			.into_iter()
			.map(|item| item.uuid)
			.collect())
	}

	pub async fn dump_decisions(&self, task_id: Uuid) -> Result<Vec<DecisionRecord>> {
		Ok(organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.filter(organize_task_item::Column::DecisionKind.is_not_null())
			.order_by_asc(organize_task_item::Column::TreeStart)
			.all(self.db)
			.await?
			.into_iter()
			.map(|item| DecisionRecord {
				item_id: item.uuid,
				decision_kind: item.decision_kind,
				move_destination: item.move_destination,
				operation_state: item.operation_state,
			})
			.collect())
	}

	pub async fn list_tasks(&self, input: OrganizeListInput) -> Result<OrganizeListOutput> {
		let status_keys = input.statuses.as_ref().map(|statuses| {
			let mut keys = statuses
				.iter()
				.map(|status| task_status(*status))
				.collect::<Vec<_>>();
			keys.sort();
			keys.dedup();
			keys
		});
		let cursor = input.cursor.as_deref().map(parse_task_cursor).transpose()?;
		if let Some(cursor) = &cursor {
			if cursor.statuses != status_keys {
				return Err(
					OrganizeError::InvalidTree("task cursor filter mismatch".into()).into(),
				);
			}
		}
		let mut query = organize_task::Entity::find();
		if let Some(statuses) = &status_keys {
			if statuses.is_empty() {
				return Ok(OrganizeListOutput {
					tasks: Vec::new(),
					next_cursor: None,
				});
			}
			query = query.filter(organize_task::Column::Status.is_in(statuses.clone()));
		}
		let mut rows = query
			.order_by_desc(organize_task::Column::UpdatedAt)
			.order_by_desc(organize_task::Column::Id)
			.all(self.db)
			.await?;
		if let Some(cursor) = cursor {
			rows.retain(|row| (row.updated_at, row.id) < (cursor.updated_at, cursor.task_id));
		}
		let limit = input.limit.clamp(1, 100) as usize;
		let has_next = rows.len() > limit;
		rows.truncate(limit);
		let next_cursor = has_next.then(|| {
			let row = rows.last().expect("non-empty page has a next cursor");
			encode_task_cursor(&TaskCursor {
				statuses: status_keys.clone(),
				updated_at: row.updated_at,
				task_id: row.id,
			})
		});
		let mut tasks = Vec::with_capacity(rows.len());
		for row in rows {
			tasks.push(task_summary(self.db, row).await?);
		}
		Ok(OrganizeListOutput { tasks, next_cursor })
	}

	pub async fn get_task(&self, task_id: Uuid) -> Result<OrganizeGetOutput> {
		let task = organize_task::Entity::find_by_id(task_id)
			.one(self.db)
			.await?
			.ok_or_else(|| {
				OrganizeError::InvalidTaskState("organize task does not exist".into())
			})?;
		let root_item_id = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.filter(organize_task_item::Column::ParentId.is_null())
			.filter(organize_task_item::Column::MembershipState.eq("included"))
			.order_by_asc(organize_task_item::Column::TreeStart)
			.one(self.db)
			.await?
			.ok_or_else(|| OrganizeError::InvalidTree("organize task has no root item".into()))?
			.uuid;
		Ok(OrganizeGetOutput {
			task: task_summary(self.db, task).await?,
			root_item_id,
		})
	}

	pub async fn children(&self, input: OrganizeChildrenInput) -> Result<OrganizeChildrenOutput> {
		let revision = self.get_task_revision(input.task_id).await?;
		let cursor = input
			.cursor
			.as_deref()
			.map(parse_child_cursor)
			.transpose()?;
		if let Some(cursor) = &cursor {
			if cursor.revision != revision
				|| cursor.parent_item_id != input.parent_item_id
				|| cursor.filter != input.filter
				|| cursor.sort != input.sort
				|| cursor.direction != input.direction
			{
				return Err(
					OrganizeError::InvalidTree("child cursor does not match query".into()).into(),
				);
			}
		}
		let parent = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.filter(organize_task_item::Column::Uuid.eq(input.parent_item_id))
			.one(self.db)
			.await?
			.ok_or_else(|| OrganizeError::InvalidTree("children parent disappeared".into()))?;
		let all_items = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.all(self.db)
			.await?;
		let children = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.filter(organize_task_item::Column::ParentId.eq(parent.id))
			.all(self.db)
			.await?
			.into_iter()
			.filter(|item| selection_filter_matches(item, &all_items, input.filter))
			.collect::<Vec<_>>();
		let matching_child_count = children.len() as u64;
		let limit = input.limit.clamp(1, 200) as usize;
		let mut children = children;
		children.sort_by(|left, right| {
			let ordering = child_sort_cmp(left, right, input.sort);
			match input.direction {
				OrganizeSortDirection::Asc => ordering,
				OrganizeSortDirection::Desc => ordering.reverse(),
			}
		});
		let start_index = cursor
			.as_ref()
			.map(|cursor| {
				children
					.iter()
					.position(|item| match input.direction {
						OrganizeSortDirection::Asc => {
							child_cursor_cmp(item, cursor, input.sort).is_gt()
						}
						OrganizeSortDirection::Desc => {
							child_cursor_cmp(item, cursor, input.sort).is_lt()
						}
					})
					.unwrap_or(children.len())
			})
			.unwrap_or(0);
		let end_index = (start_index + limit).min(children.len());
		let page = children[start_index..end_index].to_vec();
		let next_cursor = (end_index < children.len()).then(|| {
			let item = &page[page.len() - 1];
			encode_child_cursor(&ChildCursor {
				revision,
				parent_item_id: input.parent_item_id,
				filter: input.filter,
				sort: input.sort,
				direction: input.direction,
				name: item.name.clone(),
				modified_at_100ns: item.modified_at_100ns,
				size_bytes: item.size_bytes,
				progress: item.unit_count.unwrap_or(0),
				item_id: item.uuid,
			})
		});
		Ok(OrganizeChildrenOutput {
			revision,
			items: page,
			next_cursor,
			matching_child_count,
		})
	}

	pub async fn resolve_selection(
		&self,
		task_id: Uuid,
		expected_revision: i64,
		selection: OrganizeSelectionInput,
	) -> Result<Vec<organize_task_item::Model>> {
		let current_revision = self.get_task_revision(task_id).await?;
		if current_revision != expected_revision {
			return Err(OrganizeError::StaleRevision(current_revision).into());
		}
		let ids = selection_ids(self.db, task_id, selection).await?;
		Ok(organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.filter(organize_task_item::Column::Uuid.is_in(ids))
			.all(self.db)
			.await?)
	}

	pub async fn store_change_scan(&self, task_id: Uuid, result: ChangeScanResult) -> Result<i64> {
		let txn = self.db.begin().await?;
		let task = task_in_state(&txn, task_id, &[OrganizeTaskStatus::Active]).await?;
		for draft in &result.additions {
			if draft.task_id != task_id {
				txn.rollback().await?;
				return Err(OrganizeError::InvalidTaskState(
					"change-scan addition belongs to another task".into(),
				)
				.into());
			}
			if draft.operation_state == OrganizeOperationState::Applied {
				txn.rollback().await?;
				return Err(OrganizeError::AppliedDecisionImmutable(draft.uuid).into());
			}
		}
		for draft in result.additions {
			let mut draft = draft;
			draft.membership_state = "pending_addition".to_string();
			draft.external_state = "present".to_string();
			draft.decision_kind = None;
			draft.move_destination = None;
			draft.operation_state = OrganizeOperationState::None;
			draft.last_error = None;
			draft.applied_at = None;
			draft.tree_start = None;
			draft.tree_end = None;
			draft.unit_count = None;
			item_active_model(draft).insert(&txn).await?;
		}
		mark_external_state(&txn, task_id, &result.changed_ids, "changed").await?;
		mark_external_state(&txn, task_id, &result.missing_ids, "missing").await?;
		let pending_addition_count = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.filter(organize_task_item::Column::MembershipState.eq("pending_addition"))
			.count(&txn)
			.await? as i64;
		let revision = task.revision + 1;
		let mut active: organize_task::ActiveModel = task.into();
		active.pending_addition_count = Set(pending_addition_count);
		active.revision = Set(revision);
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(revision)
	}

	pub async fn accept_changes(
		&self,
		input: OrganizeAcceptChangesInput,
	) -> Result<OrganizeAcceptChangesOutcome> {
		let txn = self.db.begin().await?;
		let task = task_in_state(&txn, input.task_id, &[OrganizeTaskStatus::Active]).await?;
		if task.revision != input.expected_revision {
			txn.rollback().await?;
			return Ok(OrganizeAcceptChangesOutcome::StaleRevision {
				current_revision: task.revision,
			});
		}
		validate_accept_ids(
			&txn,
			input.task_id,
			&input.include_addition_ids,
			"pending_addition",
			"present",
		)
		.await?;
		validate_accept_ids(
			&txn,
			input.task_id,
			&input.remove_missing_ids,
			"included",
			"missing",
		)
		.await?;
		validate_accept_ids(
			&txn,
			input.task_id,
			&input.refresh_changed_ids,
			"included",
			"changed",
		)
		.await?;
		let all_items = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.all(&txn)
			.await?;
		let additions = selected_items(&all_items, &input.include_addition_ids);
		let mut inherited_conflicts = Vec::new();
		let mut discard_units = 0_u64;
		let mut move_units = 0_u64;
		let mut affected_bytes = 0_u64;
		for addition in &additions {
			if let Some(ancestor) = explicit_ancestor(&all_items, addition) {
				if matches!(ancestor.decision_kind.as_deref(), Some("discard" | "move")) {
					if !inherited_conflicts.contains(&ancestor.uuid) {
						inherited_conflicts.push(ancestor.uuid);
						affected_bytes = affected_bytes
							.saturating_add(ancestor.aggregate_size_bytes.max(0) as u64);
						match ancestor.decision_kind.as_deref() {
							Some("discard") => {
								discard_units = discard_units
									.saturating_add(ancestor.unit_count.unwrap_or(0).max(0) as u64);
							}
							Some("move") => {
								move_units = move_units
									.saturating_add(ancestor.unit_count.unwrap_or(0).max(0) as u64);
							}
							_ => {}
						}
					}
				}
			}
		}
		if !inherited_conflicts.is_empty() && !input.confirm_inherited_destructive {
			txn.rollback().await?;
			return Ok(OrganizeAcceptChangesOutcome::ConfirmationRequired {
				discard_units,
				move_units,
				affected_bytes,
				conflicting_roots: inherited_conflicts,
			});
		}
		if let Some(applied_id) = applied_descendant_of(&all_items, &input.remove_missing_ids) {
			txn.rollback().await?;
			return Err(OrganizeError::AppliedDecisionImmutable(applied_id).into());
		}

		let mut temporary_tree_start = all_items
			.iter()
			.filter(|item| item.membership_state == "included")
			.filter_map(|item| item.tree_end)
			.max()
			.unwrap_or(0);
		for item in additions {
			// Pending additions cannot be made included with NULL tree fields because
			// the membership CHECK is immediate. Temporary disjoint intervals keep the
			// row valid until the complete tree is rebuilt below.
			let placeholder_start = temporary_tree_start;
			temporary_tree_start += 1;
			let mut active: organize_task_item::ActiveModel = item.into();
			active.membership_state = Set("included".to_string());
			active.external_state = Set("present".to_string());
			active.tree_start = Set(Some(placeholder_start));
			active.tree_end = Set(Some(placeholder_start + 1));
			active.unit_count = Set(Some(1));
			active.updated_at = Set(Utc::now());
			active.update(&txn).await?;
		}
		if !input.remove_missing_ids.is_empty() {
			organize_task_item::Entity::delete_many()
				.filter(organize_task_item::Column::TaskId.eq(input.task_id))
				.filter(organize_task_item::Column::Uuid.is_in(input.remove_missing_ids))
				.exec(&txn)
				.await?;
		}
		for item_id in input.refresh_changed_ids {
			if let Some(item) = organize_task_item::Entity::find()
				.filter(organize_task_item::Column::TaskId.eq(input.task_id))
				.filter(organize_task_item::Column::Uuid.eq(item_id))
				.one(&txn)
				.await?
			{
				let mut active: organize_task_item::ActiveModel = item.into();
				active.external_state = Set("present".to_string());
				if !input.preserve_changed_decisions {
					active.decision_kind = Set(None);
					active.move_destination = Set(None);
					active.operation_state = Set(operation_state(OrganizeOperationState::None));
					active.last_error = Set(None);
					active.applied_at = Set(None);
				}
				active.updated_at = Set(Utc::now());
				active.update(&txn).await?;
			}
		}

		let (total_entries, total_units, total_bytes) =
			rebuild_included_tree(&txn, input.task_id).await?;
		let pending_addition_count = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.filter(organize_task_item::Column::MembershipState.eq("pending_addition"))
			.count(&txn)
			.await? as i64;
		let revision = task.revision + 1;
		let mut active: organize_task::ActiveModel = task.into();
		active.total_entries = Set(total_entries);
		active.total_units = Set(total_units);
		active.total_bytes = Set(total_bytes);
		active.pending_addition_count = Set(pending_addition_count);
		active.revision = Set(revision);
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(OrganizeAcceptChangesOutcome::Applied { revision })
	}

	pub async fn lock_for_commit(
		&self,
		task_id: Uuid,
		expected_revision: i64,
		job_id: JobId,
	) -> Result<i64> {
		let txn = self.db.begin().await?;
		let task = task_in_state(&txn, task_id, &[OrganizeTaskStatus::Active]).await?;
		if task.revision != expected_revision {
			txn.rollback().await?;
			return Err(OrganizeError::StaleRevision(task.revision).into());
		}
		let revision = task.revision + 1;
		let mut active: organize_task::ActiveModel = task.into();
		active.status = Set(task_status(OrganizeTaskStatus::Committing));
		active.commit_job_id = Set(Some(job_id.into()));
		active.revision = Set(revision);
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(revision)
	}

	pub async fn settle_operation_roots(
		&self,
		task_id: Uuid,
		settlements: Vec<OperationSettlement>,
	) -> Result<i64> {
		let txn = self.db.begin().await?;
		let task = task_in_state(&txn, task_id, &[OrganizeTaskStatus::Committing]).await?;
		for settlement in settlements {
			let item = organize_task_item::Entity::find()
				.filter(organize_task_item::Column::TaskId.eq(task_id))
				.filter(organize_task_item::Column::Uuid.eq(settlement.item_id))
				.one(&txn)
				.await?
				.ok_or_else(|| OrganizeError::InvalidTree("operation root disappeared".into()))?;
			if item.operation_state == operation_state(OrganizeOperationState::Applied) {
				txn.rollback().await?;
				return Err(OrganizeError::AppliedDecisionImmutable(item.uuid).into());
			}
			let mut active: organize_task_item::ActiveModel = item.into();
			active.operation_state = Set(operation_state(settlement.state));
			active.last_error = Set(settlement.last_error);
			active.applied_at = Set(settlement.applied_at);
			active.updated_at = Set(Utc::now());
			active.update(&txn).await?;
		}
		let revision = task.revision + 1;
		let mut active: organize_task::ActiveModel = task.into();
		active.status = Set(task_status(OrganizeTaskStatus::Active));
		active.revision = Set(revision);
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(revision)
	}

	pub async fn apply_decision(
		&self,
		request: DecisionTransactionRequest,
	) -> Result<OrganizeDecisionOutcome> {
		let txn = self.db.begin().await?;
		let task = organize_task::Entity::find_by_id(request.task_id)
			.one(&txn)
			.await?
			.ok_or_else(|| {
				OrganizeError::InvalidTaskState("organize task does not exist".into())
			})?;
		if task.revision != request.expected_revision {
			txn.rollback().await?;
			return Ok(OrganizeDecisionOutcome::StaleRevision {
				current_revision: task.revision,
			});
		}
		if task.status != task_status(OrganizeTaskStatus::Active) {
			txn.rollback().await?;
			return Err(OrganizeError::InvalidTaskState(task.status).into());
		}

		let selected_ids = selection_ids(&txn, request.task_id, request.selection).await?;
		if selected_ids.is_empty() {
			txn.rollback().await?;
			return Err(OrganizeError::InvalidTree("selection is empty".into()).into());
		}
		let all_items = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(request.task_id))
			.filter(organize_task_item::Column::MembershipState.eq("included"))
			.all(&txn)
			.await?;
		let selected_unique = selected_ids.iter().copied().collect::<HashSet<_>>();
		let selected = all_items
			.iter()
			.filter(|item| selected_unique.contains(&item.uuid))
			.collect::<Vec<_>>();
		if selected.len() != selected_unique.len() {
			txn.rollback().await?;
			return Err(
				OrganizeError::InvalidTree("selection contains an unknown item".into()).into(),
			);
		}
		ensure_no_applied_items(&txn, request.task_id, &selected_ids).await?;

		let state = decision_tree_state(&all_items)?;
		let intervals = state
			.nodes
			.iter()
			.map(|node| (node.item_id, (node.tree_start, node.tree_end)))
			.collect::<HashMap<_, _>>();
		let normalized_ids = normalize_selection(&selected_ids, &intervals)?;
		let resolution = resolve_set_decision(
			&state,
			&selected_ids,
			request.decision.clone(),
			request.confirm_descendant_override,
			request.confirm_ancestor_split,
		)?;
		match resolution {
			DecisionResolution::ConfirmationRequired {
				conflict_kind,
				keep_units,
				discard_units,
				move_units,
				unmarked_units,
				affected_bytes,
				conflicting_roots,
			} => {
				txn.rollback().await?;
				return Ok(OrganizeDecisionOutcome::ConfirmationRequired {
					conflict_kind,
					keep_units,
					discard_units,
					move_units,
					unmarked_units,
					affected_bytes,
					conflicting_roots,
				});
			}
			DecisionResolution::InheritedNoOp { ancestor_item_id } => {
				txn.rollback().await?;
				return Ok(OrganizeDecisionOutcome::InheritedNoOp { ancestor_item_id });
			}
			DecisionResolution::Apply(patch) => {
				if decision_patch_is_noop(&all_items, &patch) {
					txn.rollback().await?;
					return Ok(OrganizeDecisionOutcome::InheritedNoOp {
						ancestor_item_id: normalized_ids[0],
					});
				}
				for item_id in &patch.delete_roots {
					if let Some(item) = all_items.iter().find(|item| item.uuid == *item_id) {
						let mut active: organize_task_item::ActiveModel = item.clone().into();
						active.decision_kind = Set(None);
						active.move_destination = Set(None);
						active.operation_state = Set(operation_state(OrganizeOperationState::None));
						active.last_error = Set(None);
						active.applied_at = Set(None);
						active.updated_at = Set(Utc::now());
						active.update(&txn).await?;
					}
				}
				for root in &patch.upsert_roots {
					let item = all_items
						.iter()
						.find(|item| item.uuid == root.item_id)
						.ok_or_else(|| {
							OrganizeError::InvalidTree("selected node disappeared".into())
						})?;
					let (decision_kind, move_destination, state) =
						decision_columns(Some(&root.decision));
					let mut active: organize_task_item::ActiveModel = item.clone().into();
					active.decision_kind = Set(decision_kind);
					active.move_destination = Set(move_destination);
					active.operation_state = Set(state);
					active.last_error = Set(None);
					active.applied_at = Set(None);
					active.updated_at = Set(Utc::now());
					active.update(&txn).await?;
				}
				let revision = task.revision + 1;
				let mut active: organize_task::ActiveModel = task.into();
				active.revision = Set(revision);
				active.updated_at = Set(Utc::now());
				active.update(&txn).await?;
				txn.commit().await?;
				return Ok(OrganizeDecisionOutcome::Applied {
					revision,
					affected_roots: normalized_ids,
				});
			}
		}
	}

	pub async fn set_completed(&self, task_id: Uuid) -> Result<()> {
		let task = organize_task::Entity::find_by_id(task_id)
			.one(self.db)
			.await?
			.ok_or_else(|| {
				OrganizeError::InvalidTaskState("organize task does not exist".into())
			})?;
		let mut active: organize_task::ActiveModel = task.into();
		active.status = Set(task_status(OrganizeTaskStatus::Completed));
		active.completed_at = Set(Some(Utc::now()));
		active.updated_at = Set(Utc::now());
		active.update(self.db).await?;
		Ok(())
	}

	pub async fn finish(&self, input: OrganizeFinishInput) -> Result<OrganizeFinishOutcome> {
		let txn = self.db.begin().await?;
		let task = task_in_state(&txn, input.task_id, &[OrganizeTaskStatus::Active]).await?;
		if task.revision != input.expected_revision {
			txn.rollback().await?;
			return Ok(OrganizeFinishOutcome::StaleRevision {
				current_revision: task.revision,
			});
		}
		let items = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.filter(organize_task_item::Column::MembershipState.eq("included"))
			.all(&txn)
			.await?;
		let decision_state = decision_tree_state(&items)?;
		let progress = reduce_progress(&decision_state.nodes, &decision_state.decisions)?;
		let unmarked_units = progress.unmarked_units;
		if unmarked_units > 0 && !input.confirm_unmarked {
			txn.rollback().await?;
			return Ok(OrganizeFinishOutcome::ConfirmationRequired { unmarked_units });
		}
		let operation_roots = compact_operation_roots(&decision_state.decisions);
		let pending = operation_roots
			.iter()
			.filter(|root| root.operation_state == OrganizeOperationState::Pending)
			.count() as u64;
		let running = operation_roots
			.iter()
			.filter(|root| root.operation_state == OrganizeOperationState::Running)
			.count() as u64;
		let failed = operation_roots
			.iter()
			.filter(|root| root.operation_state == OrganizeOperationState::Failed)
			.count() as u64;
		if pending > 0 || running > 0 || failed > 0 {
			txn.rollback().await?;
			return Ok(OrganizeFinishOutcome::RejectedPendingOperations {
				pending,
				running,
				failed,
			});
		}
		let revision = task.revision + 1;
		let mut active: organize_task::ActiveModel = task.into();
		active.status = Set(task_status(OrganizeTaskStatus::Completed));
		active.completed_at = Set(Some(Utc::now()));
		active.revision = Set(revision);
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(OrganizeFinishOutcome::Completed { revision })
	}

	pub async fn reopen(
		&self,
		task_id: Uuid,
		expected_revision: i64,
	) -> Result<OrganizeLifecycleOutcome> {
		let txn = self.db.begin().await?;
		let task = organize_task::Entity::find_by_id(task_id)
			.one(&txn)
			.await?
			.ok_or_else(|| {
				OrganizeError::InvalidTaskState("organize task does not exist".into())
			})?;
		if task.revision != expected_revision {
			txn.rollback().await?;
			return Ok(OrganizeLifecycleOutcome::StaleRevision {
				current_revision: task.revision,
			});
		}
		if task.status != task_status(OrganizeTaskStatus::Completed) {
			txn.rollback().await?;
			return Err(OrganizeError::InvalidTaskState(task.status).into());
		}
		let revision = task.revision + 1;
		let mut active: organize_task::ActiveModel = task.into();
		active.status = Set(task_status(OrganizeTaskStatus::Active));
		active.completed_at = Set(None);
		active.revision = Set(revision);
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(OrganizeLifecycleOutcome::Applied { revision })
	}

	pub async fn delete_task_metadata(
		&self,
		task_id: Uuid,
		expected_revision: i64,
	) -> Result<OrganizeLifecycleOutcome> {
		let txn = self.db.begin().await?;
		let task = organize_task::Entity::find_by_id(task_id)
			.one(&txn)
			.await?
			.ok_or_else(|| {
				OrganizeError::InvalidTaskState("organize task does not exist".into())
			})?;
		if task.revision != expected_revision {
			txn.rollback().await?;
			return Ok(OrganizeLifecycleOutcome::StaleRevision {
				current_revision: task.revision,
			});
		}
		if task.status == task_status(OrganizeTaskStatus::Committing) {
			txn.rollback().await?;
			return Err(OrganizeError::InvalidTaskState("task is committing".into()).into());
		}
		organize_task::Entity::delete_by_id(task_id)
			.exec(&txn)
			.await?;
		txn.commit().await?;
		Ok(OrganizeLifecycleOutcome::Applied {
			revision: task.revision,
		})
	}
}

async fn selection_ids<C: sea_orm::ConnectionTrait>(
	txn: &C,
	task_id: Uuid,
	selection: OrganizeSelectionInput,
) -> Result<Vec<Uuid>> {
	match selection {
		OrganizeSelectionInput::Items { item_ids } => Ok(item_ids),
		OrganizeSelectionInput::DirectChildren {
			parent_item_id,
			filter,
			excluded_item_ids,
		} => {
			let parent = organize_task_item::Entity::find()
				.filter(organize_task_item::Column::TaskId.eq(task_id))
				.filter(organize_task_item::Column::Uuid.eq(parent_item_id))
				.one(txn)
				.await?
				.ok_or_else(|| OrganizeError::InvalidTree("selection parent disappeared".into()))?;
			let all_items = organize_task_item::Entity::find()
				.filter(organize_task_item::Column::TaskId.eq(task_id))
				.all(txn)
				.await?;
			let children = organize_task_item::Entity::find()
				.filter(organize_task_item::Column::TaskId.eq(task_id))
				.filter(organize_task_item::Column::ParentId.eq(parent.id))
				.order_by_asc(organize_task_item::Column::Name)
				.all(txn)
				.await?;
			Ok(children
				.into_iter()
				.filter(|item| selection_filter_matches(item, &all_items, filter))
				.filter(|item| !excluded_item_ids.contains(&item.uuid))
				.map(|item| item.uuid)
				.collect())
		}
	}
}

fn selection_filter_matches(
	item: &organize_task_item::Model,
	all_items: &[organize_task_item::Model],
	filter: SelectionFilter,
) -> bool {
	let effective_decision =
		effective_decision(all_items, item).and_then(|ancestor| ancestor.decision_kind.as_deref());
	match filter {
		SelectionFilter::All => true,
		SelectionFilter::Unmarked => effective_decision.is_none(),
		SelectionFilter::Keep => effective_decision == Some("keep"),
		SelectionFilter::Discard => effective_decision == Some("discard"),
		SelectionFilter::Move => effective_decision == Some("move"),
		SelectionFilter::Failed => item.operation_state == "failed",
		SelectionFilter::Changed => item.external_state == "changed",
		SelectionFilter::Missing => item.external_state == "missing",
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskCursor {
	statuses: Option<Vec<String>>,
	updated_at: DateTime<Utc>,
	task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChildCursor {
	revision: i64,
	parent_item_id: Uuid,
	filter: OrganizeItemFilter,
	sort: OrganizeItemSort,
	direction: OrganizeSortDirection,
	name: String,
	modified_at_100ns: i64,
	size_bytes: i64,
	progress: i64,
	item_id: Uuid,
}

const URL_SAFE_BASE64: &[u8; 64] =
	b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode_opaque<T: Serialize>(value: &T) -> String {
	let json = serde_json::to_vec(value).expect("organize cursor serialization");
	let mut encoded = String::with_capacity(json.len().div_ceil(3) * 4);
	for chunk in json.chunks(3) {
		let first = chunk[0] as u32;
		let second = chunk.get(1).copied().unwrap_or(0) as u32;
		let third = chunk.get(2).copied().unwrap_or(0) as u32;
		let value = (first << 16) | (second << 8) | third;
		encoded.push(URL_SAFE_BASE64[((value >> 18) & 0x3f) as usize] as char);
		encoded.push(URL_SAFE_BASE64[((value >> 12) & 0x3f) as usize] as char);
		if chunk.len() > 1 {
			encoded.push(URL_SAFE_BASE64[((value >> 6) & 0x3f) as usize] as char);
		}
		if chunk.len() > 2 {
			encoded.push(URL_SAFE_BASE64[(value & 0x3f) as usize] as char);
		}
	}
	encoded
}

fn decode_opaque(cursor: &str) -> Result<Vec<u8>> {
	if cursor.is_empty() || cursor.len() % 4 == 1 {
		return Err(OrganizeError::InvalidTree("invalid opaque cursor".into()).into());
	}
	let mut decoded = Vec::with_capacity(cursor.len() * 3 / 4);
	let mut value = 0_u32;
	let mut bits = 0_u8;
	for byte in cursor.bytes() {
		let digit = URL_SAFE_BASE64
			.iter()
			.position(|candidate| *candidate == byte)
			.ok_or_else(|| OrganizeError::InvalidTree("invalid opaque cursor".into()))?
			as u32;
		value = (value << 6) | digit;
		bits += 6;
		if bits >= 8 {
			bits -= 8;
			decoded.push((value >> bits) as u8);
			value &= (1 << bits) - 1;
		}
	}
	Ok(decoded)
}

fn parse_task_cursor(cursor: &str) -> Result<TaskCursor> {
	serde_json::from_slice(&decode_opaque(cursor)?).map_err(|_| {
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTree("invalid task cursor".into()))
	})
}

fn encode_task_cursor(cursor: &TaskCursor) -> String {
	encode_opaque(cursor)
}

fn parse_child_cursor(cursor: &str) -> Result<ChildCursor> {
	serde_json::from_slice(&decode_opaque(cursor)?).map_err(|_| {
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTree("invalid child cursor".into()))
	})
}

fn encode_child_cursor(cursor: &ChildCursor) -> String {
	encode_opaque(cursor)
}

fn child_sort_cmp(
	left: &organize_task_item::Model,
	right: &organize_task_item::Model,
	sort: OrganizeItemSort,
) -> std::cmp::Ordering {
	let ordering = match sort {
		OrganizeItemSort::Name => left.name.cmp(&right.name),
		OrganizeItemSort::Modified => left.modified_at_100ns.cmp(&right.modified_at_100ns),
		OrganizeItemSort::Size => left.size_bytes.cmp(&right.size_bytes),
		OrganizeItemSort::Progress => left
			.unit_count
			.unwrap_or(0)
			.cmp(&right.unit_count.unwrap_or(0)),
	};
	ordering.then_with(|| left.uuid.cmp(&right.uuid))
}

fn child_cursor_cmp(
	item: &organize_task_item::Model,
	cursor: &ChildCursor,
	sort: OrganizeItemSort,
) -> std::cmp::Ordering {
	let ordering = match sort {
		OrganizeItemSort::Name => item.name.cmp(&cursor.name),
		OrganizeItemSort::Modified => item.modified_at_100ns.cmp(&cursor.modified_at_100ns),
		OrganizeItemSort::Size => item.size_bytes.cmp(&cursor.size_bytes),
		OrganizeItemSort::Progress => item.unit_count.unwrap_or(0).cmp(&cursor.progress),
	};
	ordering.then_with(|| item.uuid.cmp(&cursor.item_id))
}

async fn task_summary(
	db: &DatabaseConnection,
	task: organize_task::Model,
) -> Result<OrganizeTaskSummary> {
	let items = organize_task_item::Entity::find()
		.filter(organize_task_item::Column::TaskId.eq(task.id))
		.all(db)
		.await?;
	let included = items
		.iter()
		.filter(|item| item.membership_state == "included")
		.cloned()
		.collect::<Vec<_>>();
	let state = decision_tree_state(&included)?;
	let progress = reduce_progress(&state.nodes, &state.decisions)?;
	let failed_operation_count = compact_operation_roots(&state.decisions)
		.into_iter()
		.filter(|root| root.operation_state == OrganizeOperationState::Failed)
		.count() as u64;
	let total_bytes = included
		.iter()
		.filter(|item| item.kind == "file")
		.map(|item| item.size_bytes.max(0) as u64)
		.sum();
	Ok(OrganizeTaskSummary {
		id: task.id,
		name: task.name,
		root_path: task.root_path.clone(),
		root_sd_path: SdPath::physical(task.device_slug, task.root_path),
		status: parse_task_status(&task.status)?,
		revision: task.revision,
		snapshot_version: task.snapshot_version,
		total_entries: included.len() as u64,
		total_bytes,
		progress,
		scan_issue_count: task.scan_issue_count.max(0) as u64,
		pending_addition_count: task.pending_addition_count.max(0) as u64,
		failed_operation_count,
		changed_count: items
			.iter()
			.filter(|item| item.external_state == "changed")
			.count() as u64,
		missing_count: items
			.iter()
			.filter(|item| item.external_state == "missing")
			.count() as u64,
		scan_job_id: task.scan_job_id.map(JobId::from),
		commit_job_id: task.commit_job_id.map(JobId::from),
		last_error: task.last_error,
		completed_at: task.completed_at,
	})
}

fn parse_task_status(value: &str) -> Result<OrganizeTaskStatus> {
	match value {
		"scanning" => Ok(OrganizeTaskStatus::Scanning),
		"active" => Ok(OrganizeTaskStatus::Active),
		"committing" => Ok(OrganizeTaskStatus::Committing),
		"completed" => Ok(OrganizeTaskStatus::Completed),
		"failed" => Ok(OrganizeTaskStatus::Failed),
		status => {
			Err(OrganizeError::InvalidTaskState(format!("unknown task status: {status}")).into())
		}
	}
}

fn item_active_model(draft: SnapshotItemDraft) -> organize_task_item::ActiveModel {
	let (decision_kind, move_destination) = match draft.decision_kind.as_ref() {
		Some(DecisionValue::Keep) => (Some("keep".to_string()), None),
		Some(DecisionValue::Discard) => {
			(Some("discard".to_string()), draft.move_destination.clone())
		}
		Some(DecisionValue::Move { destination }) => {
			(Some("move".to_string()), Some(destination.clone()))
		}
		None => (None, draft.move_destination.clone()),
	};
	organize_task_item::ActiveModel {
		id: draft.id.map_or(NotSet, Set),
		uuid: Set(draft.uuid),
		task_id: Set(draft.task_id),
		parent_id: Set(draft.parent_id),
		entry_uuid: Set(draft.entry_uuid),
		relative_path: Set(draft.relative_path),
		relative_path_key: Set(draft.relative_path_key),
		name: Set(draft.name),
		extension: Set(draft.extension),
		kind: Set(item_kind(draft.kind)),
		size_bytes: Set(draft.size_bytes),
		aggregate_size_bytes: Set(draft.aggregate_size_bytes),
		modified_at_100ns: Set(draft.modified_at_100ns),
		metadata_signature: Set(draft.metadata_signature),
		tree_start: Set(draft.tree_start),
		tree_end: Set(draft.tree_end),
		unit_count: Set(draft.unit_count),
		membership_state: Set(draft.membership_state),
		external_state: Set(draft.external_state),
		decision_kind: Set(decision_kind),
		move_destination: Set(move_destination),
		operation_state: Set(operation_state(draft.operation_state)),
		last_error: Set(draft.last_error),
		applied_at: Set(draft.applied_at),
		created_at: Set(draft.created_at),
		updated_at: Set(draft.updated_at),
	}
}

fn decision_tree_state(items: &[organize_task_item::Model]) -> Result<DecisionTreeState> {
	let nodes = items
		.iter()
		.filter(|item| item.membership_state == "included")
		.map(|item| {
			Ok(TreeItemComputed {
				item_id: item.uuid,
				tree_start: item.tree_start.ok_or_else(|| {
					OrganizeError::InvalidTree("included item has no tree start".into())
				})?,
				tree_end: item.tree_end.ok_or_else(|| {
					OrganizeError::InvalidTree("included item has no tree end".into())
				})?,
				unit_count: item
					.unit_count
					.ok_or_else(|| {
						OrganizeError::InvalidTree("included item has no unit count".into())
					})?
					.try_into()
					.map_err(|_| {
						OrganizeError::InvalidTree("included item has invalid unit count".into())
					})?,
				aggregate_size_bytes: item.aggregate_size_bytes.max(0).try_into().map_err(
					|_| {
						OrganizeError::InvalidTree(
							"included item has invalid aggregate size".into(),
						)
					},
				)?,
			})
		})
		.collect::<Result<Vec<_>>>()?;
	let decisions = items
		.iter()
		.filter(|item| item.membership_state == "included" && item.decision_kind.is_some())
		.map(|item| {
			let decision = decision_value(item)?
				.ok_or_else(|| OrganizeError::InvalidTree("decision kind disappeared".into()))?;
			Ok(ExplicitDecisionRoot {
				item_id: item.uuid,
				tree_start: item.tree_start.ok_or_else(|| {
					OrganizeError::InvalidTree("decision has no tree start".into())
				})?,
				tree_end: item
					.tree_end
					.ok_or_else(|| OrganizeError::InvalidTree("decision has no tree end".into()))?,
				unit_count: item
					.unit_count
					.ok_or_else(|| OrganizeError::InvalidTree("decision has no unit count".into()))?
					.try_into()
					.map_err(|_| {
						OrganizeError::InvalidTree("decision has invalid unit count".into())
					})?,
				aggregate_size_bytes: item.aggregate_size_bytes.max(0).try_into().map_err(
					|_| OrganizeError::InvalidTree("decision has invalid aggregate size".into()),
				)?,
				decision,
				operation_state: parse_operation_state(&item.operation_state)?,
			})
		})
		.collect::<Result<Vec<_>>>()?;
	Ok(DecisionTreeState { nodes, decisions })
}

fn decision_value(item: &organize_task_item::Model) -> Result<Option<DecisionValue>> {
	match item.decision_kind.as_deref() {
		None => Ok(None),
		Some("keep") => Ok(Some(DecisionValue::keep())),
		Some("discard") => Ok(Some(DecisionValue::discard())),
		Some("move") => Ok(Some(DecisionValue::move_to(
			item.move_destination.clone().ok_or_else(|| {
				OrganizeError::InvalidTree("move decision has no destination".into())
			})?,
		))),
		Some(kind) => {
			Err(OrganizeError::InvalidTree(format!("unknown decision kind: {kind}")).into())
		}
	}
}

fn parse_operation_state(value: &str) -> Result<OrganizeOperationState> {
	match value {
		"none" => Ok(OrganizeOperationState::None),
		"pending" => Ok(OrganizeOperationState::Pending),
		"running" => Ok(OrganizeOperationState::Running),
		"applied" => Ok(OrganizeOperationState::Applied),
		"failed" => Ok(OrganizeOperationState::Failed),
		state => {
			Err(OrganizeError::InvalidTree(format!("unknown operation state: {state}")).into())
		}
	}
}

fn decision_patch_is_noop(
	items: &[organize_task_item::Model],
	patch: &crate::ops::organize::model::DecisionPatch,
) -> bool {
	let mut current = items
		.iter()
		.filter(|item| item.decision_kind.is_some())
		.map(|item| {
			(
				item.uuid,
				(
					item.decision_kind.clone(),
					item.move_destination.clone(),
					item.operation_state.clone(),
				),
			)
		})
		.collect::<HashMap<_, _>>();
	let original = current.clone();
	for item_id in &patch.delete_roots {
		current.remove(item_id);
	}
	for root in &patch.upsert_roots {
		let (decision_kind, move_destination, operation_state) =
			decision_columns(Some(&root.decision));
		current.insert(
			root.item_id,
			(decision_kind, move_destination, operation_state),
		);
	}
	current == original
}

fn decision_columns(decision: Option<&DecisionValue>) -> (Option<String>, Option<String>, String) {
	match decision {
		None => (None, None, operation_state(OrganizeOperationState::None)),
		Some(DecisionValue::Keep) => (
			Some("keep".to_string()),
			None,
			operation_state(OrganizeOperationState::None),
		),
		Some(DecisionValue::Discard) => (
			Some("discard".to_string()),
			None,
			operation_state(OrganizeOperationState::Pending),
		),
		Some(DecisionValue::Move { destination }) => (
			Some("move".to_string()),
			Some(normalize_move_destination(destination)),
			operation_state(OrganizeOperationState::Pending),
		),
	}
}

fn normalize_move_destination(destination: &str) -> String {
	let mut key = destination.replace('/', "\\").to_lowercase();
	if let Some(unc) = key.strip_prefix(r"\\?\unc\") {
		key = format!(r"\\{}", unc);
	} else if let Some(drive) = key.strip_prefix(r"\\?\") {
		if drive.as_bytes().get(1) == Some(&b':') {
			key = drive.to_string();
		}
	}

	let is_unc = key.starts_with(r"\\");
	let root_components = if is_unc { 2 } else { 1 };
	let mut components = Vec::new();
	for component in key.split('\\') {
		if component.is_empty() || component == "." {
			continue;
		}
		if component == ".." {
			if components.len() > root_components {
				components.pop();
			}
			continue;
		}
		components.push(component);
	}

	if is_unc {
		format!(r"\\{}", components.join("\\"))
	} else if components.len() == 1 && components[0].ends_with(':') {
		components[0].to_string()
	} else if let Some((drive, rest)) = components.split_first() {
		format!("{}\\{}", drive, rest.join("\\"))
	} else {
		String::new()
	}
}

fn normalize_windows_key(value: &str) -> String {
	let mut key = value.replace('/', "\\").to_lowercase();
	while key.ends_with('\\') && !is_windows_root(&key) {
		key.pop();
	}
	key
}

fn is_windows_root(value: &str) -> bool {
	(value.len() == 3 && value.as_bytes().get(1) == Some(&b':') && value.ends_with('\\'))
		|| value.starts_with("\\\\")
}

fn task_status(status: OrganizeTaskStatus) -> String {
	match status {
		OrganizeTaskStatus::Scanning => "scanning",
		OrganizeTaskStatus::Active => "active",
		OrganizeTaskStatus::Committing => "committing",
		OrganizeTaskStatus::Completed => "completed",
		OrganizeTaskStatus::Failed => "failed",
	}
	.to_string()
}

fn item_kind(kind: OrganizeItemKind) -> String {
	match kind {
		OrganizeItemKind::File => "file",
		OrganizeItemKind::Directory => "directory",
		OrganizeItemKind::ReparsePoint => "reparse_point",
		OrganizeItemKind::Unreadable => "unreadable",
	}
	.to_string()
}

fn operation_state(state: OrganizeOperationState) -> String {
	match state {
		OrganizeOperationState::None => "none",
		OrganizeOperationState::Pending => "pending",
		OrganizeOperationState::Running => "running",
		OrganizeOperationState::Applied => "applied",
		OrganizeOperationState::Failed => "failed",
	}
	.to_string()
}
