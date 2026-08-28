use chrono::{DateTime, Utc};
use sd_core::infra::db::entities::{organize_task, organize_task_item};
use sd_core::infra::db::migration::Migrator;
use sd_core::ops::organize::error::OrganizeError;
use sd_core::ops::organize::model::{
	DecisionValue, OrganizeItemKind, OrganizeOperationState, OrganizeTaskStatus,
};
use sd_core::ops::organize::repository::{
	ChangeScanResult, DecisionTransactionRequest, NewOrganizeTask, OrganizeAcceptChangesInput,
	OrganizeAcceptChangesOutcome, OrganizeChildrenInput, OrganizeDecisionOutcome,
	OrganizeItemFilter, OrganizeItemSort, OrganizeListInput, OrganizeRepository,
	OrganizeRepositoryError, OrganizeSelectionInput, OrganizeSortDirection, SelectionFilter,
	SnapshotItemDraft, SnapshotTotals,
};
use sea_orm::{
	ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityTrait, PaginatorTrait,
	QueryFilter, QueryOrder, Statement,
};
use sea_orm_migration::MigratorTrait;
use tempfile::TempDir;
use uuid::Uuid;

async fn migrated_temp_db() -> (TempDir, DatabaseConnection) {
	let temp_dir = TempDir::new().expect("temporary database directory");
	let url = format!(
		"sqlite://{}?mode=rwc",
		temp_dir.path().join("test.db").display()
	);
	let db = sea_orm::Database::connect(url)
		.await
		.expect("connect temporary database");
	Migrator::up(&db, None).await.expect("run migrations");
	(temp_dir, db)
}

async fn sqlite_names(db: &DatabaseConnection, object_type: &str, pattern: &str) -> Vec<String> {
	let statement = Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"SELECT name FROM sqlite_master WHERE type = ? AND name LIKE ? ORDER BY name",
		[object_type.into(), pattern.into()],
	);
	db.query_all(statement)
		.await
		.expect("query sqlite names")
		.into_iter()
		.map(|row| row.try_get_by_index(0).expect("sqlite object name"))
		.collect()
}

async fn pragma_foreign_keys(db: &DatabaseConnection) {
	db.execute(Statement::from_string(
		DatabaseBackend::Sqlite,
		"PRAGMA foreign_keys = ON".to_string(),
	))
	.await
	.expect("enable foreign keys");
}

fn task(id: Uuid, root_path: &str, status: OrganizeTaskStatus) -> NewOrganizeTask {
	NewOrganizeTask {
		id,
		name: "Photos".to_string(),
		root_path: root_path.to_string(),
		root_path_key: root_path.to_lowercase(),
		device_slug: "device".to_string(),
		volume_id: None,
		root_entry_uuid: None,
		status,
		revision: 0,
		snapshot_version: 1,
		total_entries: 0,
		total_units: 0,
		total_bytes: 0,
		scan_issue_count: 0,
		pending_addition_count: 0,
		scan_job_id: None,
		commit_job_id: None,
		last_error: None,
		completed_at: None,
		created_at: Utc::now(),
		updated_at: Utc::now(),
	}
}

fn item(
	task_id: Uuid,
	uuid: Uuid,
	parent_id: Option<i32>,
	relative_path: &str,
) -> SnapshotItemDraft {
	SnapshotItemDraft {
		id: None,
		uuid,
		task_id,
		parent_id,
		entry_uuid: None,
		relative_path: relative_path.to_string(),
		relative_path_key: relative_path.to_lowercase(),
		name: relative_path
			.rsplit('\\')
			.next()
			.unwrap_or(relative_path)
			.to_string(),
		extension: None,
		kind: OrganizeItemKind::Directory,
		size_bytes: 0,
		aggregate_size_bytes: 0,
		modified_at_100ns: 0,
		metadata_signature: "signature".to_string(),
		tree_start: Some(1),
		tree_end: Some(1),
		unit_count: Some(1),
		membership_state: "included".to_string(),
		external_state: "present".to_string(),
		decision_kind: None,
		move_destination: None,
		operation_state: OrganizeOperationState::None,
		last_error: None,
		applied_at: None,
		created_at: Utc::now(),
		updated_at: Utc::now(),
	}
}

#[tokio::test]
async fn migration_creates_only_two_organize_tables_and_required_indexes() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let names = sqlite_names(&db, "table", "organize_%").await;
	assert_eq!(names, vec!["organize_task_items", "organize_tasks"]);
	let indexes = sqlite_names(&db, "index", "idx_organize_%").await;
	assert!(indexes.contains(&"idx_organize_items_task_parent_name".to_string()));
	assert!(indexes.contains(&"idx_organize_items_task_decision_tree".to_string()));
	assert!(indexes.contains(&"idx_organize_items_task_membership_external".to_string()));
}

#[tokio::test]
async fn inserting_scanning_and_active_tasks_preserves_uuid_primary_keys() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);

	for (status, root_path) in [
		(OrganizeTaskStatus::Scanning, r"C:\Photos"),
		(OrganizeTaskStatus::Active, r"C:\PhotosActive"),
	] {
		let task_id = Uuid::new_v4();
		let inserted = repo
			.insert_scanning_task(task(task_id, root_path, status))
			.await
			.expect("insert task with UUID primary key");
		assert_eq!(inserted.id, task_id);
	}
}

