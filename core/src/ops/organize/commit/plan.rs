use super::{
	OrganizeCommitBlockReason, OrganizeCommitPlanOutput, OrganizeMoveGroup, OrganizePlanRoot,
	OrganizeTopologyConflict,
};
use crate::domain::addressing::SdPath;
use crate::infra::db::entities::{organize_task, organize_task_item};
use crate::ops::organize::error::OrganizeError;
use crate::ops::organize::model::{
	DecisionValue, ExplicitDecisionRoot, OrganizeOperationState, OrganizeTaskStatus,
	TreeItemComputed,
};
use crate::ops::organize::tree::reduce_progress;
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct PlannedRoot {
	item_id: Uuid,
	relative_path: String,
	tree_start: i64,
	tree_end: i64,
	units: u64,
	bytes: u64,
	decision: DecisionValue,
	operation_state: OrganizeOperationState,
	destination: Option<String>,
}

/// Builds the read-only physical operation projection for one task revision.
///
/// The task manifest is the source of truth for this query. Keep decisions are
/// included in progress accounting but never become physical roots, while
/// Discard and Move roots are compacted independently so a damaged nested
/// decision cannot schedule the same physical subtree twice.
pub fn build_commit_plan(
	task: &organize_task::Model,
	items: &[organize_task_item::Model],
) -> Result<OrganizeCommitPlanOutput, OrganizeError> {
	let included = items
		.iter()
		.filter(|item| item.membership_state == "included")
		.collect::<Vec<_>>();
	let pending_addition_count = items
		.iter()
		.filter(|item| item.membership_state == "pending_addition")
		.count() as u64;

	let nodes = included
		.iter()
		.map(|item| {
			let (tree_start, tree_end) = item_interval(item)?;
			Ok(TreeItemComputed {
				item_id: item.uuid,
				tree_start,
				tree_end,
				unit_count: required_units(item.unit_count)?,
				aggregate_size_bytes: item.aggregate_size_bytes.max(0) as u64,
			})
		})
		.collect::<Result<Vec<_>, OrganizeError>>()?;

	let mut all_decisions = Vec::new();
	for item in &included {
		let Some(decision) = parse_decision(item)? else {
			continue;
		};
		let (tree_start, tree_end) = item_interval(item)?;
		all_decisions.push(PlannedRoot {
			item_id: item.uuid,
			relative_path: item.relative_path.clone(),
			tree_start,
			tree_end,
			units: required_units(item.unit_count)?,
			bytes: item.aggregate_size_bytes.max(0) as u64,
			decision,
			operation_state: parse_operation_state(&item.operation_state)?,
			destination: item.move_destination.clone(),
		});
	}

	let progress = reduce_progress(
		&nodes,
		&all_decisions
			.iter()
			.map(|root| ExplicitDecisionRoot {
				item_id: root.item_id,
				tree_start: root.tree_start,
				tree_end: root.tree_end,
				unit_count: root.units,
				aggregate_size_bytes: root.bytes,
				decision: root.decision.clone(),
				operation_state: root.operation_state,
			})
			.collect::<Vec<_>>(),
	)?;

	let physical_roots = all_decisions
		.iter()
		.filter(|root| root.operation_state != OrganizeOperationState::Applied)
		.cloned()
		.collect::<Vec<_>>();
	let discard_roots = compact_same_action(
		physical_roots
			.iter()
			.filter(|root| matches!(&root.decision, DecisionValue::Discard))
			.cloned()
			.collect(),
	)
	.into_iter()
	.map(|root| plan_root(task, &root))
	.collect::<Result<Vec<_>, OrganizeError>>()?;

	let mut move_builders = BTreeMap::<String, (String, Vec<PlannedRoot>)>::new();
	for root in physical_roots
		.iter()
		.filter(|root| matches!(&root.decision, DecisionValue::Move { .. }))
	{
		let destination = root
			.destination
			.clone()
			.ok_or_else(|| OrganizeError::InvalidTree("move decision has no destination".into()))?;
		let destination_key = topology_key(&destination);
		move_builders
			.entry(destination_key)
			.or_insert_with(|| (destination.clone(), Vec::new()))
			.1
			.push(root.clone());
	}

	let mut move_groups = Vec::with_capacity(move_builders.len());
	for (_destination_key, (destination, roots)) in move_builders {
		let roots = compact_same_action(roots);
		let plan_roots = roots
			.iter()
			.map(|root| plan_root(task, root))
			.collect::<Result<Vec<_>, OrganizeError>>()?;
		let (units, bytes) = totals(&plan_roots);
		move_groups.push(OrganizeMoveGroup {
			destination: SdPath::physical(task.device_slug.clone(), PathBuf::from(destination)),
			roots: plan_roots,
			units,
			bytes,
		});
	}

	let unsafe_conflicts = find_unsafe_conflicts(task, &move_groups, &discard_roots);
	let changed_or_missing_roots = sorted_ids(
		all_decisions
			.iter()
			.filter(|root| {
				root.operation_state != OrganizeOperationState::Applied
					&& items.iter().any(|item| {
						item.uuid == root.item_id
							&& matches!(item.external_state.as_str(), "changed" | "missing")
					})
			})
			.map(|root| (root.tree_start, root.item_id))
			.collect(),
	);
	let failed_operation_roots = sorted_ids(
		all_decisions
			.iter()
			.filter(|root| root.operation_state == OrganizeOperationState::Failed)
			.map(|root| (root.tree_start, root.item_id))
			.collect(),
	);

	let mut blocking_reasons = Vec::new();
	let status = parse_task_status(&task.status)?;
	if status != OrganizeTaskStatus::Active {
		blocking_reasons.push(OrganizeCommitBlockReason::TaskNotActive { status });
	}
	if pending_addition_count > 0 {
		blocking_reasons.push(OrganizeCommitBlockReason::PendingAdditions {
			count: pending_addition_count,
		});
	}
	if !changed_or_missing_roots.is_empty() {
		blocking_reasons.push(OrganizeCommitBlockReason::ChangedOrMissing {
			item_ids: changed_or_missing_roots.clone(),
		});
	}
	if !unsafe_conflicts.is_empty() {
		blocking_reasons.push(OrganizeCommitBlockReason::UnsafeTopology {
			conflicts: unsafe_conflicts.clone(),
		});
	}
	if move_groups.is_empty() && discard_roots.is_empty() {
		blocking_reasons.push(OrganizeCommitBlockReason::NoPhysicalOperations);
	}

	Ok(OrganizeCommitPlanOutput {
		revision: task.revision,
		move_groups,
		discard_roots,
		keep_units: progress.keep_units,
		unmarked_units: progress.unmarked_units,
		pending_addition_count,
		changed_or_missing_roots,
		failed_operation_roots,
		unsafe_conflicts,
		can_commit: blocking_reasons.is_empty(),
		blocking_reasons,
	})
}

