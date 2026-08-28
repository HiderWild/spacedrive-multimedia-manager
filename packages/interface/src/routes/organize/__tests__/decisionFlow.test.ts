import {describe, expect, test} from 'bun:test';
import {
	buildCommitInput,
	buildFinishInput,
	buildSetDecisionInput,
	conflictDialogModel,
	progressSegments,
	type OrganizeSelectionState,
} from '../decision';

const selection: OrganizeSelectionState = {
	kind: 'directChildren',
	parentItemId: 'parent',
	filter: 'Unmarked',
	excludedItemIds: new Set(['skip']),
	focusId: null,
	anchorId: null,
};

describe('organize decision contracts', () => {
	test('serializes the complete direct-children scope', () => {
		expect(buildSetDecisionInput('task', 8, selection, 'Discard')).toEqual({
			task_id: 'task',
			expected_revision: 8,
			selection: {DirectChildren: {parent_item_id: 'parent', filter: 'Unmarked', excluded_item_ids: ['skip']}},
			decision: 'Discard',
			confirm_descendant_override: false,
			confirm_ancestor_split: false,
		});
	});

	test('serializes the matching confirmation flag for a retried move', () => {
		expect(
			buildSetDecisionInput('task', 8, selection, {Move: {destination: {Physical: {device_slug: 'dev', path: 'C:/Sorted'}}}}, 'ancestor_split'),
		).toMatchObject({
			confirm_descendant_override: false,
			confirm_ancestor_split: true,
		});
	});

	test('keeps backend counts in conflict presentation', () => {
		const model = conflictDialogModel({ConfirmationRequired: {
			conflict_kind: 'descendant_override', keep_units: 4, discard_units: 2, move_units: 3,
			unmarked_units: 7, affected_bytes: 2048, conflicting_roots: ['a'],
		}});
		expect(model).toMatchObject({keepUnits: 4, moveUnits: 3, affectedBytes: 2048, destructive: true});
	});

	test('builds explicit commit and finish confirmations', () => {
		expect(buildCommitInput('task', 12, false)).toMatchObject({permanent_delete_confirmed: false, move_conflict_policy: 'AutoModifyName', allow_current_subtree_drift: false});
		expect(buildFinishInput('task', 12, true)).toEqual({task_id: 'task', expected_revision: 12, confirm_unmarked: true});
	});

	test('segments progress using backend categories', () => {
		expect(progressSegments({total_units: 10, processed_units: 7, keep_units: 2, discard_units: 3, move_units: 2, unmarked_units: 3}).map((segment) => segment.fraction)).toEqual([0.2, 0.3, 0.2, 0.3]);
	});
});