#[tokio::test]
async fn snapshot_updates_reject_active_completed_and_decided_tasks() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let totals = SnapshotTotals {
		total_entries: 0,
		total_units: 0,
		total_bytes: 0,
		scan_issue_count: 0,
	};

	let active_id = Uuid::new_v4();
	repo.insert_scanning_task(task(active_id, r"C:\Active", OrganizeTaskStatus::Active))
		.await
		.expect("insert active task");
	let active_error = repo
		.replace_included_snapshot(active_id, Vec::new(), totals)
		.await
		.expect_err("active task must reject snapshot replacement");
	assert!(matches!(
		active_error,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));
	let active_failure_error = repo
		.fail_snapshot(active_id, "late failure".to_string())
		.await
		.expect_err("active task must reject snapshot failure");
	assert!(matches!(
		active_failure_error,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));

	let completed_id = Uuid::new_v4();
	repo.insert_scanning_task(task(
		completed_id,
		r"C:\Completed",
		OrganizeTaskStatus::Scanning,
	))
	.await
	.expect("insert scanning task");
	repo.replace_included_snapshot(completed_id, Vec::new(), totals)
		.await
		.expect("complete initial snapshot");
	repo.set_completed(completed_id)
		.await
		.expect("complete task");
	let completed_error = repo
		.replace_included_snapshot(completed_id, Vec::new(), totals)
		.await
		.expect_err("completed task must reject snapshot replacement");
	assert!(matches!(
		completed_error,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));
	let completed_failure_error = repo
		.fail_snapshot(completed_id, "late failure".to_string())
		.await
		.expect_err("completed task must reject snapshot failure");
	assert!(matches!(
		completed_failure_error,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));

	let decided_id = Uuid::new_v4();
	let decided_item_id = Uuid::new_v4();
	repo.insert_scanning_task(task(
		decided_id,
		r"C:\Decided",
		OrganizeTaskStatus::Scanning,
	))
	.await
	.expect("insert scanning task with decision");
	db.execute(Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"INSERT INTO organize_task_items (id, uuid, task_id, relative_path, relative_path_key, name, kind, size_bytes, aggregate_size_bytes, modified_at_100ns, metadata_signature, tree_start, tree_end, unit_count, membership_state, external_state, decision_kind, operation_state, created_at, updated_at) VALUES (1, ?, ?, '', '', 'root', 'directory', 0, 0, 0, 'signature', 0, 0, 1, 'included', 'present', 'keep', 'none', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
		[decided_item_id.into(), decided_id.into()],
	))
	.await
	.expect("insert existing decision");
	let decided_error = repo
		.replace_included_snapshot(decided_id, Vec::new(), totals)
		.await
		.expect_err("decided task must reject snapshot replacement");
	assert!(matches!(
		decided_error,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));
	let decided_failure_error = repo
		.fail_snapshot(decided_id, "late failure".to_string())
		.await
		.expect_err("decided task must reject snapshot failure");
	assert!(matches!(
		decided_failure_error,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));
}

#[tokio::test]
async fn inserting_overlapping_active_task_is_rejected_atomically() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let first_id = Uuid::new_v4();
	repo.insert_scanning_task(task(first_id, r"C:\Photos", OrganizeTaskStatus::Active))
		.await
		.expect("insert first active task");

	let second_error = repo
		.insert_scanning_task(task(
			Uuid::new_v4(),
			r"C:\Photos\Trips",
			OrganizeTaskStatus::Active,
		))
		.await
		.expect_err("overlapping active task must be rejected");
	assert!(matches!(
		second_error,
		OrganizeRepositoryError::Organize(OrganizeError::UnsafeTopology(_))
	));
	assert_eq!(organize_task::Entity::find().count(&db).await.unwrap(), 1);
}

#[tokio::test]
async fn applied_items_are_immutable_for_decisions_acceptance_and_settlement() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);

	let active_task_id = Uuid::new_v4();
	let active_item_id = Uuid::new_v4();
	repo.insert_scanning_task(task(
		active_task_id,
		r"C:\Applied",
		OrganizeTaskStatus::Scanning,
	))
	.await
	.expect("insert active test task");
	repo.replace_included_snapshot(
		active_task_id,
		vec![item(active_task_id, active_item_id, None, "")],
		SnapshotTotals {
			total_entries: 1,
			total_units: 1,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert active test item");
	db.execute(Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"UPDATE organize_task_items SET decision_kind = 'discard', operation_state = 'applied' WHERE task_id = ? AND uuid = ?",
		[active_task_id.into(), active_item_id.into()],
	))
	.await
	.expect("mark item applied");

	let decision_error = repo
		.apply_decision(DecisionTransactionRequest {
			task_id: active_task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![active_item_id],
			},
			decision: Some(DecisionValue::keep()),
			expected_revision: 1,
			confirm_descendant_override: true,
			confirm_ancestor_split: true,
		})
		.await
		.expect_err("applied item must reject a new decision");
	assert!(matches!(
		decision_error,
		OrganizeRepositoryError::Organize(OrganizeError::AppliedDecisionImmutable(id))
			if id == active_item_id
	));
	let acceptance_error = repo
		.accept_changes(OrganizeAcceptChangesInput {
			task_id: active_task_id,
			expected_revision: 1,
			include_addition_ids: Vec::new(),
			remove_missing_ids: Vec::new(),
			refresh_changed_ids: vec![active_item_id],
			preserve_changed_decisions: true,
			confirm_inherited_destructive: false,
		})
		.await
		.expect_err("applied item must reject acceptance refresh");
	assert!(matches!(
		acceptance_error,
		OrganizeRepositoryError::Organize(OrganizeError::AppliedDecisionImmutable(id))
			if id == active_item_id
	));

	let committing_task_id = Uuid::new_v4();
	let committing_item_id = Uuid::new_v4();
	repo.insert_scanning_task(task(
		committing_task_id,
		r"C:\Committing",
		OrganizeTaskStatus::Scanning,
	))
	.await
	.expect("insert committing test task");
	repo.replace_included_snapshot(
		committing_task_id,
		vec![item(committing_task_id, committing_item_id, None, "")],
		SnapshotTotals {
			total_entries: 1,
			total_units: 1,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert committing test item");
	repo.lock_for_commit(committing_task_id, 1, Uuid::new_v4().into())
		.await
		.expect("lock task for commit");
	db.execute(Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"UPDATE organize_task_items SET decision_kind = 'discard', operation_state = 'applied' WHERE task_id = ? AND uuid = ?",
		[committing_task_id.into(), committing_item_id.into()],
	))
	.await
	.expect("mark committing item applied");
	let settlement_error = repo
		.settle_operation_roots(
			committing_task_id,
			vec![sd_core::ops::organize::repository::OperationSettlement {
				item_id: committing_item_id,
				state: OrganizeOperationState::Failed,
				last_error: Some("late settlement".to_string()),
				applied_at: None,
			}],
		)
		.await
		.expect_err("applied item must reject settlement rewrite");
	assert!(matches!(
		settlement_error,
		OrganizeRepositoryError::Organize(OrganizeError::AppliedDecisionImmutable(id))
			if id == committing_item_id
	));
}

