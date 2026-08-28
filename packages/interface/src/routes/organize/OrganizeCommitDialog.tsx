import type {OrganizeCommitInput, OrganizeCommitPlanOutput, OrganizeCommitBlockReason} from '@sd/ts-client';
import {useState} from 'react';

export interface CommitReview {
	revision: number;
	canCommit: boolean;
	requiresPermanentDeleteConfirmation: boolean;
	requiresDriftConfirmation: boolean;
	blockingReasons: string[];
}

export function commitBlockReasonText(reason: OrganizeCommitBlockReason): string {
	if (reason === 'NoPhysicalOperations') return 'There are no file operations to commit.';
	if ('TaskNotActive' in reason) return `The task is ${reason.TaskNotActive.status} and cannot commit.`;
	if ('PendingAdditions' in reason) return `${reason.PendingAdditions.count} new items are still pending.`;
	if ('ChangedOrMissing' in reason) return 'Changed or missing items must be reviewed.';
	return `Unsafe topology blocks commit for ${reason.UnsafeTopology.conflicts.length} item(s).`;
}

export function buildCommitReview(plan: OrganizeCommitPlanOutput): CommitReview {
	return {
		revision: plan.revision,
		canCommit: plan.can_commit,
		requiresPermanentDeleteConfirmation: plan.discard_roots.length > 0,
		requiresDriftConfirmation: plan.changed_or_missing_roots.length > 0,
		blockingReasons: plan.blocking_reasons.map(commitBlockReasonText),
	};
}

export function buildOrganizeCommitInput(
	taskId: string,
	review: CommitReview,
	permanentDeleteConfirmed: boolean,
	allowCurrentSubtreeDrift = false,
): OrganizeCommitInput {
	return {
		task_id: taskId,
		expected_revision: review.revision,
		permanent_delete_confirmed: permanentDeleteConfirmed,
		move_conflict_policy: 'AutoModifyName',
		allow_current_subtree_drift: allowCurrentSubtreeDrift,
	};
}

export interface OrganizeCommitDialogProps {
	plan: OrganizeCommitPlanOutput | undefined;
	open: boolean;
	onCancel: () => void;
	onConfirm: (input: OrganizeCommitInput) => void;
	taskId: string;
}

export function OrganizeCommitDialog({plan, open, onCancel, onConfirm, taskId}: OrganizeCommitDialogProps) {
	if (!open || !plan) return null;
	const review = buildCommitReview(plan);
	return <CommitReviewPanel review={review} taskId={taskId} onCancel={onCancel} onConfirm={onConfirm} />;
}

function CommitReviewPanel({review, taskId, onCancel, onConfirm}: {review: CommitReview; taskId: string; onCancel: () => void; onConfirm: (input: OrganizeCommitInput) => void}) {
	const [permanentDeleteConfirmed, setPermanentDeleteConfirmed] = useState(false);
	const [driftConfirmed, setDriftConfirmed] = useState(false);
	const confirmed = (!review.requiresPermanentDeleteConfirmation || permanentDeleteConfirmed) && (!review.requiresDriftConfirmation || driftConfirmed);
	return <div role="dialog" aria-modal="true" className="rounded-lg border border-app-line bg-app-box p-4 shadow-xl">
		<h2 className="text-base font-semibold">Review commit</h2>
		<p className="mt-1 text-xs text-ink-faint">Revision {review.revision}. This applies the task plan to the files.</p>
		{review.blockingReasons.length > 0 && <ul className="mt-3 list-disc space-y-1 pl-5 text-sm text-amber-300">{review.blockingReasons.map((reason) => <li key={reason}>{reason}</li>)}</ul>}
		{permanentDeleteConfirmed && <p className="mt-3 text-sm text-red-300">Discarded files will be permanently deleted.</p>}
		{driftConfirmed && <p className="mt-2 text-sm text-amber-300">The source subtree changed since the snapshot.</p>}
		{review.requiresPermanentDeleteConfirmation && <label className="mt-3 flex gap-2 text-sm"><input type="checkbox" checked={permanentDeleteConfirmed} onChange={(event) => setPermanentDeleteConfirmed(event.target.checked)} /> I understand discarded files will be permanently deleted.</label>}
		{review.requiresDriftConfirmation && <label className="mt-2 flex gap-2 text-sm"><input type="checkbox" checked={driftConfirmed} onChange={(event) => setDriftConfirmed(event.target.checked)} /> I reviewed the changed or missing items and allow current subtree drift.</label>}
		<div className="mt-4 flex justify-end gap-2"><button type="button" onClick={onCancel} className="rounded px-3 py-1.5 text-sm">Cancel</button><button type="button" disabled={(!review.canCommit && !(review.requiresDriftConfirmation && driftConfirmed)) || !confirmed} onClick={() => onConfirm(buildOrganizeCommitInput(taskId, review, permanentDeleteConfirmed, driftConfirmed))} className="rounded bg-accent px-3 py-1.5 text-sm text-white disabled:opacity-50">Commit plan</button></div>
	</div>;
}
