use super::error::OrganizeError;
use super::model::{
	DecisionPatch, DecisionResolution, DecisionTreeState, DecisionValue, ExplicitDecisionRoot,
	OrganizeDecisionConflictKind, OrganizeItemKind, OrganizeOperationState,
	OrganizeProgressSummary, TreeItemComputed, TreeItemDraft,
};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub fn compute_tree(items: &[TreeItemDraft]) -> Result<Vec<TreeItemComputed>, OrganizeError> {
	let mut indexes = HashMap::with_capacity(items.len());
	for (index, item) in items.iter().enumerate() {
		if indexes.insert(item.item_id, index).is_some() {
			return Err(OrganizeError::InvalidTree("duplicate item id".into()));
		}
	}

	let mut children: HashMap<Option<Uuid>, Vec<usize>> = HashMap::new();
	for (index, item) in items.iter().enumerate() {
		if let Some(parent) = item.parent_item_id {
			let Some(&parent_index) = indexes.get(&parent) else {
				return Err(OrganizeError::InvalidTree(
					"item refers to a missing parent".into(),
				));
			};
			if parent_index >= index {
				return Err(OrganizeError::InvalidTree(
					"parents must precede children in depth-first input".into(),
				));
			}
		}
		children.entry(item.parent_item_id).or_default().push(index);
	}
	if children.get(&None).map_or(0, Vec::len) != 1 {
		return Err(OrganizeError::InvalidTree(
			"the organize tree must contain exactly one root".into(),
		));
	}

	let mut order = Vec::with_capacity(items.len());
	fn visit_order(
		parent: Option<Uuid>,
		children: &HashMap<Option<Uuid>, Vec<usize>>,
		items: &[TreeItemDraft],
		order: &mut Vec<usize>,
	) {
		for &index in children.get(&parent).into_iter().flatten() {
			order.push(index);
			visit_order(Some(items[index].item_id), children, items, order);
		}
	}
	visit_order(None, &children, items, &mut order);
	if order.len() != items.len() {
		return Err(OrganizeError::InvalidTree(
			"tree contains a cycle or unreachable item".into(),
		));
	}
	if order.iter().copied().ne(0..items.len()) {
		return Err(OrganizeError::InvalidTree(
			"items are not in fixed depth-first order".into(),
		));
	}

	let mut computed: Vec<Option<TreeItemComputed>> = vec![None; items.len()];
	let mut cursor = 0_i64;
	fn visit(
		index: usize,
		items: &[TreeItemDraft],
		children: &HashMap<Option<Uuid>, Vec<usize>>,
		computed: &mut [Option<TreeItemComputed>],
		cursor: &mut i64,
	) -> Result<(u64, u64), OrganizeError> {
		let start = *cursor;
		*cursor += 1;
		let child_indexes = children
			.get(&Some(items[index].item_id))
			.cloned()
			.unwrap_or_default();
		if !child_indexes.is_empty() && items[index].kind != OrganizeItemKind::Directory {
			return Err(OrganizeError::InvalidTree(
				"only directories may contain children".into(),
			));
		}
		let mut units = 0_u64;
		let mut bytes = items[index].size_bytes;
		for child in child_indexes {
			let (child_units, child_bytes) = visit(child, items, children, computed, cursor)?;
			units = units.saturating_add(child_units);
			bytes = bytes.saturating_add(child_bytes);
		}
		if units == 0 {
			units = 1;
		}
		let end = *cursor;
		computed[index] = Some(TreeItemComputed {
			item_id: items[index].item_id,
			tree_start: start,
			tree_end: end,
			unit_count: units,
			aggregate_size_bytes: bytes,
		});
		Ok((units, bytes))
	}
	for &index in children.get(&None).into_iter().flatten() {
		visit(index, items, &children, &mut computed, &mut cursor)?;
	}
	Ok(computed.into_iter().map(Option::unwrap).collect())
}