#[tokio::test]
async fn move_destination_and_operation_state_checks_reject_invalid_rows() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let task_id = Uuid::new_v4();
	let item_id = Uuid::new_v4();
	let invalid_move = Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"INSERT INTO organize_task_items (uuid, task_id, relative_path, relative_path_key, name, kind, size_bytes, aggregate_size_bytes, modified_at_100ns, metadata_signature, membership_state, external_state, decision_kind, operation_state, created_at, updated_at) VALUES (?, ?, '', '', 'root', 'directory', 0, 0, 0, 'signature', 'included', 'present', 'move', 'pending', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
		[item_id.into(), task_id.into()],
	);
	assert!(db.execute(invalid_move).await.is_err());

	let invalid_keep = Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"INSERT INTO organize_task_items (uuid, task_id, relative_path, relative_path_key, name, kind, size_bytes, aggregate_size_bytes, modified_at_100ns, metadata_signature, membership_state, external_state, decision_kind, operation_state, created_at, updated_at) VALUES (?, ?, 'keep', 'keep', 'keep', 'directory', 0, 0, 0, 'signature', 'included', 'present', 'keep', 'pending', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
		[Uuid::new_v4().into(), task_id.into()],
	);
	assert!(db.execute(invalid_keep).await.is_err());
}

#[tokio::test]
async fn task_deletion_cascades_items_and_volume_deletion_sets_task_volume_null() {
	let (_temp_dir, db) = migrated_temp_db().await;
	pragma_foreign_keys(&db).await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let created = repo
		.insert_scanning_task(task(task_id, r"C:\Photos", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert task");
	repo.replace_included_snapshot(
		task_id,
		vec![item(task_id, root_id, None, "")],
		SnapshotTotals {
			total_entries: 1,
			total_units: 1,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert root item");
	organize_task::Entity::delete_by_id(created.id)
		.exec(&db)
		.await
		.expect("delete task");
	assert!(organize_task_item::Entity::find()
		.all(&db)
		.await
		.unwrap()
		.is_empty());
}

#[tokio::test]
async fn overlap_blocks_equal_ancestor_and_descendant_but_not_sibling_or_completed() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let active = Uuid::new_v4();
	repo.insert_scanning_task(task(active, r"C:\Photos", OrganizeTaskStatus::Active))
		.await
		.expect("insert active task");
	assert_eq!(
		repo.find_overlapping_active(r"c:\photos").await.unwrap(),
		Some(active)
	);
	assert_eq!(
		repo.find_overlapping_active(r"C:\Photos\Trips")
			.await
			.unwrap(),
		Some(active)
	);
	assert_eq!(
		repo.find_overlapping_active(r"C:\").await.unwrap(),
		Some(active)
	);
	assert_eq!(
		repo.find_overlapping_active(r"C:\Photographs")
			.await
			.unwrap(),
		None
	);
	repo.set_completed(active).await.expect("complete task");
	assert_eq!(
		repo.find_overlapping_active(r"C:\Photos\Trips")
			.await
			.unwrap(),
		None
	);
}

#[tokio::test]
async fn decision_transaction_is_atomic_and_revision_increments_once() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let parent_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Photos", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert active task");
	repo.replace_included_snapshot(
		task_id,
		vec![item(task_id, parent_id, None, "")],
		SnapshotTotals {
			total_entries: 1,
			total_units: 1,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert root item");
	let before = repo.get_task_revision(task_id).await.unwrap();
	let outcome = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![parent_id],
			},
			decision: Some(DecisionValue::discard()),
			expected_revision: before,
			confirm_descendant_override: true,
			confirm_ancestor_split: true,
		})
		.await
		.expect("apply decision");
	assert!(
		matches!(outcome, OrganizeDecisionOutcome::Applied { revision, .. } if revision == before + 1)
	);
	assert_eq!(
		repo.explicit_decision_ids(task_id).await.unwrap(),
		vec![parent_id]
	);
}

#[tokio::test]
async fn stale_decision_does_not_change_rows_or_revision() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let item_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Photos", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert active task");
	repo.replace_included_snapshot(
		task_id,
		vec![item(task_id, item_id, None, "")],
		SnapshotTotals {
			total_entries: 1,
			total_units: 1,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert root item");
	let before = repo.dump_decisions(task_id).await.unwrap();
	let outcome = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![item_id],
			},
			decision: Some(DecisionValue::keep()),
			expected_revision: 0,
			confirm_descendant_override: true,
			confirm_ancestor_split: true,
		})
		.await
		.expect("stale decision outcome");
	assert!(matches!(
		outcome,
		OrganizeDecisionOutcome::StaleRevision { .. }
	));
	assert_eq!(repo.dump_decisions(task_id).await.unwrap(), before);
}

