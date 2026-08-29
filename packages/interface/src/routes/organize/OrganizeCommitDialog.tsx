import type {
	OrganizeCommitBlockReason,
	OrganizeCommitInput,
	OrganizeCommitPlanOutput,
	OrganizeTaskStatus,
	SdPath
} from '@sd/ts-client';
import {useEffect, useRef, useState, type KeyboardEvent} from 'react';

export interface CommitReview {
	revision: number;
	canCommit: boolean;
	requiresPermanentDeleteConfirmation: boolean;
	requiresDriftConfirmation: boolean;
	driftOnlyBlocked: boolean;
	blockingReasons: string[];
	discardRoots: OrganizeCommitPlanOutput['discard_roots'];
	moveGroups: OrganizeCommitPlanOutput['move_groups'];
}

type CurrentSubtreeDriftReason = Extract<
	OrganizeCommitBlockReason,
	{CurrentSubtreeDrift: {item_ids: string[]}}
>;

function isCurrentSubtreeDrift(
	reason: OrganizeCommitBlockReason
): reason is CurrentSubtreeDriftReason {
	return (
		typeof reason === 'object' &&
		reason !== null &&
		'CurrentSubtreeDrift' in reason
	);
}

export function commitBlockReasonText(
	reason: OrganizeCommitBlockReason
): string {
	if (reason === 'NoPhysicalOperations')
		return 'There are no file operations to commit.';
	if ('TaskNotActive' in reason)
		return `The task is ${reason.TaskNotActive.status} and cannot commit.`;
	if ('PendingAdditions' in reason)
		return `${reason.PendingAdditions.count} new items are still pending.`;
	if (isCurrentSubtreeDrift(reason))
		return 'The current source subtree contains unreviewed descendants.';
	if ('ChangedOrMissing' in reason)
		return 'Changed or missing items must be reviewed.';
	return `Unsafe topology blocks commit for ${reason.UnsafeTopology.conflicts.length} item(s).`;
}

function pathText(path: SdPath): string {
	if ('Physical' in path) return path.Physical.path;
	if ('Cloud' in path) return path.Cloud.path;
	if ('Content' in path) return path.Content.content_id;
	if ('Sidecar' in path)
		return `${path.Sidecar.content_id}/${path.Sidecar.kind}/${path.Sidecar.variant}`;
	return 'Unknown path';
}

export function buildCommitReview(
	plan: OrganizeCommitPlanOutput
): CommitReview {
	return {
		revision: plan.revision,
		canCommit: plan.can_commit,
		requiresPermanentDeleteConfirmation: plan.discard_roots.length > 0,
		requiresDriftConfirmation: plan.blocking_reasons.some(
			isCurrentSubtreeDrift
		),
		driftOnlyBlocked:
			!plan.can_commit &&
			plan.blocking_reasons.length > 0 &&
			plan.blocking_reasons.every(isCurrentSubtreeDrift),
		blockingReasons: plan.blocking_reasons.map(commitBlockReasonText),
		discardRoots: plan.discard_roots,
		moveGroups: plan.move_groups
	};
}

export function buildOrganizeCommitInput(
	taskId: string,
	review: CommitReview,
	permanentDeleteConfirmed: boolean,
	allowCurrentSubtreeDrift = false
): OrganizeCommitInput {
	return {
		task_id: taskId,
		expected_revision: review.revision,
		permanent_delete_confirmed: permanentDeleteConfirmed,
		move_conflict_policy: 'AutoModifyName',
		allow_current_subtree_drift: allowCurrentSubtreeDrift
	};
}

export interface OrganizeCommitDialogProps {
	plan: OrganizeCommitPlanOutput | undefined;
	open: boolean;
	onCancel: () => void;
	onConfirm: (input: OrganizeCommitInput) => void;
	taskId: string;
	taskStatus: OrganizeTaskStatus;
}

export function OrganizeCommitDialog({
	plan,
	open,
	onCancel,
	onConfirm,
	taskId,
	taskStatus
}: OrganizeCommitDialogProps) {
	if (!open || !plan) return null;
	const review = buildCommitReview(plan);
	return (
		<CommitReviewPanel
			review={review}
			taskId={taskId}
			taskStatus={taskStatus}
			onCancel={onCancel}
			onConfirm={onConfirm}
		/>
	);
}

