import {describe, expect, test} from 'bun:test';
import type {LegacyImportApi, LegacyOrganizeRecord, LegacyOrganizeState} from './types';
import {importLegacyState, normalizeLegacyPath, selectNonOverlappingLegacyStates} from './importLegacy';

const state = (key: string, directoryPath: string, items: LegacyOrganizeState['items'] = {}): LegacyOrganizeRecord => ({
	version: 1,
	key,
	directoryPath,
	updatedAt: '2026-06-05T15:00:00Z',
	items,
});

function fakeApi(overrides: Partial<LegacyImportApi> = {}): LegacyImportApi {
	return {
		createTask: async () => ({Created: {task_id: 'task-1', status: 'scanning', snapshot_job: {id: 'job-1', job_name: 'organize.snapshot'}}}),
		waitForJob: async () => undefined,
		getTask: async () => ({task: {revision: 1}, root_item_id: 'root-1'}),
		listChildren: async () => ({revision: 1, items: [], decision_projections: [], next_cursor: null, matching_child_count: 0}),
		setDecision: async (input) => ({Applied: {revision: input.expected_revision + 1, task_summary: {} as never, affected_roots: []}}),
		archiveLegacyState: async () => undefined,
		...overrides,
	};
}

describe('legacy organize import boundary', () => {
	test('normalizes Windows paths case-insensitively without changing the root', () => {
		expect(normalizeLegacyPath('C:\\Photos\\Sub\\..\\a.jpg')).toBe('c:/photos/a.jpg');
		expect(normalizeLegacyPath('C:/')).toBe('c:/');
	});

	test('drops duplicate and nested legacy roots deterministically', () => {
		const records = [
			state('nested', 'C:/Photos/Trips'),
			state('root', 'C:/Photos'),
			state('duplicate', 'c:\\photos'),
			state('other', 'D:/Photos'),
		];

		expect(selectNonOverlappingLegacyStates(records).map((record) => record.key)).toEqual(['duplicate', 'other']);
	});

	test('applies only explicit keep/discard decisions and archives after all mapped decisions', async () => {
		const calls: string[] = [];
		const api = fakeApi({
			listChildren: async () => ({
				revision: 1,
				items: [
					{uuid: 'keep-item', relative_path: 'keep.jpg', kind: 'file'} as never,
					{uuid: 'move-item', relative_path: 'move.jpg', kind: 'file'} as never,
				],
				decision_projections: [],
				next_cursor: null,
				matching_child_count: 2,
			}),
			setDecision: async (input) => {
				calls.push(JSON.stringify(input));
				return {Applied: {revision: input.expected_revision + 1, task_summary: {} as never, affected_roots: []}};
			},
			archiveLegacyState: async () => calls.push('archive'),
		});
	const result = await importLegacyState(state('dir-a', 'C:/Photos', {
			keep: {itemId: 'legacy-keep', path: 'C:/Photos/KEEP.jpg', name: 'KEEP.jpg', kind: 'File', decision: 'keep', updatedAt: 'now'},
			move: {itemId: 'legacy-move', path: 'C:/Photos/move.jpg', name: 'move.jpg', kind: 'File', decision: 'move', updatedAt: 'now'},
		}), api);

		expect(result.appliedItemIds).toEqual(['keep-item']);
		expect(result.unsupportedDecisions).toEqual([{path: 'C:/Photos/move.jpg', decision: 'move'}]);
		expect(result.archived).toBe(false);
		expect(calls).toHaveLength(1);
		expect(calls[0]).not.toBe('archive');
	});

	test('leaves the legacy record available when a backend step fails', async () => {
		let archived = false;
		const result = await importLegacyState(
			state('dir-a', 'C:/Photos', {
				keep: {itemId: 'legacy-keep', path: 'C:/Photos/keep.jpg', name: 'keep.jpg', kind: 'File', decision: 'keep', updatedAt: 'now'},
			}),
			fakeApi({
				waitForJob: async () => { throw new Error('snapshot failed'); },
				archiveLegacyState: async () => { archived = true; },
			}),
		);

		expect(result.archived).toBe(false);
		expect(result.failure).toMatchObject({key: 'dir-a', path: 'C:/Photos'});
		expect(archived).toBe(false);
	});

	test('archives after mapped decisions and reports missing paths without inventing decisions', async () => {
		let archived = false;
		const result = await importLegacyState(
			state('dir-a', 'C:/Photos', {
				keep: {itemId: 'legacy-keep', path: 'C:/Photos/keep.jpg', name: 'keep.jpg', kind: 'File', decision: 'keep', updatedAt: 'now'},
				missing: {itemId: 'legacy-missing', path: 'C:/Photos/missing.jpg', name: 'missing.jpg', kind: 'File', decision: 'discard', updatedAt: 'now'},
			}),
			fakeApi({
				listChildren: async () => ({revision: 1, items: [{uuid: 'keep-item', relative_path: 'keep.jpg', kind: 'file'} as never], decision_projections: [], next_cursor: null, matching_child_count: 1}),
				archiveLegacyState: async () => { archived = true; },
			}),
		);

		expect(result.appliedItemIds).toEqual(['keep-item']);
		expect(result.missingPaths).toEqual(['C:/Photos/missing.jpg']);
		expect(result.archived).toBe(true);
		expect(archived).toBe(true);
		expect(result.failure).toBeNull();
	});
});