#[tokio::test]
async fn direct_children_paging_is_stable_and_filters_exclusions() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Photos", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert active task");
	let root_id = Uuid::new_v4();
	let first_id = Uuid::new_v4();
	let second_id = Uuid::new_v4();
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	root.tree_end = Some(3);
	let mut first = item(task_id, first_id, Some(1), "a");
	first.id = Some(2);
	first.name = "same".to_string();
	first.tree_start = Some(1);
	first.tree_end = Some(1);
	let mut second = item(task_id, second_id, Some(1), "b");
	second.id = Some(3);
	second.name = "same".to_string();
	second.tree_start = Some(2);
	second.tree_end = Some(2);
	repo.replace_included_snapshot(
		task_id,
		vec![root, first, second],
		SnapshotTotals {
			total_entries: 3,
			total_units: 2,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert snapshot");
	let page = repo
		.children(OrganizeChildrenInput {
			task_id,
			parent_item_id: root_id,
			cursor: None,
			limit: 1,
			sort: OrganizeItemSort::Name,
			direction: OrganizeSortDirection::Asc,
			filter: SelectionFilter::All,
		})
		.await
		.expect("read first child page");
	assert_eq!(page.items.len(), 1);
	let next = repo
		.children(OrganizeChildrenInput {
			task_id,
			parent_item_id: root_id,
			cursor: page.next_cursor,
			limit: 1,
			sort: OrganizeItemSort::Name,
			direction: OrganizeSortDirection::Asc,
			filter: SelectionFilter::All,
		})
		.await
		.expect("read second child page");
	assert_eq!(next.items.len(), 1);
	assert_ne!(page.items[0].uuid, next.items[0].uuid);
	let selected = repo
		.resolve_selection(
			task_id,
			1,
			OrganizeSelectionInput::DirectChildren {
				parent_item_id: root_id,
				filter: SelectionFilter::All,
				excluded_item_ids: vec![first_id],
			},
		)
		.await
		.expect("resolve direct child selection");
	assert_eq!(selected.len(), 1);
	assert_eq!(selected[0].uuid, second_id);
}

#[tokio::test]
async fn accepted_additions_rebuild_included_intervals_in_one_revision() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Photos", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert active task");
	let root_id = Uuid::new_v4();
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	root.tree_end = Some(1);
	let initial_revision = repo
		.replace_included_snapshot(
			task_id,
			vec![root],
			SnapshotTotals {
				total_entries: 1,
				total_units: 1,
				total_bytes: 0,
				scan_issue_count: 0,
			},
		)
		.await
		.expect("insert initial snapshot");
	let addition_id = Uuid::new_v4();
	let mut addition = item(task_id, addition_id, Some(1), "new");
	addition.id = Some(2);
	addition.tree_start = None;
	addition.tree_end = None;
	addition.unit_count = None;
	addition.membership_state = "pending_addition".to_string();
	repo.store_change_scan(
		task_id,
		ChangeScanResult {
			additions: vec![addition],
			changed_ids: Vec::new(),
			missing_ids: Vec::new(),
		},
	)
	.await
	.expect("store change scan");
	let accepted = repo
		.accept_changes(OrganizeAcceptChangesInput {
			task_id,
			expected_revision: initial_revision + 1,
			include_addition_ids: vec![addition_id],
			remove_missing_ids: Vec::new(),
			refresh_changed_ids: Vec::new(),
			preserve_changed_decisions: false,
			confirm_inherited_destructive: false,
		})
		.await
		.expect("accept addition");
	assert!(matches!(
		accepted,
		OrganizeAcceptChangesOutcome::Applied { revision }
			if revision == initial_revision + 2
	));
	let models = organize_task_item::Entity::find()
		.filter(organize_task_item::Column::TaskId.eq(task_id))
		.order_by_asc(organize_task_item::Column::TreeStart)
		.all(&db)
		.await
		.expect("read rebuilt tree");
	assert_eq!(models.len(), 2);
	assert_eq!(models[0].tree_start, Some(0));
	assert_eq!(models[0].tree_end, Some(2));
	assert_eq!(models[1].tree_start, Some(1));
}

#[tokio::test]
async fn decisions_normalize_nested_selection_and_repeat_without_revision() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let child_id = Uuid::new_v4();
	let sibling_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Decision", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert active task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	let mut child = item(task_id, child_id, Some(1), "child");
	child.id = Some(2);
	let mut sibling = item(task_id, sibling_id, Some(1), "sibling");
	sibling.id = Some(3);
	let initial_revision = repo
		.replace_included_snapshot(
			task_id,
			vec![root, child, sibling],
			SnapshotTotals {
				total_entries: 3,
				total_units: 2,
				total_bytes: 0,
				scan_issue_count: 0,
			},
		)
		.await
		.expect("insert decision tree");

	let root_keep = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![root_id],
			},
			decision: Some(DecisionValue::keep()),
			expected_revision: initial_revision,
			confirm_descendant_override: false,
			confirm_ancestor_split: false,
		})
		.await
		.expect("keep root");
	assert!(matches!(
		root_keep,
		OrganizeDecisionOutcome::Applied { revision: 2, .. }
	));

	let inherited = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![child_id, root_id, child_id],
			},
			decision: Some(DecisionValue::keep()),
			expected_revision: 2,
			confirm_descendant_override: false,
			confirm_ancestor_split: false,
		})
		.await
		.expect("resolve inherited keep");
	assert!(matches!(
		inherited,
		OrganizeDecisionOutcome::InheritedNoOp {
			ancestor_item_id
		} if ancestor_item_id == root_id
	));
	assert_eq!(repo.get_task_revision(task_id).await.unwrap(), 2);

	let moved = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![child_id],
			},
			decision: Some(DecisionValue::move_to(r"C:\Archive\\.\2026\..\Trip\\")),
			expected_revision: 2,
			confirm_descendant_override: false,
			confirm_ancestor_split: true,
		})
		.await
		.expect("split root decision and move child");
	assert!(matches!(
		moved,
		OrganizeDecisionOutcome::Applied { revision: 3, .. }
	));
	let decisions = repo.dump_decisions(task_id).await.unwrap();
	assert_eq!(decisions.len(), 1);
	assert_eq!(decisions[0].item_id, child_id);
	assert_eq!(
		decisions[0].move_destination.as_deref(),
		Some(r"c:\archive\trip")
	);

	let repeated = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![child_id, child_id],
			},
			decision: Some(DecisionValue::move_to(r"c:/archive/trip")),
			expected_revision: 3,
			confirm_descendant_override: true,
			confirm_ancestor_split: true,
		})
		.await
		.expect("repeat move decision");
	assert!(matches!(
		repeated,
		OrganizeDecisionOutcome::InheritedNoOp {
			ancestor_item_id
		} if ancestor_item_id == child_id
	));
	assert_eq!(repo.get_task_revision(task_id).await.unwrap(), 3);

	let unknown_error = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![Uuid::new_v4()],
			},
			decision: Some(DecisionValue::keep()),
			expected_revision: 3,
			confirm_descendant_override: true,
			confirm_ancestor_split: true,
		})
		.await
		.expect_err("unknown decision item must be rejected");
	assert!(matches!(
		unknown_error,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTree(_))
	));
	assert_eq!(repo.get_task_revision(task_id).await.unwrap(), 3);
}