pub fn normalize_selection(
	selected: &[Uuid],
	intervals: &HashMap<Uuid, (i64, i64)>,
) -> Result<Vec<Uuid>, OrganizeError> {
	let mut unique = HashSet::new();
	let mut entries = Vec::with_capacity(selected.len());
	for item_id in selected {
		if !unique.insert(*item_id) {
			continue;
		}
		let Some(&(start, end)) = intervals.get(item_id) else {
			return Err(OrganizeError::InvalidTree(
				"selection contains an unknown item".into(),
			));
		};
		if start < 0 || start >= end {
			return Err(OrganizeError::InvalidTree(
				"selection contains an invalid interval".into(),
			));
		}
		entries.push((*item_id, start, end));
	}
	entries.sort_by_key(|(_, start, end)| (*start, *end));
	let mut normalized = Vec::with_capacity(entries.len());
	let mut covered_until = None;
	for (item_id, start, end) in entries {
		if covered_until.is_some_and(|covered| end <= covered) {
			continue;
		}
		normalized.push(item_id);
		covered_until = Some(end.max(covered_until.unwrap_or(start)));
	}
	Ok(normalized)
}

pub fn resolve_set_decision(
	state: &DecisionTreeState,
	selected: &[Uuid],
	requested: Option<DecisionValue>,
	confirm_descendant_override: bool,
	confirm_ancestor_split: bool,
) -> Result<DecisionResolution, OrganizeError> {
	let intervals: HashMap<_, _> = state
		.nodes
		.iter()
		.map(|node| (node.item_id, (node.tree_start, node.tree_end)))
		.collect();
	let selected = normalize_selection(selected, &intervals)?;
	if selected.is_empty() {
		return Err(OrganizeError::InvalidTree("selection is empty".into()));
	}

	for root in &state.decisions {
		if root.operation_state == OrganizeOperationState::Applied
			&& selected.iter().any(|id| {
				intervals.get(id).is_some_and(|&(start, end)| {
					ranges_overlap(start, end, root.tree_start, root.tree_end)
				})
			}) {
			return Err(OrganizeError::AppliedDecisionImmutable(root.item_id));
		}
	}

	let mut delete_roots = Vec::new();
	let mut upsert_roots = Vec::new();
	let mut inherited_ancestor = None;
	for item_id in selected {
		let (start, end) = intervals[&item_id];
		let descendants: Vec<&ExplicitDecisionRoot> = state
			.decisions
			.iter()
			.filter(|root| root.tree_start > start && root.tree_end <= end)
			.collect();
		let ancestors: Vec<&ExplicitDecisionRoot> = state
			.decisions
			.iter()
			.filter(|root| root.tree_start < start && end <= root.tree_end)
			.collect();

		if let Some(ancestor) = ancestors.iter().max_by_key(|root| root.tree_start) {
			if requested.as_ref() == Some(&ancestor.decision) {
				inherited_ancestor.get_or_insert(ancestor.item_id);
				continue;
			}
			if !confirm_ancestor_split {
				return Ok(conflict(
					OrganizeDecisionConflictKind::AncestorSplit,
					std::slice::from_ref(ancestor),
				));
			}
			delete_roots.push(ancestor.item_id);
		}

		let has_mixed_descendants = requested.as_ref().is_some_and(|request| {
			descendants.iter().any(|root| root.decision != *request)
				&& descendants.iter().any(|root| root.decision == *request)
		});
		let has_conflicting_descendants = requested
			.as_ref()
			.is_some_and(|request| descendants.iter().any(|root| root.decision != *request));
		if (has_mixed_descendants || has_conflicting_descendants) && !confirm_descendant_override {
			return Ok(conflict(
				OrganizeDecisionConflictKind::DescendantOverride,
				&descendants,
			));
		}
		delete_roots.extend(descendants.iter().map(|root| root.item_id));

		if let Some(decision) = requested.clone() {
			let node = state
				.nodes
				.iter()
				.find(|node| node.item_id == item_id)
				.ok_or_else(|| OrganizeError::InvalidTree("selected node disappeared".into()))?;
			upsert_roots.push(ExplicitDecisionRoot {
				item_id,
				tree_start: node.tree_start,
				tree_end: node.tree_end,
				unit_count: node.unit_count,
				aggregate_size_bytes: node.aggregate_size_bytes,
				decision,
				operation_state: OrganizeOperationState::None,
			});
		}
	}
	if delete_roots.is_empty() && upsert_roots.is_empty() {
		if let Some(ancestor_item_id) = inherited_ancestor {
			return Ok(DecisionResolution::InheritedNoOp { ancestor_item_id });
		}
	}

	delete_roots.sort_unstable();
	delete_roots.dedup();
	Ok(DecisionResolution::Apply(DecisionPatch {
		delete_roots,
		upsert_roots,
	}))
}