fn compact_same_action(mut roots: Vec<PlannedRoot>) -> Vec<PlannedRoot> {
	roots.sort_by_key(|root| {
		(
			root.tree_start,
			std::cmp::Reverse(root.tree_end),
			root.item_id,
		)
	});
	let mut compacted = Vec::with_capacity(roots.len());
	for root in roots {
		if compacted.iter().any(|kept: &PlannedRoot| {
			kept.tree_start <= root.tree_start && root.tree_end <= kept.tree_end
		}) {
			continue;
		}
		compacted.push(root);
	}
	compacted
}

fn plan_root(
	task: &organize_task::Model,
	root: &PlannedRoot,
) -> Result<OrganizePlanRoot, OrganizeError> {
	Ok(OrganizePlanRoot {
		item_id: root.item_id,
		source: SdPath::physical(
			task.device_slug.clone(),
			PathBuf::from(join_windows_path(&task.root_path, &root.relative_path)),
		),
		units: root.units,
		bytes: root.bytes,
	})
}

fn find_unsafe_conflicts(
	task: &organize_task::Model,
	move_groups: &[OrganizeMoveGroup],
	discard_roots: &[OrganizePlanRoot],
) -> Vec<OrganizeTopologyConflict> {
	let discard_keys = discard_roots
		.iter()
		.filter_map(|root| physical_path(root).map(|path| topology_key(&path)))
		.collect::<Vec<_>>();
	let mut moves = Vec::new();
	for group in move_groups {
		let Some(destination) = physical_path_from_sd_path(&group.destination) else {
			continue;
		};
		for root in &group.roots {
			let Some(source) = physical_path(root) else {
				continue;
			};
			moves.push((
				root.item_id,
				source,
				destination.clone(),
				group.destination.clone(),
			));
		}
	}

	let mut conflicts = Vec::new();
	for (item_id, source, destination, destination_path) in &moves {
		let source_key = topology_key(source);
		let destination_key = topology_key(destination);
		let reason = if source_key == destination_key {
			Some("a move destination cannot equal its source")
		} else if is_ancestor_key(&source_key, &destination_key) {
			Some("a move destination cannot be inside its source")
		} else if discard_keys
			.iter()
			.any(|discard| paths_overlap(discard, &destination_key))
		{
			Some("a move destination overlaps a discard root")
		} else {
			None
		};
		if let Some(reason) = reason {
			conflicts.push(OrganizeTopologyConflict {
				item_id: *item_id,
				source: SdPath::physical(task.device_slug.clone(), PathBuf::from(source.as_str())),
				destination: destination_path.clone(),
				reason: reason.into(),
			});
		}
	}

	let source_to_destination = moves
		.iter()
		.map(|(item_id, source, destination, destination_path)| {
			(
				*item_id,
				topology_key(source),
				topology_key(destination),
				destination_path.clone(),
			)
		})
		.collect::<Vec<_>>();
	for (item_id, source, _destination, destination_path) in &source_to_destination {
		let mut current = source.clone();
		let mut visited = Vec::new();
		while let Some((_, _, next, _)) = source_to_destination
			.iter()
			.find(|(_, candidate, _, _)| candidate == &current)
		{
			if visited.iter().any(|seen| seen == &current) {
				conflicts.push(OrganizeTopologyConflict {
					item_id: *item_id,
					source: SdPath::physical(
						task.device_slug.clone(),
						PathBuf::from(source.as_str()),
					),
					destination: destination_path.clone(),
					reason: "move destinations form a cycle".into(),
				});
				break;
			}
			visited.push(current.clone());
			current = next.clone();
		}
	}
	conflicts.sort_by_key(|conflict| conflict.item_id);
	conflicts.dedup_by(|left, right| left.item_id == right.item_id && left.reason == right.reason);
	conflicts
}