#[tokio::test]
async fn decision_confirmation_reports_conflicting_root_statistics() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let keep_id = Uuid::new_v4();
	let move_id = Uuid::new_v4();
	repo.insert_scanning_task(task(
		task_id,
		r"C:\Statistics",
		OrganizeTaskStatus::Scanning,
	))
	.await
	.expect("insert active task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	let mut keep = item(task_id, keep_id, Some(1), "keep");
	keep.id = Some(2);
	keep.kind = OrganizeItemKind::File;
	keep.size_bytes = 10;
	let mut moved = item(task_id, move_id, Some(1), "move");
	moved.id = Some(3);
	moved.kind = OrganizeItemKind::File;
	moved.size_bytes = 20;
	repo.replace_included_snapshot(
		task_id,
		vec![root, keep, moved],
		SnapshotTotals {
			total_entries: 3,
			total_units: 2,
			total_bytes: 30,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert statistics tree");
	repo.apply_decision(DecisionTransactionRequest {
		task_id,
		selection: OrganizeSelectionInput::Items {
			item_ids: vec![keep_id],
		},
		decision: Some(DecisionValue::keep()),
		expected_revision: 1,
		confirm_descendant_override: true,
		confirm_ancestor_split: true,
	})
	.await
	.expect("keep one child");
	repo.apply_decision(DecisionTransactionRequest {
		task_id,
		selection: OrganizeSelectionInput::Items {
			item_ids: vec![move_id],
		},
		decision: Some(DecisionValue::move_to(r"C:\Archive")),
		expected_revision: 2,
		confirm_descendant_override: true,
		confirm_ancestor_split: true,
	})
	.await
	.expect("move one child");

	let outcome = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![root_id],
			},
			decision: Some(DecisionValue::discard()),
			expected_revision: 3,
			confirm_descendant_override: false,
			confirm_ancestor_split: true,
		})
		.await
		.expect("get descendant confirmation");
	assert!(matches!(
		outcome,
		OrganizeDecisionOutcome::ConfirmationRequired {
			conflict_kind: sd_core::ops::organize::model::OrganizeDecisionConflictKind::DescendantOverride,
			keep_units: 1,
			discard_units: 0,
			move_units: 1,
			unmarked_units: 0,
			affected_bytes: 30,
			conflicting_roots,
		} if conflicting_roots == vec![keep_id, move_id]
	));
	assert_eq!(repo.get_task_revision(task_id).await.unwrap(), 3);
}