fn conflict(
	conflict_kind: OrganizeDecisionConflictKind,
	roots: &[&ExplicitDecisionRoot],
) -> DecisionResolution {
	let mut result = OrganizeProgressSummary::default();
	let mut conflicting_roots = Vec::with_capacity(roots.len());
	for root in roots {
		conflicting_roots.push(root.item_id);
		result.processed_units = result.processed_units.saturating_add(root.unit_count);
		match root.decision {
			DecisionValue::Keep => {
				result.keep_units = result.keep_units.saturating_add(root.unit_count)
			}
			DecisionValue::Discard => {
				result.discard_units = result.discard_units.saturating_add(root.unit_count)
			}
			DecisionValue::Move { .. } => {
				result.move_units = result.move_units.saturating_add(root.unit_count)
			}
		}
	}
	DecisionResolution::ConfirmationRequired {
		conflict_kind,
		keep_units: result.keep_units,
		discard_units: result.discard_units,
		move_units: result.move_units,
		unmarked_units: result.unmarked_units,
		affected_bytes: roots.iter().map(|root| root.aggregate_size_bytes).sum(),
		conflicting_roots,
	}
}

pub fn reduce_progress(
	nodes: &[TreeItemComputed],
	decisions: &[ExplicitDecisionRoot],
) -> Result<OrganizeProgressSummary, OrganizeError> {
	let total_units = nodes
		.iter()
		.filter(|node| {
			!nodes.iter().any(|ancestor| {
				ancestor.item_id != node.item_id
					&& ancestor.tree_start <= node.tree_start
					&& node.tree_end <= ancestor.tree_end
			})
		})
		.map(|node| node.unit_count)
		.sum();
	let compacted = compact_operation_roots(decisions);
	let mut summary = OrganizeProgressSummary {
		total_units,
		..Default::default()
	};
	for root in compacted {
		if root.unit_count == 0 || root.tree_start >= root.tree_end {
			return Err(OrganizeError::InvalidTree(
				"decision root has invalid interval or units".into(),
			));
		}
		summary.processed_units = summary.processed_units.saturating_add(root.unit_count);
		match root.decision {
			DecisionValue::Keep => summary.keep_units += root.unit_count,
			DecisionValue::Discard => summary.discard_units += root.unit_count,
			DecisionValue::Move { .. } => summary.move_units += root.unit_count,
		}
	}
	if summary.processed_units > summary.total_units {
		return Err(OrganizeError::InvalidTree(
			"decision roots exceed tree units".into(),
		));
	}
	summary.unmarked_units = summary.total_units - summary.processed_units;
	Ok(summary)
}

pub fn compact_operation_roots(decisions: &[ExplicitDecisionRoot]) -> Vec<ExplicitDecisionRoot> {
	let mut sorted = decisions.to_vec();
	sorted.sort_by_key(|root| (root.tree_start, std::cmp::Reverse(root.tree_end)));
	let mut compacted = Vec::with_capacity(sorted.len());
	for root in sorted {
		if compacted.iter().any(|kept: &ExplicitDecisionRoot| {
			kept.tree_start <= root.tree_start && root.tree_end <= kept.tree_end
		}) {
			continue;
		}
		compacted.push(root);
	}
	compacted.sort_by_key(|root| (root.decision.priority(), root.tree_start));
	compacted
}