fn parse_decision(
	item: &organize_task_item::Model,
) -> Result<Option<DecisionValue>, OrganizeError> {
	match item.decision_kind.as_deref() {
		None => Ok(None),
		Some("keep") => Ok(Some(DecisionValue::Keep)),
		Some("discard") => Ok(Some(DecisionValue::Discard)),
		Some("move") => Ok(Some(DecisionValue::Move {
			destination: item.move_destination.clone().ok_or_else(|| {
				OrganizeError::InvalidTree("move decision has no destination".into())
			})?,
		})),
		Some(kind) => Err(OrganizeError::InvalidTree(format!(
			"unknown decision kind: {kind}"
		))),
	}
}

fn parse_operation_state(value: &str) -> Result<OrganizeOperationState, OrganizeError> {
	match value {
		"none" => Ok(OrganizeOperationState::None),
		"pending" => Ok(OrganizeOperationState::Pending),
		"running" => Ok(OrganizeOperationState::Running),
		"applied" => Ok(OrganizeOperationState::Applied),
		"failed" => Ok(OrganizeOperationState::Failed),
		state => Err(OrganizeError::InvalidTree(format!(
			"unknown operation state: {state}"
		))),
	}
}

fn parse_task_status(value: &str) -> Result<OrganizeTaskStatus, OrganizeError> {
	match value {
		"scanning" => Ok(OrganizeTaskStatus::Scanning),
		"active" => Ok(OrganizeTaskStatus::Active),
		"committing" => Ok(OrganizeTaskStatus::Committing),
		"completed" => Ok(OrganizeTaskStatus::Completed),
		"failed" => Ok(OrganizeTaskStatus::Failed),
		status => Err(OrganizeError::InvalidTaskState(format!(
			"unknown task status: {status}"
		))),
	}
}

