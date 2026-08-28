use chrono::Utc;
use sd_core::infra::db::migration::Migrator;
use sd_core::ops::organize::model::{
	DecisionValue, OrganizeItemKind, OrganizeOperationState, OrganizeTaskStatus,
};
use sd_core::ops::organize::repository::{
	DecisionTransactionRequest, NewOrganizeTask, OrganizeChildrenInput, OrganizeDecisionKind,
	OrganizeDecisionOutcome, OrganizeDecisionSource, OrganizeItemFilter, OrganizeItemSort,
	OrganizeRepository, OrganizeSelectionInput, OrganizeSortDirection, SnapshotItemDraft,
	SnapshotTotals,
};
use sea_orm::DatabaseConnection;
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

fn task(id: Uuid, root_path: &str) -> NewOrganizeTask {
	NewOrganizeTask {
		id,
		name: "Decision fixture".to_string(),
		root_path: root_path.to_string(),
		root_path_key: root_path.to_lowercase(),
		device_slug: "device".to_string(),
		volume_id: None,
		root_entry_uuid: None,
		status: OrganizeTaskStatus::Scanning,
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

#[allow(clippy::too_many_arguments)]
fn item(
	task_id: Uuid,
	id: i32,
	uuid: Uuid,
	parent_id: Option<i32>,
	relative_path: &str,
	kind: OrganizeItemKind,
	tree_start: i64,
	tree_end: i64,
	unit_count: i64,
	size_bytes: i64,
	aggregate_size_bytes: i64,
) -> SnapshotItemDraft {
	SnapshotItemDraft {
		id: Some(id),
		uuid,
		task_id,
		parent_id,
		entry_uuid: None,
		relative_path: relative_path.to_string(),
		relative_path_key: relative_path.to_lowercase(),
		name: relative_path
			.rsplit('\\')
			.next()
			.unwrap_or("root")
			.to_string(),
		extension: None,
		kind,
		size_bytes,
		aggregate_size_bytes,
		modified_at_100ns: 0,
		metadata_signature: format!("signature-{id}"),
		tree_start: Some(tree_start),
		tree_end: Some(tree_end),
		unit_count: Some(unit_count),
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

async fn install_tree(
	repo: &OrganizeRepository<'_>,
	task_id: Uuid,
	root_path: &str,
	items: Vec<SnapshotItemDraft>,
	total_units: i64,
	total_bytes: i64,
) -> i64 {
	repo.insert_scanning_task(task(task_id, root_path))
		.await
		.expect("insert scanning task");
	repo.replace_included_snapshot(
		task_id,
		items,
		SnapshotTotals {
			total_entries: 0,
			total_units,
			total_bytes,
			scan_issue_count: 0,
		},
	)
	.await
	.expect("install active tree")
}

#[tokio::test]
async fn mixed_descendant_confirmation_projects_every_category_and_unmarked_unit() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let keep_id = Uuid::new_v4();
	let move_id = Uuid::new_v4();
	let album_id = Uuid::new_v4();
	let unmarked_id = Uuid::new_v4();
	let discard_id = Uuid::new_v4();
	let revision = install_tree(
		&repo,
		task_id,
		r"C:\DecisionConfirm",
		vec![
			item(
				task_id,
				1,
				root_id,
				None,
				"",
				OrganizeItemKind::Directory,
				0,
				6,
				4,
				0,
				700,
			),
			item(
				task_id,
				2,
				keep_id,
				Some(1),
				"keep.jpg",
				OrganizeItemKind::File,
				1,
				2,
				1,
				100,
				100,
			),
			item(
				task_id,
				3,
				move_id,
				Some(1),
				"move.jpg",
				OrganizeItemKind::File,
				2,
				3,
				1,
				200,
				200,
			),
			item(
				task_id,
				4,
				album_id,
				Some(1),
				"album",
				OrganizeItemKind::Directory,
				3,
				6,
				2,
				0,
				400,
			),
			item(
				task_id,
				5,
				unmarked_id,
				Some(4),
				r"album\unmarked.jpg",
				OrganizeItemKind::File,
				4,
				5,
				1,
				150,
				150,
			),
			item(
				task_id,
				6,
				discard_id,
				Some(4),
				r"album\discard.jpg",
				OrganizeItemKind::File,
				5,
				6,
				1,
				250,
				250,
			),
		],
		4,
		700,
	)
	.await;

	for decision in [
		(keep_id, DecisionValue::keep()),
		(move_id, DecisionValue::move_to(r"C:\Archive")),
		(discard_id, DecisionValue::discard()),
	] {
		let current = repo.get_task_revision(task_id).await.unwrap();
		repo.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![decision.0],
			},
			decision: Some(decision.1),
			expected_revision: current,
			confirm_descendant_override: false,
			confirm_ancestor_split: false,
		})
		.await
		.expect("seed explicit decision");
	}
	let before_revision = repo.get_task_revision(task_id).await.unwrap();
	let before_decisions = repo.dump_decisions(task_id).await.unwrap();

	let outcome = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![root_id],
			},
			decision: Some(DecisionValue::discard()),
			expected_revision: before_revision,
			confirm_descendant_override: false,
			confirm_ancestor_split: false,
		})
		.await
		.expect("analyze parent override");

	assert!(matches!(
		outcome,
		OrganizeDecisionOutcome::ConfirmationRequired {
			keep_units: 1,
			discard_units: 1,
			move_units: 1,
			unmarked_units: 1,
			affected_bytes: 700,
			ref conflicting_roots,
			..
		} if conflicting_roots == &vec![keep_id, move_id]
	));
	assert_eq!(
		repo.get_task_revision(task_id).await.unwrap(),
		before_revision
	);
	assert_eq!(
		repo.dump_decisions(task_id).await.unwrap(),
		before_decisions
	);
	assert_eq!(revision, 1);
}