fn ranges_overlap(left_start: i64, left_end: i64, right_start: i64, right_end: i64) -> bool {
	left_start < right_end && right_start < left_end
}

#[cfg(test)]
mod tests {
	use super::*;

	fn item(id: u128, parent: Option<u128>, kind: OrganizeItemKind, size: u64) -> TreeItemDraft {
		TreeItemDraft {
			item_id: Uuid::from_u128(id),
			parent_item_id: parent.map(Uuid::from_u128),
			kind,
			size_bytes: size,
		}
	}

	fn root_node(nodes: &[TreeItemComputed], id: u128) -> TreeItemComputed {
		nodes
			.iter()
			.find(|node| node.item_id == Uuid::from_u128(id))
			.unwrap()
			.clone()
	}

	#[test]
	fn units_do_not_double_count_non_empty_directories() {
		let tree = compute_tree(&[
			item(1, None, OrganizeItemKind::Directory, 0),
			item(2, Some(1), OrganizeItemKind::File, 10),
			item(3, Some(1), OrganizeItemKind::Directory, 0),
			item(4, Some(3), OrganizeItemKind::File, 20),
			item(5, Some(1), OrganizeItemKind::Directory, 0),
		])
		.unwrap();
		assert_eq!(root_node(&tree, 1).unit_count, 3);
		assert_eq!(root_node(&tree, 3).unit_count, 1);
		assert_eq!(root_node(&tree, 5).unit_count, 1);
	}

	#[test]
	fn selection_normalization_keeps_only_outermost_intervals() {
		let outer = Uuid::from_u128(1);
		let inner = Uuid::from_u128(2);
		let selected = normalize_selection(
			&[inner, outer, inner],
			&HashMap::from([(outer, (0, 4)), (inner, (1, 2))]),
		)
		.unwrap();
		assert_eq!(selected, vec![outer]);
	}

	#[test]
	fn compute_tree_rejects_multiple_roots() {
		let result = compute_tree(&[
			item(1, None, OrganizeItemKind::Directory, 0),
			item(2, None, OrganizeItemKind::Directory, 0),
		]);
		assert!(matches!(result, Err(OrganizeError::InvalidTree(_))));
	}

	#[test]
	fn selected_root_is_replaced_instead_of_treated_as_descendant_conflict() {
		let id = Uuid::from_u128(1);
		let node = TreeItemComputed {
			item_id: id,
			tree_start: 0,
			tree_end: 1,
			unit_count: 1,
			aggregate_size_bytes: 10,
		};
		let state = DecisionTreeState {
			nodes: vec![node],
			decisions: vec![ExplicitDecisionRoot {
				item_id: id,
				tree_start: 0,
				tree_end: 1,
				unit_count: 1,
				aggregate_size_bytes: 10,
				decision: DecisionValue::discard(),
				operation_state: OrganizeOperationState::None,
			}],
		};
		let result =
			resolve_set_decision(&state, &[id], Some(DecisionValue::keep()), false, false).unwrap();
		assert!(matches!(
			result,
			DecisionResolution::Apply(ref patch)
				if patch.delete_roots == vec![id]
					&& patch.upsert_roots.len() == 1
					&& patch.upsert_roots[0].decision == DecisionValue::keep()
		));
	}

