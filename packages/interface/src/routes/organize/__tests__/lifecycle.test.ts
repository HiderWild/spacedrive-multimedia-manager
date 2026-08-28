import type {OrganizeTaskSummary} from '@sd/ts-client';
import {describe, expect, test} from 'bun:test';
import {
	handleOrganizeOutcome,
	taskCapabilities
} from '../OrganizeLifecycleDialogs';

const task = (status: OrganizeTaskSummary['status']): OrganizeTaskSummary => ({
	id: 'task',
	name: 'Photos',
	root_path: 'C:\\Photos',
	root_sd_path: {Physical: {device_slug: 'local', path: 'C:\\Photos'}},
	status,
	revision: 4,
	snapshot_version: 1,
	total_entries: 1,
	total_bytes: 1,
	progress: {
		total_units: 1,
		processed_units: 1,
		keep_units: 1,
		discard_units: 0,
		move_units: 0,
		unmarked_units: 0
	},
	scan_issue_count: 0,
	pending_addition_count: 0,
	failed_operation_count: 0,
	changed_count: 0,
	missing_count: 0,
	scan_job_id: null,
	commit_job_id: null,
	last_error: null,
	completed_at: null
});

describe('organize lifecycle contract', () => {
	test('completed tasks only expose reopen and record deletion', () => {
		expect(taskCapabilities(task('completed'))).toEqual({
			decide: false,
			scan: false,
			retrySnapshot: false,
			commit: false,
			finish: false,
			reopen: true,
			deleteRecord: true
		});
	});

	test('committing tasks cannot start another mutation or delete the record', () => {
		expect(taskCapabilities(task('committing'))).toEqual({
			decide: false,
			scan: false,
			retrySnapshot: false,
			commit: false,
			finish: false,
			reopen: false,
			deleteRecord: false
		});
	});

	test('scanning and failed tasks do not expose decision or commit capabilities', () => {
		expect(taskCapabilities(task('scanning'))).toMatchObject({
			decide: false,
			commit: false,
			finish: false
		});
		expect(taskCapabilities(task('failed'))).toMatchObject({
			decide: false,
			commit: false,
			finish: false,
			retrySnapshot: true
		});
	});

	test('stale revision only requests refetch', () => {
		const calls: string[] = [];
		handleOrganizeOutcome(
			{StaleRevision: {current_revision: 5}},
			{
				refetch: () => calls.push('refetch'),
				notify: () => calls.push('notify')
			}
		);
		expect(calls).toEqual(['refetch']);
	});
});