#[tokio::test]
async fn confirmed_parent_discard_collapses_descendants_and_exposes_one_destructive_root() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let first_id = Uuid::new_v4();
	let second_id = Uuid::new_v4();
	install_tree(
		&repo,
		task_id,
		r"C:\DecisionCollapse",
		vec![
			item(
				task_id,
				1,
				root_id,
				None,
				"",
				OrganizeItemKind::Directory,
				0,
				3,
				2,
				0,
				30,
			),
			item(
				task_id,
				2,
				first_id,
				Some(1),
				"first.jpg",
				OrganizeItemKind::File,
				1,
				2,
				1,
				10,
				10,
			),
			item(
				task_id,
				3,
				second_id,
				Some(1),
				"second.jpg",
				OrganizeItemKind::File,
				2,
				3,
				1,
				20,
				20,
			),
		],
		2,
		30,
	)
	.await;
	for item_id in [first_id, second_id] {
		let revision = repo.get_task_revision(task_id).await.unwrap();
		repo.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![item_id],
			},
			decision: Some(DecisionValue::discard()),
			expected_revision: revision,
			confirm_descendant_override: false,
			confirm_ancestor_split: false,
		})
		.await
		.expect("seed child discard");
	}
	let revision = repo.get_task_revision(task_id).await.unwrap();
	let outcome = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![root_id],
			},
			decision: Some(DecisionValue::discard()),
			expected_revision: revision,
			confirm_descendant_override: false,
			confirm_ancestor_split: false,
		})
		.await
		.expect("collapse child discards");
	assert!(matches!(
		outcome,
		OrganizeDecisionOutcome::Applied { ref affected_roots, .. }
			if affected_roots == &vec![root_id]
	));
	let summary = repo.get_task(task_id).await.unwrap().task;
	assert_eq!(summary.progress.total_units, 2);
	assert_eq!(summary.progress.discard_units, 2);
	assert_eq!(summary.progress.unmarked_units, 0);
	let roots = repo.compact_destructive_roots(task_id).await.unwrap();
	assert_eq!(roots.len(), 1);
	assert_eq!(roots[0].item_id, root_id);
	assert_eq!(roots[0].decision, OrganizeDecisionKind::Discard);
}