	#[test]
	fn inherited_noop_does_not_discard_the_rest_of_a_batch() {
		let ancestor = Uuid::from_u128(1);
		let inherited = Uuid::from_u128(2);
		let sibling = Uuid::from_u128(3);
		let state = DecisionTreeState {
			nodes: vec![
				TreeItemComputed {
					item_id: ancestor,
					tree_start: 0,
					tree_end: 2,
					unit_count: 1,
					aggregate_size_bytes: 10,
				},
				TreeItemComputed {
					item_id: inherited,
					tree_start: 1,
					tree_end: 2,
					unit_count: 1,
					aggregate_size_bytes: 10,
				},
				TreeItemComputed {
					item_id: sibling,
					tree_start: 2,
					tree_end: 3,
					unit_count: 1,
					aggregate_size_bytes: 10,
				},
			],
			decisions: vec![ExplicitDecisionRoot {
				item_id: ancestor,
				tree_start: 0,
				tree_end: 2,
				unit_count: 1,
				aggregate_size_bytes: 10,
				decision: DecisionValue::keep(),
				operation_state: OrganizeOperationState::None,
			}],
		};
		let result = resolve_set_decision(
			&state,
			&[inherited, sibling],
			Some(DecisionValue::keep()),
			false,
			false,
		)
		.unwrap();
		assert!(matches!(
			result,
			DecisionResolution::Apply(ref patch)
				if patch.upsert_roots.len() == 1 && patch.upsert_roots[0].item_id == sibling
		));
	}

