import {describe, expect, test} from 'bun:test';
import type {Model, OrganizeGetOutput, OrganizeProgressSummary, OrganizeTaskSummary} from '@sd/ts-client';
import type {LegacyImportApi, LegacyOrganizeRecord, LegacyOrganizeState} from './types';
import {importLegacyState, normalizeLegacyPath, selectNonOverlappingLegacyStates} from './importLegacy';

const state = (key: string, directoryPath: string, items: LegacyOrganizeState['items'] = {}): LegacyOrganizeRecord => ({
	version: 1,
	key,
	directoryPath,
	updatedAt: '2026-06-05T15:00:00Z',
	items,
});

const progress = (): OrganizeProgressSummary => ({total_units: 0, processed_units: 0, keep_units: 0, discard_units: 0, move_units: 0, unmarked_units: 0});

const taskSummary = (revision = 1): OrganizeTaskSummary => ({
	id: 'task-1',
	name: 'Photos',
	root_path: 'C:/Photos',
	root_sd_path: {Physical: {device_slug: 'local', path: 'C:/Photos'}},
	status: 'active',
	revision,
	snapshot_version: 1,
	total_entries: 0,
	total_bytes: 0,
	progress: progress(),
	scan_issue_count: 0,
	pending_addition_count: 0,
	failed_operation_count: 0,
	changed_count: 0,
	missing_count: 0,
	scan_job_id: null,
	commit_job_id: null,
	last_error: null,
	completed_at: null,
});

const model = (uuid: string, relativePath: string, kind = 'file'): Model => ({
	id: 1,
	uuid,
	task_id: 'task-1',
	parent_id: 1,
	entry_uuid: null,
	relative_path: relativePath,
	relative_path_key: relativePath.toLowerCase(),
	name: relativePath.split('/').at(-1) ?? relativePath,
	extension: null,
	kind,
	size_bytes: 1,
	aggregate_size_bytes: 1,
	modified_at_100ns: 1,
	metadata_signature: 'signature',
	tree_start: null,
	tree_end: null,
	unit_count: 1,
	membership_state: 'included',
	external_state: 'present',
	decision_kind: null,
	move_destination: null,
	operation_state: 'none',
	last_error: null,
	applied_at: null,
	created_at: '2026-06-05T15:00:00Z',
	updated_at: '2026-06-05T15:00:00Z',
});

const taskOutput = (): OrganizeGetOutput => ({task: taskSummary(), root_item_id: 'root-1'});

function fakeApi(overrides: Partial<LegacyImportApi> = {}): LegacyImportApi {
	return {
		createTask: async () => ({Created: {task_id: 'task-1', status: 'scanning', snapshot_job: {id: 'job-1', job_name: 'organize.snapshot'}}}),
		waitForJob: async () => undefined,
		getTask: async () => taskOutput(),
		listChildren: async () => ({revision: 1, items: [], decision_projections: [], next_cursor: null, matching_child_count: 0}),
		setDecision: async (input) => ({Applied: {revision: input.expected_revision + 1, task_summary: taskSummary(input.expected_revision + 1), affected_roots: []}}),
		archiveLegacyState: async (): Promise<void> => {},
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
					model('keep-item', 'keep.jpg'),
					model('move-item', 'move.jpg'),
				],
				decision_projections: [],
				next_cursor: null,
				matching_child_count: 2,
			}),
			setDecision: async (input) => {
				calls.push(JSON.stringify(input));
				return {Applied: {revision: input.expected_revision + 1, task_summary: taskSummary(input.expected_revision + 1), affected_roots: []}};
			},
			archiveLegacyState: async (): Promise<void> => { calls.push('archive'); },
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
				archiveLegacyState: async (): Promise<void> => { archived = true; },
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
				listChildren: async () => ({revision: 1, items: [model('keep-item', 'keep.jpg')], decision_projections: [], next_cursor: null, matching_child_count: 1}),
				archiveLegacyState: async (): Promise<void> => { archived = true; },
			}),
		);

		expect(result.appliedItemIds).toEqual(['keep-item']);
		expect(result.missingPaths).toEqual(['C:/Photos/missing.jpg']);
		expect(result.archived).toBe(true);
		expect(archived).toBe(true);
		expect(result.failure).toBeNull();
	});
});