#[tokio::test]
async fn child_projection_reports_partial_progress_and_inherited_decision_source() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let album_id = Uuid::new_v4();
	let first_id = Uuid::new_v4();
	let second_id = Uuid::new_v4();
	install_tree(
		&repo,
		task_id,
		r"C:\DecisionProjection",
		vec![
			item(
				task_id,
				1,
				root_id,
				None,
				"",
				OrganizeItemKind::Directory,
				0,
				4,
				2,
				0,
				30,
			),
			item(
				task_id,
				2,
				album_id,
				Some(1),
				"album",
				OrganizeItemKind::Directory,
				1,
				4,
				2,
				0,
				30,
			),
			item(
				task_id,
				3,
				first_id,
				Some(2),
				r"album\first.jpg",
				OrganizeItemKind::File,
				2,
				3,
				1,
				10,
				10,
			),
			item(
				task_id,
				4,
				second_id,
				Some(2),
				r"album\second.jpg",
				OrganizeItemKind::File,
				3,
				4,
				1,
				20,
				20,
			),
		],
		2,
		30,
	)
	.await;
	repo.apply_decision(DecisionTransactionRequest {
		task_id,
		selection: OrganizeSelectionInput::Items {
			item_ids: vec![first_id],
		},
		decision: Some(DecisionValue::keep()),
		expected_revision: 1,
		confirm_descendant_override: false,
		confirm_ancestor_split: false,
	})
	.await
	.expect("keep first child");
	let root_children = repo
		.children(OrganizeChildrenInput {
			task_id,
			parent_item_id: root_id,
			cursor: None,
			limit: 10,
			sort: OrganizeItemSort::Name,
			direction: OrganizeSortDirection::Asc,
			filter: OrganizeItemFilter::All,
		})
		.await
		.expect("project album progress");
	let album = root_children
		.decision_projections
		.iter()
		.find(|projection| projection.item_id == album_id)
		.expect("album projection");
	assert_eq!(album.explicit_decision, None);
	assert_eq!(album.effective_decision, None);
	assert_eq!(album.progress.processed_units, 1);
	assert_eq!(album.progress.keep_units, 1);
	assert_eq!(album.progress.unmarked_units, 1);

	repo.apply_decision(DecisionTransactionRequest {
		task_id,
		selection: OrganizeSelectionInput::Items {
			item_ids: vec![album_id],
		},
		decision: Some(DecisionValue::move_to(r"C:\Archive")),
		expected_revision: 2,
		confirm_descendant_override: true,
		confirm_ancestor_split: false,
	})
	.await
	.expect("replace album descendants with move");
	let album_children = repo
		.children(OrganizeChildrenInput {
			task_id,
			parent_item_id: album_id,
			cursor: None,
			limit: 10,
			sort: OrganizeItemSort::Name,
			direction: OrganizeSortDirection::Asc,
			filter: OrganizeItemFilter::Move,
		})
		.await
		.expect("project inherited moves");
	assert_eq!(album_children.decision_projections.len(), 2);
	for child in album_children.decision_projections {
		assert_eq!(child.explicit_decision, None);
		assert_eq!(child.effective_decision, Some(OrganizeDecisionKind::Move));
		assert_eq!(
			child.decision_source,
			Some(OrganizeDecisionSource::Inherited {
				ancestor_item_id: album_id,
			})
		);
		assert_eq!(child.progress.processed_units, 1);
		assert_eq!(child.progress.move_units, 1);
		assert_eq!(child.operation_state, OrganizeOperationState::Pending);
		assert_eq!(
			child
				.move_destination
				.as_ref()
				.and_then(|path| path.as_physical())
				.map(|(_, path)| path.to_string_lossy().into_owned()),
			Some(r"c:\archive".to_string())
		);
	}
}

#[tokio::test]
async fn clearing_an_unmarked_parent_preserves_explicit_descendant_decisions() {
	let (_temp_dir, db) = migrated_temp_db().await;
	let repo = OrganizeRepository::new(&db);
	let task_id = Uuid::new_v4();
	let root_id = Uuid::new_v4();
	let child_id = Uuid::new_v4();
	install_tree(
		&repo,
		task_id,
		r"C:\DecisionClear",
		vec![
			item(
				task_id,
				1,
				root_id,
				None,
				"",
				OrganizeItemKind::Directory,
				0,
				2,
				1,
				0,
				10,
			),
			item(
				task_id,
				2,
				child_id,
				Some(1),
				"child.jpg",
				OrganizeItemKind::File,
				1,
				2,
				1,
				10,
				10,
			),
		],
		1,
		10,
	)
	.await;
	repo.apply_decision(DecisionTransactionRequest {
		task_id,
		selection: OrganizeSelectionInput::Items {
			item_ids: vec![child_id],
		},
		decision: Some(DecisionValue::keep()),
		expected_revision: 1,
		confirm_descendant_override: false,
		confirm_ancestor_split: false,
	})
	.await
	.expect("keep child");
	let before = repo.dump_decisions(task_id).await.unwrap();
	let outcome = repo
		.apply_decision(DecisionTransactionRequest {
			task_id,
			selection: OrganizeSelectionInput::Items {
				item_ids: vec![root_id],
			},
			decision: None,
			expected_revision: 2,
			confirm_descendant_override: false,
			confirm_ancestor_split: false,
		})
		.await
		.expect("clear unmarked parent");
	assert!(matches!(
		outcome,
		OrganizeDecisionOutcome::InheritedNoOp { .. }
	));
	assert_eq!(repo.get_task_revision(task_id).await.unwrap(), 2);
	assert_eq!(repo.dump_decisions(task_id).await.unwrap(), before);
}
