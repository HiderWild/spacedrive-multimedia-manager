import type {OrganizeCommitPlanOutput} from '@sd/ts-client';
import {describe, expect, test} from 'bun:test';
import {
	buildCommitReview,
	buildOrganizeCommitInput,
	commitBlockReasonText
} from '../OrganizeCommitDialog';

const plan = (
	overrides: Partial<OrganizeCommitPlanOutput> = {}
): OrganizeCommitPlanOutput => ({
	revision: 9,
	move_groups: [],
	discard_roots: [],
	keep_units: 3,
	unmarked_units: 0,
	pending_addition_count: 0,
	changed_or_missing_roots: [],
	failed_operation_roots: [],
	unsafe_conflicts: [],
	can_commit: true,
	blocking_reasons: [],
	...overrides
});

describe('organize commit review contract', () => {
	test('uses the plan revision and requires permanent deletion confirmation', () => {
		const review = buildCommitReview(
			plan({
				discard_roots: [
					{
						item_id: 'discard',
						source: {
							Physical: {device_slug: 'local', path: 'C:\\old'}
						},
						units: 1,
						bytes: 10
					}
				],
				move_groups: [
					{
						destination: {
							Physical: {device_slug: 'local', path: 'C:\\sorted'}
						},
						roots: [],
						units: 2,
						bytes: 20
					}
				]
			})
		);
		expect(review).toMatchObject({
			revision: 9,
			canCommit: true,
			requiresPermanentDeleteConfirmation: true,
			discardRoots: [{item_id: 'discard'}],
			moveGroups: [{units: 2, bytes: 20}]
		});
		expect(buildOrganizeCommitInput('task', review, false)).toMatchObject({
			task_id: 'task',
			expected_revision: 9,
			permanent_delete_confirmed: false,
			move_conflict_policy: 'AutoModifyName',
			allow_current_subtree_drift: false
		});
	});

	test('drift confirmation is independent and blocked reasons remain visible', () => {
		const review = buildCommitReview(
			plan({
				can_commit: false,
				changed_or_missing_roots: ['changed'],
				blocking_reasons: [{ChangedOrMissing: {item_ids: ['changed']}}]
			})
		);
		expect(review).toMatchObject({
			canCommit: false,
			requiresDriftConfirmation: false,
			driftOnlyBlocked: false,
			blockingReasons: ['Changed or missing items must be reviewed.']
		});
		expect(
			buildOrganizeCommitInput('task', review, true, true)
				.allow_current_subtree_drift
		).toBe(true);
	});

	test('only current subtree drift can be explicitly overridden', () => {
		const review = buildCommitReview(
			plan({
				can_commit: false,
				blocking_reasons: [
					{CurrentSubtreeDrift: {item_ids: ['folder']}}
				]
			})
		);
		expect(review).toMatchObject({
			requiresDriftConfirmation: true,
			driftOnlyBlocked: true
		});
		expect(
			commitBlockReasonText({CurrentSubtreeDrift: {item_ids: ['folder']}})
		).toBe('The current source subtree contains unreviewed descendants.');
	});

	test('formats every backend block reason without inventing DTOs', () => {
		expect(commitBlockReasonText('NoPhysicalOperations')).toBe(
			'There are no file operations to commit.'
		);
		expect(commitBlockReasonText({PendingAdditions: {count: 2}})).toBe(
			'2 new items are still pending.'
		);
	});
});