function CommitReviewPanel({
	review,
	taskId,
	taskStatus,
	onCancel,
	onConfirm
}: {
	review: CommitReview;
	taskId: string;
	taskStatus: OrganizeTaskStatus;
	onCancel: () => void;
	onConfirm: (input: OrganizeCommitInput) => void;
}) {
	const [permanentDeleteConfirmed, setPermanentDeleteConfirmed] =
		useState(false);
	const [driftConfirmed, setDriftConfirmed] = useState(false);
	const cancelRef = useRef<HTMLButtonElement>(null);
	const dialogRef = useRef<HTMLDivElement>(null);
	const confirmed =
		(!review.requiresPermanentDeleteConfirmation ||
			permanentDeleteConfirmed) &&
		(!review.requiresDriftConfirmation || driftConfirmed);
	const canCommit = taskStatus === 'active';

	useEffect(() => {
		cancelRef.current?.focus();
	}, []);

	const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
		if (event.key === 'Escape') {
			event.preventDefault();
			onCancel();
			return;
		}
		if (event.key !== 'Tab' || !dialogRef.current) return;
		const focusable = [
			...dialogRef.current.querySelectorAll<HTMLElement>('button, input')
		].filter((element) => !element.hasAttribute('disabled'));
		if (focusable.length === 0) return;
		const first = focusable[0];
		const last = focusable[focusable.length - 1];
		if (event.shiftKey && document.activeElement === first) {
			event.preventDefault();
			last.focus();
		} else if (!event.shiftKey && document.activeElement === last) {
			event.preventDefault();
			first.focus();
		}
	};

	return (
		<div
			className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
			onPointerDown={(event) => {
				if (event.target === event.currentTarget) onCancel();
			}}
		>
			<div
				ref={dialogRef}
				role="dialog"
				data-testid="organize-commit-dialog"
				aria-modal="true"
				aria-labelledby="organize-commit-title"
				tabIndex={-1}
				onKeyDown={handleKeyDown}
				className="border-app-line bg-app-box max-h-[90vh] w-full max-w-2xl overflow-auto rounded-lg border p-4 shadow-xl"
			>
				<h2
					id="organize-commit-title"
					className="text-base font-semibold"
				>
					Review commit
				</h2>
				<p className="text-ink-faint mt-1 text-xs">
					Revision {review.revision}. This applies the task plan to
					the files.
				</p>
				{review.blockingReasons.length > 0 && (
					<ul className="mt-3 list-disc space-y-1 pl-5 text-sm text-amber-300">
						{review.blockingReasons.map((reason) => (
							<li key={reason}>{reason}</li>
						))}
					</ul>
				)}
				<div className="mt-3 space-y-2 text-sm">
					<h3 className="font-medium">Discard roots</h3>
					{review.discardRoots.length === 0 ? (
						<p className="text-ink-faint">None</p>
					) : (
						review.discardRoots.map((root) => (
							<p key={root.item_id} className="text-red-300">
								{pathText(root.source)} · {root.units} units ·{' '}
								{root.bytes} bytes
							</p>
						))
					)}
				</div>
				<div className="mt-3 space-y-2 text-sm">
					<h3 className="font-medium">Move groups</h3>
					{review.moveGroups.length === 0 ? (
						<p className="text-ink-faint">None</p>
					) : (
						review.moveGroups.map((group, index) => (
							<p
								key={`${index}-${group.units}`}
								className="text-accent"
							>
								{pathText(group.destination)} ·{' '}
								{group.roots.length} roots · {group.units} units
								· {group.bytes} bytes
							</p>
						))
					)}
				</div>
				{permanentDeleteConfirmed && (
					<p className="mt-3 text-sm text-red-300">
						Discarded files will be permanently deleted.
					</p>
				)}
				{driftConfirmed && (
					<p className="mt-2 text-sm text-amber-300">
						The source subtree changed since the snapshot.
					</p>
				)}
				{review.requiresPermanentDeleteConfirmation && (
					<label className="mt-3 flex gap-2 text-sm">
						<input
							type="checkbox"
							checked={permanentDeleteConfirmed}
							onChange={(event) =>
								setPermanentDeleteConfirmed(
									event.target.checked
								)
							}
						/>{' '}
						I understand discarded files will be permanently
						deleted.
					</label>
				)}
				{review.requiresDriftConfirmation && (
					<label className="mt-2 flex gap-2 text-sm">
						<input
							type="checkbox"
							checked={driftConfirmed}
							onChange={(event) =>
								setDriftConfirmed(event.target.checked)
							}
						/>{' '}
						I reviewed the changed or missing items and allow
						current subtree drift.
					</label>
				)}
				<div className="mt-4 flex justify-end gap-2">
					<button
						ref={cancelRef}
						type="button"
						onClick={onCancel}
						className="rounded px-3 py-1.5 text-sm"
					>
						Cancel
					</button>
					<button
						type="button"
						data-testid="organize-commit-plan"
						disabled={
							!canCommit ||
							(!review.canCommit &&
								!(review.driftOnlyBlocked && driftConfirmed)) ||
							!confirmed
						}
						onClick={() =>
							onConfirm(
								buildOrganizeCommitInput(
									taskId,
									review,
									permanentDeleteConfirmed,
									driftConfirmed
								)
							)
						}
						className="bg-accent rounded px-3 py-1.5 text-sm text-white disabled:opacity-50"
					>
						Commit plan
					</button>
				</div>
			</div>
		</div>
	);
}