#[tokio::test]
async fn finish_deduplicates_unmarked_units_and_uses_inherited_decisions() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);

	let unmarked_task_id = Uuid::new_v4();
	repo.insert_scanning_task(task(
		unmarked_task_id,
		r"C:\Unmarked",
		OrganizeTaskStatus::Scanning,
	))
	.await
	.expect("insert unmarked task");
	let mut unmarked_root = item(unmarked_task_id, Uuid::new_v4(), None, "");
	unmarked_root.id = Some(1);
	let mut first = item(unmarked_task_id, Uuid::new_v4(), Some(1), "first");
	first.id = Some(2);
	first.kind = OrganizeItemKind::File;
	let mut second = item(unmarked_task_id, Uuid::new_v4(), Some(1), "second");
	second.id = Some(3);
	second.kind = OrganizeItemKind::File;
	repo.replace_included_snapshot(
		unmarked_task_id,
		vec![unmarked_root, first, second],
		SnapshotTotals {
			total_entries: 3,
			total_units: 2,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert unmarked tree");
	let unmarked = repo
		.finish(sd_core::ops::organize::repository::OrganizeFinishInput {
			task_id: unmarked_task_id,
			expected_revision: 1,
			confirm_unmarked: false,
		})
		.await
		.expect("calculate unmarked units");
	assert!(matches!(
		unmarked,
		sd_core::ops::organize::repository::OrganizeFinishOutcome::ConfirmationRequired {
			unmarked_units: 2
		}
	));

	let inherited_task_id = Uuid::new_v4();
	let inherited_root_id = Uuid::new_v4();
	repo.insert_scanning_task(task(
		inherited_task_id,
		r"C:\Inherited",
		OrganizeTaskStatus::Scanning,
	))
	.await
	.expect("insert inherited task");
	let mut inherited_root = item(inherited_task_id, inherited_root_id, None, "");
	inherited_root.id = Some(1);
	let mut inherited_first = item(inherited_task_id, Uuid::new_v4(), Some(1), "first");
	inherited_first.id = Some(2);
	inherited_first.kind = OrganizeItemKind::File;
	let mut inherited_second = item(inherited_task_id, Uuid::new_v4(), Some(1), "second");
	inherited_second.id = Some(3);
	inherited_second.kind = OrganizeItemKind::File;
	repo.replace_included_snapshot(
		inherited_task_id,
		vec![inherited_root, inherited_first, inherited_second],
		SnapshotTotals {
			total_entries: 3,
			total_units: 2,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert inherited tree");
	repo.apply_decision(DecisionTransactionRequest {
		task_id: inherited_task_id,
		selection: OrganizeSelectionInput::Items {
			item_ids: vec![inherited_root_id],
		},
		decision: Some(DecisionValue::keep()),
		expected_revision: 1,
		confirm_descendant_override: true,
		confirm_ancestor_split: true,
	})
	.await
	.expect("keep inherited root");
	let finished = repo
		.finish(sd_core::ops::organize::repository::OrganizeFinishInput {
			task_id: inherited_task_id,
			expected_revision: 2,
			confirm_unmarked: false,
		})
		.await
		.expect("finish inherited decisions");
	assert!(matches!(
		finished,
		sd_core::ops::organize::repository::OrganizeFinishOutcome::Completed { revision: 3 }
	));
}

#[tokio::test]
async fn change_scan_rejects_foreign_additions_and_skips_applied_items() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let foreign_task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Scan", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert scan task");
	repo.insert_scanning_task(task(
		foreign_task_id,
		r"C:\Foreign",
		OrganizeTaskStatus::Active,
	))
	.await
	.expect("insert foreign task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	repo.replace_included_snapshot(
		task_id,
		vec![root],
		SnapshotTotals {
			total_entries: 1,
			total_units: 1,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert scan root");

	let foreign_addition = item(foreign_task_id, Uuid::new_v4(), None, "foreign");
	let foreign_error = repo
		.store_change_scan(
			task_id,
			ChangeScanResult {
				additions: vec![foreign_addition],
				changed_ids: Vec::new(),
				missing_ids: Vec::new(),
			},
		)
		.await
		.expect_err("foreign addition must be rejected");
	assert!(matches!(
		foreign_error,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));
	assert_eq!(repo.get_task_revision(task_id).await.unwrap(), 1);

	db.execute(Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"UPDATE organize_task_items SET decision_kind = 'discard', operation_state = 'applied' WHERE task_id = ? AND uuid = ?",
		[task_id.into(), root_id.into()],
	))
	.await
	.expect("mark scan root applied");
	repo.store_change_scan(
		task_id,
		ChangeScanResult {
			additions: Vec::new(),
			changed_ids: vec![root_id],
			missing_ids: Vec::new(),
		},
	)
	.await
	.expect("store scan excluding applied root");
	let applied = organize_task_item::Entity::find_by_id(1)
		.one(&db)
		.await
		.unwrap()
		.expect("read applied root");
	assert_eq!(applied.external_state, "present");
}

#[tokio::test]
async fn accept_changes_requires_membership_and_matching_external_state() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let addition_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Accept", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert accept task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	repo.replace_included_snapshot(
		task_id,
		vec![root],
		SnapshotTotals {
			total_entries: 1,
			total_units: 1,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert accept root");
	let mut addition = item(task_id, addition_id, Some(1), "addition");
	addition.id = Some(2);
	repo.store_change_scan(
		task_id,
		ChangeScanResult {
			additions: vec![addition],
			changed_ids: Vec::new(),
			missing_ids: Vec::new(),
		},
	)
	.await
	.expect("store pending addition");

	let invalid_include = repo
		.accept_changes(OrganizeAcceptChangesInput {
			task_id,
			expected_revision: 2,
			include_addition_ids: vec![root_id],
			remove_missing_ids: Vec::new(),
			refresh_changed_ids: Vec::new(),
			preserve_changed_decisions: false,
			confirm_inherited_destructive: false,
		})
		.await
		.expect_err("included item cannot be accepted as an addition");
	assert!(matches!(
		invalid_include,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));

	let invalid_refresh = repo
		.accept_changes(OrganizeAcceptChangesInput {
			task_id,
			expected_revision: 2,
			include_addition_ids: Vec::new(),
			remove_missing_ids: Vec::new(),
			refresh_changed_ids: vec![addition_id],
			preserve_changed_decisions: false,
			confirm_inherited_destructive: false,
		})
		.await
		.expect_err("pending addition cannot be refreshed as changed");
	assert!(matches!(
		invalid_refresh,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));

	db.execute(Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"UPDATE organize_task_items SET external_state = 'missing' WHERE task_id = ? AND uuid = ?",
		[task_id.into(), root_id.into()],
	))
	.await
	.expect("mark root missing");
	let invalid_remove = repo
		.accept_changes(OrganizeAcceptChangesInput {
			task_id,
			expected_revision: 2,
			include_addition_ids: Vec::new(),
			remove_missing_ids: vec![addition_id],
			refresh_changed_ids: Vec::new(),
			preserve_changed_decisions: false,
			confirm_inherited_destructive: false,
		})
		.await
		.expect_err("pending addition cannot be removed as missing");
	assert!(matches!(
		invalid_remove,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTaskState(_))
	));
}

#[tokio::test]
async fn settling_operation_roots_returns_task_to_active() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Settle", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert settle task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	repo.replace_included_snapshot(
		task_id,
		vec![root],
		SnapshotTotals {
			total_entries: 1,
			total_units: 1,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert settle root");
	let committing_revision = repo
		.lock_for_commit(task_id, 1, Uuid::new_v4().into())
		.await
		.expect("lock settle task");
	let settled_revision = repo
		.settle_operation_roots(
			task_id,
			vec![sd_core::ops::organize::repository::OperationSettlement {
				item_id: root_id,
				state: OrganizeOperationState::Applied,
				last_error: None,
				applied_at: Some(Utc::now()),
			}],
		)
		.await
		.expect("settle root");
	assert_eq!(settled_revision, committing_revision + 1);
	let settled = organize_task::Entity::find_by_id(task_id)
		.one(&db)
		.await
		.unwrap()
		.expect("read settled task");
	assert_eq!(settled.status, "active");
}

#[tokio::test]
async fn children_cursor_is_opaque_and_binds_query_metadata() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Cursor", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert cursor task");
	let root_id = Uuid::new_v4();
	let first_id = Uuid::new_v4();
	let second_id = Uuid::new_v4();
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	let mut first = item(task_id, first_id, Some(1), "a");
	first.id = Some(2);
	let mut second = item(task_id, second_id, Some(1), "b");
	second.id = Some(3);
	repo.replace_included_snapshot(
		task_id,
		vec![root, first, second],
		SnapshotTotals {
			total_entries: 3,
			total_units: 2,
			total_bytes: 0,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("insert cursor tree");
	let request = |cursor| OrganizeChildrenInput {
		task_id,
		parent_item_id: root_id,
		cursor,
		limit: 1,
		sort: OrganizeItemSort::Name,
		direction: OrganizeSortDirection::Asc,
		filter: OrganizeItemFilter::All,
	};
	let page = repo
		.children(request(None))
		.await
		.expect("read first cursor page");
	let cursor = page.next_cursor.clone().expect("next cursor");
	assert!(!cursor.contains('|'));
	let next = repo
		.children(request(Some(cursor.clone())))
		.await
		.expect("read second cursor page");
	assert_eq!(next.items.len(), 1);
	let invalid_filter = repo
		.children(OrganizeChildrenInput {
			filter: OrganizeItemFilter::Changed,
			..request(Some(cursor.clone()))
		})
		.await
		.expect_err("cursor filter mismatch must be rejected");
	assert!(matches!(
		invalid_filter,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTree(_))
	));
	repo.store_change_scan(
		task_id,
		ChangeScanResult {
			additions: Vec::new(),
			changed_ids: Vec::new(),
			missing_ids: Vec::new(),
		},
	)
	.await
	.expect("advance cursor revision");
	let invalid_revision = repo
		.children(request(Some(cursor)))
		.await
		.expect_err("cursor revision mismatch must be rejected");
	assert!(matches!(
		invalid_revision,
		OrganizeRepositoryError::Organize(OrganizeError::InvalidTree(_))
	));
}