	#[test]
	fn move_decisions_compare_normalized_windows_destinations() {
		assert_eq!(
			DecisionValue::move_to(r"C:\Archive\"),
			DecisionValue::move_to(r"c:/archive")
		);
	}

	#[test]
	fn all_discard_descendants_collapse_but_mixed_descendants_confirm() {
		let parent = Uuid::from_u128(1);
		let child = Uuid::from_u128(2);
		let other = Uuid::from_u128(3);
		let nodes = vec![
			TreeItemComputed {
				item_id: parent,
				tree_start: 0,
				tree_end: 5,
				unit_count: 5,
				aggregate_size_bytes: 500,
			},
			TreeItemComputed {
				item_id: child,
				tree_start: 1,
				tree_end: 2,
				unit_count: 2,
				aggregate_size_bytes: 200,
			},
			TreeItemComputed {
				item_id: other,
				tree_start: 2,
				tree_end: 5,
				unit_count: 3,
				aggregate_size_bytes: 300,
			},
		];
		let state = DecisionTreeState {
			nodes,
			decisions: vec![
				ExplicitDecisionRoot {
					item_id: child,
					tree_start: 1,
					tree_end: 2,
					unit_count: 2,
					aggregate_size_bytes: 200,
					decision: DecisionValue::discard(),
					operation_state: OrganizeOperationState::None,
				},
				ExplicitDecisionRoot {
					item_id: other,
					tree_start: 2,
					tree_end: 5,
					unit_count: 3,
					aggregate_size_bytes: 300,
					decision: DecisionValue::discard(),
					operation_state: OrganizeOperationState::None,
				},
			],
		};
		let collapse = resolve_set_decision(
			&state,
			&[parent],
			Some(DecisionValue::discard()),
			false,
			false,
		)
		.unwrap();
		assert!(
			matches!(collapse, DecisionResolution::Apply(ref patch) if patch.delete_roots.len() == 2 && patch.upsert_roots.len() == 1)
		);

		let mixed_state = DecisionTreeState {
			decisions: vec![
				ExplicitDecisionRoot {
					item_id: child,
					tree_start: 1,
					tree_end: 2,
					unit_count: 2,
					aggregate_size_bytes: 200,
					decision: DecisionValue::keep(),
					operation_state: OrganizeOperationState::None,
				},
				ExplicitDecisionRoot {
					item_id: other,
					tree_start: 2,
					tree_end: 5,
					unit_count: 3,
					aggregate_size_bytes: 300,
					decision: DecisionValue::move_to(r"c:\archive"),
					operation_state: OrganizeOperationState::None,
				},
			],
			..state
		};
		let mixed = resolve_set_decision(
			&mixed_state,
			&[parent],
			Some(DecisionValue::discard()),
			false,
			false,
		)
		.unwrap();
		assert!(matches!(
			mixed,
			DecisionResolution::ConfirmationRequired {
				keep_units: 2,
				move_units: 3,
				affected_bytes: 500,
				..
			}
		));
	}

	#[test]
	fn ancestor_split_leaves_siblings_unmarked_and_applied_is_immutable() {
		let parent = Uuid::from_u128(1);
		let child = Uuid::from_u128(2);
		let sibling = Uuid::from_u128(3);
		let nodes = vec![
			TreeItemComputed {
				item_id: parent,
				tree_start: 0,
				tree_end: 3,
				unit_count: 2,
				aggregate_size_bytes: 20,
			},
			TreeItemComputed {
				item_id: child,
				tree_start: 1,
				tree_end: 2,
				unit_count: 1,
				aggregate_size_bytes: 10,
			},
			TreeItemComputed {
				item_id: sibling,
				tree_start: 2,
				tree_end: 3,
				unit_count: 1,
				aggregate_size_bytes: 10,
			},
		];
		let state = DecisionTreeState {
			nodes,
			decisions: vec![ExplicitDecisionRoot {
				item_id: parent,
				tree_start: 0,
				tree_end: 3,
				unit_count: 2,
				aggregate_size_bytes: 20,
				decision: DecisionValue::keep(),
				operation_state: OrganizeOperationState::None,
			}],
		};
		let split = resolve_set_decision(
			&state,
			&[child],
			Some(DecisionValue::discard()),
			false,
			true,
		)
		.unwrap();
		assert!(
			matches!(split, DecisionResolution::Apply(ref patch) if patch.delete_roots == vec![parent] && patch.upsert_roots[0].item_id == child)
		);

		let applied = DecisionTreeState {
			decisions: vec![ExplicitDecisionRoot {
				item_id: child,
				tree_start: 1,
				tree_end: 2,
				unit_count: 1,
				aggregate_size_bytes: 10,
				decision: DecisionValue::discard(),
				operation_state: OrganizeOperationState::Applied,
			}],
			..state
		};
		assert!(matches!(
			resolve_set_decision(&applied, &[child], Some(DecisionValue::keep()), true, true),
			Err(OrganizeError::AppliedDecisionImmutable(_))
		));
	}

	#[test]
	fn progress_counts_sparse_roots_once_and_orders_moves_first() {
		let nodes = vec![
			TreeItemComputed {
				item_id: Uuid::from_u128(1),
				tree_start: 0,
				tree_end: 4,
				unit_count: 3,
				aggregate_size_bytes: 30,
			},
			TreeItemComputed {
				item_id: Uuid::from_u128(2),
				tree_start: 1,
				tree_end: 2,
				unit_count: 1,
				aggregate_size_bytes: 10,
			},
			TreeItemComputed {
				item_id: Uuid::from_u128(3),
				tree_start: 2,
				tree_end: 4,
				unit_count: 2,
				aggregate_size_bytes: 20,
			},
		];
		let decisions = vec![
			ExplicitDecisionRoot {
				item_id: Uuid::from_u128(3),
				tree_start: 2,
				tree_end: 4,
				unit_count: 2,
				aggregate_size_bytes: 20,
				decision: DecisionValue::discard(),
				operation_state: OrganizeOperationState::None,
			},
			ExplicitDecisionRoot {
				item_id: Uuid::from_u128(2),
				tree_start: 1,
				tree_end: 2,
				unit_count: 1,
				aggregate_size_bytes: 10,
				decision: DecisionValue::move_to(r"c:\archive"),
				operation_state: OrganizeOperationState::None,
			},
		];
		let progress = reduce_progress(&nodes, &decisions).unwrap();
		assert_eq!(progress.total_units, 3);
		assert_eq!(progress.processed_units, 3);
		assert_eq!(progress.move_units, 1);
		assert_eq!(progress.discard_units, 2);
		let compacted = compact_operation_roots(&decisions);
		assert!(matches!(compacted[0].decision, DecisionValue::Move { .. }));
	}
}