fn item_interval(item: &organize_task_item::Model) -> Result<(i64, i64), OrganizeError> {
	let start = item
		.tree_start
		.ok_or_else(|| OrganizeError::InvalidTree("included item has no tree start".into()))?;
	let end = item
		.tree_end
		.ok_or_else(|| OrganizeError::InvalidTree("included item has no tree end".into()))?;
	if start < 0 || start >= end {
		return Err(OrganizeError::InvalidTree(
			"included item has an invalid tree interval".into(),
		));
	}
	Ok((start, end))
}

fn required_units(value: Option<i64>) -> Result<u64, OrganizeError> {
	value
		.ok_or_else(|| OrganizeError::InvalidTree("included item has no unit count".into()))?
		.try_into()
		.map_err(|_| OrganizeError::InvalidTree("included item has invalid unit count".into()))
}

fn totals(roots: &[OrganizePlanRoot]) -> (u64, u64) {
	(
		roots.iter().map(|root| root.units).sum(),
		roots.iter().map(|root| root.bytes).sum(),
	)
}

fn sorted_ids(mut ids: Vec<(i64, Uuid)>) -> Vec<Uuid> {
	ids.sort_unstable();
	ids.into_iter().map(|(_, item_id)| item_id).collect()
}

fn join_windows_path(root: &str, relative: &str) -> String {
	if relative.is_empty() {
		return root.to_string();
	}
	format!(
		"{}\\{}",
		root.trim_end_matches(['\\', '/']),
		relative.trim_matches(['\\', '/']).replace('/', "\\")
	)
}

fn physical_path(root: &OrganizePlanRoot) -> Option<String> {
	physical_path_from_sd_path(&root.source)
}

fn physical_path_from_sd_path(path: &SdPath) -> Option<String> {
	match path {
		SdPath::Physical { path, .. } => Some(path.to_string_lossy().into_owned()),
		_ => None,
	}
}

fn topology_key(path: &str) -> String {
	let mut path = path.replace('/', "\\").to_lowercase();
	if let Some(unc) = path.strip_prefix(r"\\?\unc\") {
		path = format!(r"\\{unc}");
	} else if let Some(without_prefix) = path.strip_prefix(r"\\?\") {
		path = without_prefix.to_string();
	}
	let unc = path.starts_with(r"\\");
	let root_components = if unc { 2 } else { 1 };
	let mut components = Vec::new();
	for component in path.split('\\') {
		if component.is_empty() || component == "." {
			continue;
		}
		if component == ".." {
			if components.len() > root_components {
				components.pop();
			}
			continue;
		}
		components.push(component.to_string());
	}
	if unc {
		format!(r"\\{}", components.join("\\"))
	} else {
		components.join("\\")
	}
}

fn paths_overlap(left: &str, right: &str) -> bool {
	left == right || is_ancestor_key(left, right) || is_ancestor_key(right, left)
}

