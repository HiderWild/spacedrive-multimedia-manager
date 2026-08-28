//! Transactional persistence for the two local organize-task tables.

use chrono::{DateTime, Utc};
use sea_orm::{
	entity::prelude::*, ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection,
	DatabaseTransaction, DbErr, EntityTrait, NotSet, PaginatorTrait, QueryFilter, QueryOrder, Set,
	TransactionTrait,
};
use thiserror::Error;
use uuid::Uuid;

use crate::infra::db::entities::{organize_task, organize_task_item};
use crate::infra::job::types::JobId;
use crate::ops::organize::error::OrganizeError;
use crate::ops::organize::model::{
	DecisionValue, OrganizeDecisionConflictKind, OrganizeItemKind, OrganizeOperationState,
	OrganizeTaskStatus,
};
use crate::ops::organize::path::paths_overlap;

/// Errors returned by the organize persistence boundary.
#[derive(Debug, Error)]
pub enum OrganizeRepositoryError {
	#[error(transparent)]
	Organize(#[from] OrganizeError),
	#[error(transparent)]
	Database(#[from] DbErr),
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
	for item_id in item_ids {
		if let Some(item) = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(task_id))
			.filter(organize_task_item::Column::Uuid.eq(*item_id))
			.one(txn)
			.await?
		{
			let mut active: organize_task_item::ActiveModel = item.into();
			active.external_state = Set(state.to_string());
			active.updated_at = Set(Utc::now());
			active.update(txn).await?;
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
	let start = item.tree_start?;
	items
		.iter()
		.filter(|candidate| {
			candidate.uuid != item.uuid
				&& candidate.decision_kind.is_some()
				&& candidate.membership_state == "included"
				&& candidate
					.tree_start
					.is_some_and(|candidate_start| candidate_start < start)
				&& candidate
					.tree_end
					.is_some_and(|candidate_end| candidate_end >= start)
		})
		.max_by_key(|candidate| candidate.tree_start)
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
	let mut children = std::collections::HashMap::<i32, Vec<i32>>::new();
	let mut roots = Vec::new();
	for item in &items {
		if let Some(parent_id) = item.parent_id {
			children.entry(parent_id).or_default().push(item.id);
		} else {
			roots.push(item.id);
		}
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
	let by_id = items
		.iter()
		.map(|item| (item.id, item))
		.collect::<std::collections::HashMap<_, _>>();
	let mut counter = 0_i64;
	let mut updates = Vec::new();
	for root_id in roots {
		visit_tree(root_id, &children, &by_id, &mut counter, &mut updates)?;
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
	let total_units = updates
		.iter()
		.filter(|(_, start, _, _, _)| *start == 0)
		.map(|(_, _, _, units, _)| *units)
		.sum();
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
	let end = *counter - 1;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionFilter {
	All,
	Unmarked,
	Keep,
	Discard,
	Move,
	Failed,
	Changed,
	Missing,
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

#[derive(Debug, Clone)]
pub struct OrganizeChildrenInput {
	pub task_id: Uuid,
	pub parent_item_id: Uuid,
	pub cursor: Option<String>,
	pub limit: u32,
	pub filter: SelectionFilter,
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
		let txn = self.db.begin().await?;
		if let Some(existing_id) = find_overlapping_active_on(&txn, &draft.root_path_key).await? {
			txn.rollback().await?;
			return Err(OrganizeError::UnsafeTopology(format!(
				"organize task root overlaps active task {existing_id}"
			))
			.into());
		}
		let model = task_active_model(draft).insert(&txn).await?;
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

	pub async fn children(&self, input: OrganizeChildrenInput) -> Result<OrganizeChildrenOutput> {
		let revision = self.get_task_revision(input.task_id).await?;
		let parent = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.filter(organize_task_item::Column::Uuid.eq(input.parent_item_id))
			.one(self.db)
			.await?
			.ok_or_else(|| OrganizeError::InvalidTree("children parent disappeared".into()))?;
		let children = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.filter(organize_task_item::Column::ParentId.eq(parent.id))
			.all(self.db)
			.await?
			.into_iter()
			.filter(|item| selection_filter_matches(item, input.filter))
			.collect::<Vec<_>>();
		let matching_child_count = children.len() as u64;
		let limit = input.limit.clamp(1, 200) as usize;
		let start = input
			.cursor
			.as_deref()
			.map(parse_child_cursor)
			.transpose()?;
		let mut children = children;
		children.sort_by(|left, right| {
			left.name
				.cmp(&right.name)
				.then_with(|| left.uuid.cmp(&right.uuid))
		});
		let start_index = start
			.map(|(name, uuid)| {
				children
					.iter()
					.position(|item| (item.name.as_str(), item.uuid) > (name.as_str(), uuid))
					.unwrap_or(children.len())
			})
			.unwrap_or(0);
		let end_index = (start_index + limit).min(children.len());
		let page = children[start_index..end_index].to_vec();
		let next_cursor = (end_index < children.len()).then(|| {
			let item = &page[page.len() - 1];
			format!("{}|{}", item.name, item.uuid)
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
		for draft in result.additions {
			let mut draft = draft;
			draft.membership_state = "pending_addition".to_string();
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
		let all_items = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(input.task_id))
			.all(&txn)
			.await?;
		ensure_no_applied_items(&txn, input.task_id, &input.include_addition_ids).await?;
		ensure_no_applied_items(&txn, input.task_id, &input.remove_missing_ids).await?;
		ensure_no_applied_items(&txn, input.task_id, &input.refresh_changed_ids).await?;
		let additions = selected_items(&all_items, &input.include_addition_ids);
		let mut inherited_conflicts = Vec::new();
		for addition in &additions {
			if let Some(ancestor) = explicit_ancestor(&all_items, addition) {
				if matches!(ancestor.decision_kind.as_deref(), Some("discard" | "move")) {
					inherited_conflicts.push(ancestor.uuid);
				}
			}
		}
		if !inherited_conflicts.is_empty() && !input.confirm_inherited_destructive {
			txn.rollback().await?;
			return Ok(OrganizeAcceptChangesOutcome::ConfirmationRequired {
				discard_units: 0,
				move_units: 0,
				affected_bytes: additions.iter().map(|item| item.size_bytes as u64).sum(),
				conflicting_roots: inherited_conflicts,
			});
		}

		for item in additions {
			let mut active: organize_task_item::ActiveModel = item.into();
			active.membership_state = Set("included".to_string());
			active.external_state = Set("present".to_string());
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
		let selected = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(request.task_id))
			.filter(organize_task_item::Column::Uuid.is_in(selected_ids.clone()))
			.all(&txn)
			.await?;
		for item in &selected {
			if item.operation_state == operation_state(OrganizeOperationState::Applied) {
				txn.rollback().await?;
				return Err(OrganizeError::AppliedDecisionImmutable(item.uuid).into());
			}
		}

		let all_items = organize_task_item::Entity::find()
			.filter(organize_task_item::Column::TaskId.eq(request.task_id))
			.all(&txn)
			.await?;
		let requested_decision = request.decision.as_ref();
		let mut conflicting_roots = Vec::new();
		for root in &selected {
			if let Some(ancestor) = explicit_ancestor(&all_items, root) {
				if !selected_ids.contains(&ancestor.uuid) && !request.confirm_ancestor_split {
					txn.rollback().await?;
					return Ok(OrganizeDecisionOutcome::ConfirmationRequired {
						conflict_kind: OrganizeDecisionConflictKind::AncestorSplit,
						keep_units: 0,
						discard_units: 0,
						move_units: 0,
						unmarked_units: 0,
						affected_bytes: 0,
						conflicting_roots: vec![ancestor.uuid],
					});
				}
			}
			if let (Some(start), Some(end)) = (root.tree_start, root.tree_end) {
				let nested = all_items
					.iter()
					.filter(|candidate| {
						candidate.uuid != root.uuid
							&& candidate.decision_kind.is_some()
							&& candidate.membership_state == "included"
							&& candidate.tree_start.is_some_and(|candidate_start| {
								candidate_start >= start && candidate_start <= end
							})
					})
					.collect::<Vec<_>>();
				for candidate in &nested {
					if destructive_overwrite(requested_decision, candidate) {
						conflicting_roots.push(candidate.uuid);
					}
				}
				if !conflicting_roots.is_empty() && !request.confirm_descendant_override {
					txn.rollback().await?;
					return Ok(OrganizeDecisionOutcome::ConfirmationRequired {
						conflict_kind: OrganizeDecisionConflictKind::DescendantOverride,
						keep_units: 0,
						discard_units: 0,
						move_units: 0,
						unmarked_units: 0,
						affected_bytes: 0,
						conflicting_roots,
					});
				}
				for candidate in nested {
					let mut active: organize_task_item::ActiveModel = candidate.clone().into();
					active.decision_kind = Set(None);
					active.move_destination = Set(None);
					active.operation_state = Set(operation_state(OrganizeOperationState::None));
					active.updated_at = Set(Utc::now());
					active.update(&txn).await?;
				}
			}
		}

		for item in &selected {
			let (decision_kind, move_destination, state) =
				decision_columns(request.decision.as_ref());
			let mut active: organize_task_item::ActiveModel = item.clone().into();
			active.decision_kind = Set(decision_kind);
			active.move_destination = Set(move_destination);
			active.operation_state = Set(state);
			active.updated_at = Set(Utc::now());
			active.update(&txn).await?;
		}

		let revision = task.revision + 1;
		let mut active: organize_task::ActiveModel = task.into();
		active.revision = Set(revision);
		active.updated_at = Set(Utc::now());
		active.update(&txn).await?;
		txn.commit().await?;
		Ok(OrganizeDecisionOutcome::Applied {
			revision,
			affected_roots: selected_ids,
		})
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
		let unmarked_units = items
			.iter()
			.filter(|item| item.decision_kind.is_none())
			.map(|item| item.unit_count.unwrap_or(0).max(0) as u64)
			.sum();
		if unmarked_units > 0 && !input.confirm_unmarked {
			txn.rollback().await?;
			return Ok(OrganizeFinishOutcome::ConfirmationRequired { unmarked_units });
		}
		let pending = items
			.iter()
			.filter(|item| item.operation_state == "pending")
			.count() as u64;
		let running = items
			.iter()
			.filter(|item| item.operation_state == "running")
			.count() as u64;
		let failed = items
			.iter()
			.filter(|item| item.operation_state == "failed")
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
			let children = organize_task_item::Entity::find()
				.filter(organize_task_item::Column::TaskId.eq(task_id))
				.filter(organize_task_item::Column::ParentId.eq(parent.id))
				.order_by_asc(organize_task_item::Column::Name)
				.all(txn)
				.await?;
			Ok(children
				.into_iter()
				.filter(|item| selection_filter_matches(item, filter))
				.filter(|item| !excluded_item_ids.contains(&item.uuid))
				.map(|item| item.uuid)
				.collect())
		}
	}
}

fn selection_filter_matches(item: &organize_task_item::Model, filter: SelectionFilter) -> bool {
	match filter {
		SelectionFilter::All => true,
		SelectionFilter::Unmarked => item.decision_kind.is_none(),
		SelectionFilter::Keep => item.decision_kind.as_deref() == Some("keep"),
		SelectionFilter::Discard => item.decision_kind.as_deref() == Some("discard"),
		SelectionFilter::Move => item.decision_kind.as_deref() == Some("move"),
		SelectionFilter::Failed => item.operation_state == "failed",
		SelectionFilter::Changed => item.external_state == "changed",
		SelectionFilter::Missing => item.external_state == "missing",
	}
}

fn destructive_overwrite(
	decision: Option<&DecisionValue>,
	candidate: &organize_task_item::Model,
) -> bool {
	matches!(
		decision,
		Some(DecisionValue::Discard | DecisionValue::Move { .. })
	) && matches!(candidate.decision_kind.as_deref(), Some("keep" | "move"))
}

fn parse_child_cursor(cursor: &str) -> Result<(String, Uuid)> {
	let (name, uuid) = cursor
		.rsplit_once('|')
		.ok_or_else(|| OrganizeError::InvalidTree("invalid child cursor".into()))?;
	let uuid = Uuid::parse_str(uuid)
		.map_err(|_| OrganizeError::InvalidTree("invalid child cursor".into()))?;
	Ok((name.to_string(), uuid))
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
			Some(destination.clone()),
			operation_state(OrganizeOperationState::Pending),
		),
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