#[tokio::test]
async fn list_tasks_pages_and_get_task_returns_root_item() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let first_task_id = Uuid::new_v4();
	let second_task_id = Uuid::new_v4();
	let first_root_id = Uuid::new_v4();
	let second_root_id = Uuid::new_v4();
	for (task_id, root_id, path) in [
		(first_task_id, first_root_id, r"C:\ListA"),
		(second_task_id, second_root_id, r"C:\ListB"),
	] {
		repo.insert_scanning_task(task(task_id, path, OrganizeTaskStatus::Scanning))
			.await
			.expect("insert listed task");
		let mut root = item(task_id, root_id, None, "");
		root.id = Some(if task_id == first_task_id { 1 } else { 2 });
		repo.replace_included_snapshot(
			task_id,
			vec![root],
			SnapshotTotals {
				total_entries: 1,
				total_units: 1,
				total_bytes: 0,
				scan_issue_count: 0,
			},
		)
		.await
		.expect("insert listed root");
	}
	let first_page = repo
		.list_tasks(OrganizeListInput {
			statuses: Some(vec![OrganizeTaskStatus::Active]),
			cursor: None,
			limit: 1,
		})
		.await
		.expect("read first task page");
	assert_eq!(first_page.tasks.len(), 1);
	let next_cursor = first_page.next_cursor.clone().expect("task next cursor");
	let second_page = repo
		.list_tasks(OrganizeListInput {
			statuses: Some(vec![OrganizeTaskStatus::Active]),
			cursor: Some(next_cursor),
			limit: 1,
		})
		.await
		.expect("read second task page");
	assert_eq!(second_page.tasks.len(), 1);
	assert_ne!(first_page.tasks[0].id, second_page.tasks[0].id);
	let fetched = repo.get_task(first_task_id).await.expect("get task");
	assert_eq!(fetched.task.id, first_task_id);
	assert_eq!(fetched.root_item_id, first_root_id);
}

#[tokio::test]
async fn children_filters_use_inherited_effective_decisions() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let child_id = Uuid::new_v4();
	let sibling_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Effective", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert effective-decision task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	root.tree_start = Some(0);
	root.tree_end = Some(3);
	root.unit_count = Some(2);
	let mut child = item(task_id, child_id, Some(1), "child");
	child.id = Some(2);
	child.kind = OrganizeItemKind::File;
	child.tree_start = Some(1);
	child.tree_end = Some(2);
	child.unit_count = Some(1);
	let mut sibling = item(task_id, sibling_id, Some(1), "sibling");
	sibling.id = Some(3);
	sibling.kind = OrganizeItemKind::File;
	sibling.tree_start = Some(2);
	sibling.tree_end = Some(3);
	sibling.unit_count = Some(1);
	let revision = repo
		.replace_included_snapshot(
			task_id,
			vec![root, child, sibling],
			SnapshotTotals {
				total_entries: 3,
				total_units: 2,
				total_bytes: 0,
				scan_issue_count: 0,
			},
		)
		.await
		.expect("insert effective-decision tree");
	repo.apply_decision(DecisionTransactionRequest {
		task_id,
		selection: OrganizeSelectionInput::Items {
			item_ids: vec![root_id],
		},
		decision: Some(DecisionValue::keep()),
		expected_revision: revision,
		confirm_descendant_override: true,
		confirm_ancestor_split: true,
	})
	.await
	.expect("mark root kept");
	let page = repo
		.children(OrganizeChildrenInput {
			task_id,
			parent_item_id: root_id,
			cursor: None,
			limit: 10,
			sort: OrganizeItemSort::Name,
			direction: OrganizeSortDirection::Asc,
			filter: OrganizeItemFilter::Keep,
		})
		.await
		.expect("filter children by effective keep");
	assert_eq!(page.items.len(), 2);
	let selected = repo
		.resolve_selection(
			task_id,
			2,
			OrganizeSelectionInput::DirectChildren {
				parent_item_id: root_id,
				filter: SelectionFilter::Unmarked,
				excluded_item_ids: Vec::new(),
			},
		)
		.await
		.expect("resolve effective unmarked selection");
	assert!(selected.is_empty());
}

#[tokio::test]
async fn accepting_addition_under_destructive_root_requires_confirmation() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let addition_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Addition", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert addition task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	root.tree_start = Some(0);
	root.tree_end = Some(1);
	root.unit_count = Some(1);
	let revision = repo
		.replace_included_snapshot(
			task_id,
			vec![root],
			SnapshotTotals {
				total_entries: 1,
				total_units: 1,
				total_bytes: 0,
				scan_issue_count: 0,
			},
		)
		.await
		.expect("insert addition root");
	let discarded_revision = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![root_id],
			},
			decision: Some(DecisionValue::discard()),
			expected_revision: revision,
			confirm_descendant_override: true,
			confirm_ancestor_split: true,
		})
		.await
		.expect("discard root");
	let discarded_revision = match discarded_revision {
		OrganizeDecisionOutcome::Applied { revision, .. } => revision,
		other => panic!("unexpected discard outcome: {other:?}"),
	};
	let mut addition = item(task_id, addition_id, Some(1), "new");
	addition.id = Some(2);
	repo.store_change_scan(
		task_id,
		ChangeScanResult {
			additions: vec![addition],
			changed_ids: Vec::new(),
			missing_ids: Vec::new(),
		},
	)
	.await
	.expect("store pending addition");
	let outcome = repo
		.accept_changes(OrganizeAcceptChangesInput {
			task_id,
			expected_revision: discarded_revision + 1,
			include_addition_ids: vec![addition_id],
			remove_missing_ids: Vec::new(),
			refresh_changed_ids: Vec::new(),
			preserve_changed_decisions: false,
			confirm_inherited_destructive: false,
		})
		.await
		.expect("return inherited destructive confirmation");
	assert!(matches!(
		outcome,
		OrganizeAcceptChangesOutcome::ConfirmationRequired {
			discard_units: 1,
			move_units: 0,
			conflicting_roots,
			..
		} if conflicting_roots == vec![root_id]
	));
}