fn is_ancestor_key(ancestor: &str, descendant: &str) -> bool {
	ancestor != descendant
		&& descendant
			.strip_prefix(ancestor)
			.is_some_and(|suffix| suffix.starts_with('\\'))
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;

	fn task(id: Uuid) -> organize_task::Model {
		let now = Utc::now();
		organize_task::Model {
			id,
			name: "Photos".into(),
			root_path: r"C:\Photos".into(),
			root_path_key: r"c:\photos".into(),
			device_slug: "device".into(),
			volume_id: None,
			root_entry_uuid: None,
			status: "active".into(),
			revision: 7,
			snapshot_version: 1,
			total_entries: 4,
			total_units: 2,
			total_bytes: 10,
			scan_issue_count: 0,
			pending_addition_count: 0,
			scan_job_id: None,
			commit_job_id: None,
			last_error: None,
			created_at: now,
			updated_at: now,
			completed_at: None,
		}
	}

	fn item(
		task_id: Uuid,
		id: i32,
		path: &str,
		kind: &str,
		start: i64,
		end: i64,
		units: i64,
		bytes: i64,
		decision_kind: Option<&str>,
		move_destination: Option<&str>,
	) -> organize_task_item::Model {
		let now = Utc::now();
		organize_task_item::Model {
			id,
			uuid: Uuid::new_v4(),
			task_id,
			parent_id: None,
			entry_uuid: None,
			relative_path: path.into(),
			relative_path_key: path.to_lowercase(),
			name: path.rsplit('\\').next().unwrap_or(path).into(),
			extension: None,
			kind: kind.into(),
			size_bytes: bytes,
			aggregate_size_bytes: bytes,
			modified_at_100ns: 0,
			metadata_signature: "signature".into(),
			tree_start: Some(start),
			tree_end: Some(end),
			unit_count: Some(units),
			membership_state: "included".into(),
			external_state: "present".into(),
			decision_kind: decision_kind.map(str::to_string),
			move_destination: move_destination.map(str::to_string),
			operation_state: if decision_kind.is_some() {
				"pending".into()
			} else {
				"none".into()
			},
			last_error: None,
			applied_at: None,
			created_at: now,
			updated_at: now,
		}
	}

	#[test]
	fn compacts_nested_discard_roots_and_excludes_keep_from_physical_work() {
		let task_id = Uuid::new_v4();
		let task = task(task_id);
		let root = item(task_id, 1, "", "directory", 0, 4, 2, 10, None, None);
		let discard = item(
			task_id,
			2,
			"discard",
			"directory",
			1,
			3,
			1,
			7,
			Some("discard"),
			None,
		);
		let nested_discard = item(
			task_id,
			3,
			"discard\\nested.jpg",
			"file",
			2,
			3,
			1,
			7,
			Some("discard"),
			None,
		);
		let keep = item(
			task_id,
			4,
			"keep.jpg",
			"file",
			3,
			4,
			1,
			3,
			Some("keep"),
			None,
		);
		let plan = build_commit_plan(&task, &[root, discard, nested_discard, keep]).unwrap();

		assert_eq!(plan.discard_roots.len(), 1);
		assert_eq!(
			plan.discard_roots[0].source,
			SdPath::physical("device".into(), r"C:\Photos\discard")
		);
		assert!(plan.move_groups.is_empty());
		assert_eq!(plan.keep_units, 1);
		assert!(plan.can_commit);
	}

	#[test]
	fn groups_equivalent_move_destinations_without_scheduling_keep() {
		let task_id = Uuid::new_v4();
		let task = task(task_id);
		let root = item(task_id, 1, "", "directory", 0, 3, 2, 10, None, None);
		let move_a = item(
			task_id,
			2,
			"a.jpg",
			"file",
			1,
			2,
			1,
			4,
			Some("move"),
			Some(r"C:/Archive/2026"),
		);
		let move_b = item(
			task_id,
			3,
			"b.jpg",
			"file",
			2,
			3,
			1,
			6,
			Some("move"),
			Some(r"c:\archive\.\2026"),
		);
		let plan = build_commit_plan(&task, &[root, move_a, move_b]).unwrap();

		assert_eq!(plan.move_groups.len(), 1);
		assert_eq!(plan.move_groups[0].roots.len(), 2);
		assert_eq!(plan.move_groups[0].units, 2);
		assert!(plan.discard_roots.is_empty());
	}
}
