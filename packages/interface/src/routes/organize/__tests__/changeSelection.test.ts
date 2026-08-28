import {describe, expect, test} from 'bun:test';
import {
	buildAcceptChangesInput,
	partitionOrganizeChanges,
	type OrganizeChangeItem,
} from '../decision/changeSelection';

const item = (overrides: Partial<OrganizeChangeItem>): OrganizeChangeItem => ({
	uuid: 'item',
	name: 'Photo.jpg',
	relative_path: 'Photo.jpg',
	membership_state: 'included',
	external_state: 'present',
	...overrides,
});

describe('organize change selection', () => {
	test('partitions pending additions, changed items, and missing items', () => {
		expect(
			partitionOrganizeChanges([
				item({uuid: 'addition', membership_state: 'pending_addition'}),
				item({uuid: 'changed', external_state: 'changed'}),
				item({uuid: 'missing', external_state: 'missing'}),
			]),
		).toEqual({
			additions: ['addition'],
			changed: ['changed'],
			missing: ['missing'],
		});
	});

	test('builds a safe accept request without inherited destructive confirmation', () => {
		expect(
			buildAcceptChangesInput('task', 7, {
				additions: ['addition'],
				changed: ['changed'],
				missing: ['missing'],
			}),
		).toEqual({
			task_id: 'task',
			expected_revision: 7,
			include_addition_ids: ['addition'],
			remove_missing_ids: ['missing'],
			refresh_changed_ids: ['changed'],
			preserve_changed_decisions: true,
			confirm_inherited_destructive: false,
		});
	});
});