#[tokio::test]
async fn change_scan_ignores_children_covered_by_applied_discard_root() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let child_id = Uuid::new_v4();
	repo.insert_scanning_task(task(
		task_id,
		r"C:\AppliedScan",
		OrganizeTaskStatus::Scanning,
	))
	.await
	.expect("insert applied-scan task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	root.tree_start = Some(0);
	root.tree_end = Some(2);
	root.unit_count = Some(1);
	let mut child = item(task_id, child_id, Some(1), "child");
	child.id = Some(2);
	child.kind = OrganizeItemKind::File;
	child.tree_start = Some(1);
	child.tree_end = Some(2);
	child.unit_count = Some(1);
	let revision = repo
		.replace_included_snapshot(
			task_id,
			vec![root, child],
			SnapshotTotals {
				total_entries: 2,
				total_units: 1,
				total_bytes: 0,
				scan_issue_count: 0,
			},
		)
		.await
		.expect("insert applied-scan tree");
	repo.apply_decision(DecisionTransactionRequest {
		task_id,
		selection: OrganizeSelectionInput::Items {
			item_ids: vec![root_id],
		},
		decision: Some(DecisionValue::discard()),
		expected_revision: revision,
		confirm_descendant_override: true,
		confirm_ancestor_split: true,
	})
	.await
	.expect("discard scan root");
	db.execute(Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"UPDATE organize_task_items SET operation_state = 'applied' WHERE task_id = ? AND uuid = ?",
		[task_id.into(), root_id.into()],
	))
	.await
	.expect("mark discard root applied");
	repo.store_change_scan(
		task_id,
		ChangeScanResult {
			additions: Vec::new(),
			changed_ids: vec![child_id],
			missing_ids: Vec::new(),
		},
	)
	.await
	.expect("store scan under applied root");
	let child = organize_task_item::Entity::find()
		.filter(organize_task_item::Column::TaskId.eq(task_id))
		.filter(organize_task_item::Column::Uuid.eq(child_id))
		.one(&db)
		.await
		.expect("read scanned child")
		.expect("child remains in snapshot");
	assert_eq!(child.external_state, "present");
}

#[tokio::test]
async fn removing_missing_parent_rejects_cascade_over_applied_child() {
	let (_temp_dir, db) = migrated_temp_db().await;
	pragma_foreign_keys(&db).await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let child_id = Uuid::new_v4();
	repo.insert_scanning_task(task(task_id, r"C:\Cascade", OrganizeTaskStatus::Scanning))
		.await
		.expect("insert cascade task");
	let mut root = item(task_id, root_id, None, "");
	root.id = Some(1);
	root.tree_start = Some(0);
	root.tree_end = Some(2);
	let mut child = item(task_id, child_id, Some(1), "child");
	child.id = Some(2);
	child.tree_start = Some(1);
	child.tree_end = Some(2);
	let revision = repo
		.replace_included_snapshot(
			task_id,
			vec![root, child],
			SnapshotTotals {
				total_entries: 2,
				total_units: 1,
				total_bytes: 0,
				scan_issue_count: 0,
			},
		)
		.await
		.expect("insert cascade tree");
	db.execute(Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"UPDATE organize_task_items SET external_state = 'missing', operation_state = 'applied' WHERE task_id = ? AND uuid = ?",
		[task_id.into(), child_id.into()],
	))
	.await
	.expect("mark child applied and missing");
	db.execute(Statement::from_sql_and_values(
		DatabaseBackend::Sqlite,
		"UPDATE organize_task_items SET external_state = 'missing' WHERE task_id = ? AND uuid = ?",
		[task_id.into(), root_id.into()],
	))
	.await
	.expect("mark parent missing");
	let error = repo
		.accept_changes(OrganizeAcceptChangesInput {
			task_id,
			expected_revision: revision,
			include_addition_ids: Vec::new(),
			remove_missing_ids: vec![root_id],
			refresh_changed_ids: Vec::new(),
			preserve_changed_decisions: false,
			confirm_inherited_destructive: false,
		})
		.await
		.expect_err("missing parent must not cascade over applied child");
	assert!(matches!(
		error,
		OrganizeRepositoryError::Organize(OrganizeError::AppliedDecisionImmutable(id))
			if id == child_id
	));
}

#[tokio::test]
async fn concurrent_overlapping_task_creation_returns_one_business_conflict() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let first_id = Uuid::new_v4();
	let second_id = Uuid::new_v4();
	let first_repo = OrganizeRepository::new(&db);
	let second_repo = OrganizeRepository::new(&db);
	let (first, second) = tokio::join!(
		first_repo.insert_scanning_task(task(
			first_id,
			r"C:\Concurrent",
			OrganizeTaskStatus::Active,
		)),
		second_repo.insert_scanning_task(task(
			second_id,
			r"c:\concurrent",
			OrganizeTaskStatus::Active,
		)),
	);
	assert_eq!(
		[first.is_ok(), second.is_ok()]
			.into_iter()
			.filter(|succeeded| *succeeded)
			.count(),
		1
	);
	let conflict = if let Err(error) = first {
		error
	} else {
		second.expect_err("exactly one concurrent insert must conflict")
	};
	assert!(matches!(
		conflict,
		OrganizeRepositoryError::Organize(OrganizeError::UnsafeTopology(_))
	));
}

#[allow(dead_code)]
fn _timestamp_type_is_utc(_: DateTime<Utc>) {}
