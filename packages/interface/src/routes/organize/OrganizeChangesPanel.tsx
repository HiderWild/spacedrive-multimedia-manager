import type {
	OrganizeAcceptChangesOutcome,
	OrganizeCommitPlanOutput,
	OrganizeTaskSummary
} from '@sd/ts-client';
import {useMemo, useState} from 'react';
import {useLibraryMutation} from '../../contexts/SpacedriveContext';
import {
	buildAcceptChangesInput,
	partitionOrganizeChanges,
	type OrganizeChangeItem
} from './decision/changeSelection';
import {buildCommitReview} from './OrganizeCommitDialog';

interface OrganizeChangesPanelProps {
	plan: OrganizeCommitPlanOutput | undefined;
	task: Pick<OrganizeTaskSummary, 'id' | 'revision' | 'status'>;
	items: OrganizeChangeItem[];
	onStale: () => void;
	onApplied: () => void;
}

export function OrganizeChangesPanel({
	plan,
	task,
	items,
	onStale,
	onApplied
}: OrganizeChangesPanelProps) {
	const acceptChanges = useLibraryMutation('organize.accept_changes');
	const [selected, setSelected] = useState<Set<string>>(new Set());
	const [confirmation, setConfirmation] = useState<
		| Extract<
				OrganizeAcceptChangesOutcome,
				{ConfirmationRequired: unknown}
		  >['ConfirmationRequired']
		| null
	>(null);
	const changeItems = useMemo(() => partitionOrganizeChanges(items), [items]);
	const selectedChanges = useMemo(
		() => ({
			additions: changeItems.additions.filter((id) => selected.has(id)),
			changed: changeItems.changed.filter((id) => selected.has(id)),
			missing: changeItems.missing.filter((id) => selected.has(id))
		}),
		[changeItems, selected]
	);

	if (!plan)
		return <p className="text-ink-dull text-sm">Loading commit plan…</p>;
	const review = buildCommitReview(plan);
	const selectableItems = items.filter(
		(item) =>
			item.membership_state === 'pending_addition' ||
			item.external_state === 'changed' ||
			item.external_state === 'missing'
	);
	const toggle = (id: string) => {
		setSelected((current) => {
			const next = new Set(current);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});
	};
	const submit = async (confirmInheritedDestructive = false) => {
		const result = await acceptChanges.mutateAsync(
			buildAcceptChangesInput(
				task.id,
				task.revision,
				selectedChanges,
				confirmInheritedDestructive
			)
		);
		if ('StaleRevision' in result) {
			setSelected(new Set());
			setConfirmation(null);
			onStale();
			return;
		}
		if ('ConfirmationRequired' in result) {
			setConfirmation(result.ConfirmationRequired);
			return;
		}
		setSelected(new Set());
		setConfirmation(null);
		onApplied();
	};
	const selectedCount =
		selectedChanges.additions.length +
		selectedChanges.changed.length +
		selectedChanges.missing.length;
	const canAccept = task.status === 'active';

	return (
		<section
			className="border-app-line bg-app-box/20 border-b px-4 py-2 text-xs"
			aria-label="Organize changes"
		>
			<div className="text-ink-dull flex flex-wrap gap-x-4 gap-y-1">
				<span>Revision {review.revision}</span>
				<span>{plan.discard_roots.length} discard roots</span>
				<span>{plan.move_groups.length} move groups</span>
				<span>{plan.keep_units} keep units</span>
				<span>{plan.unmarked_units} unmarked</span>
			</div>
			<div className="mt-2 flex flex-wrap gap-2 text-sm">
				<span>{plan.pending_addition_count} pending additions</span>
				<span>
					{plan.changed_or_missing_roots.length} changed or missing
					roots
				</span>
				<span>{selectedCount} selected</span>
			</div>
			{selectableItems.length > 0 && (
				<div className="mt-2 grid gap-1 md:grid-cols-3">
					{selectableItems.map((item) => (
						<label
							key={item.uuid}
							className="border-app-line flex items-start gap-2 rounded border px-2 py-1.5"
						>
							<input
								type="checkbox"
								checked={selected.has(item.uuid)}
								onChange={() => toggle(item.uuid)}
							/>
							<span className="min-w-0">
								<span className="block truncate">
									{item.name}
								</span>
								<span className="text-ink-faint block truncate">
									{item.membership_state ===
									'pending_addition'
										? 'Pending addition'
										: item.external_state === 'changed'
											? 'Changed'
											: 'Missing'}{' '}
									· {item.relative_path}
								</span>
							</span>
						</label>
					))}
				</div>
			)}
			{selectableItems.length === 0 &&
				(plan.pending_addition_count > 0 ||
					plan.changed_or_missing_roots.length > 0) && (
					<p className="mt-2 text-amber-300">
						Change details are not available for this directory
						page. Refresh the task to load visible items.
					</p>
				)}
			{selectedCount > 0 && (
				<button
					type="button"
					disabled={!canAccept || acceptChanges.isPending}
					onClick={() => void submit()}
					className="bg-accent mt-2 rounded px-3 py-1.5 text-xs text-white disabled:opacity-50"
				>
					{acceptChanges.isPending
						? 'Applying…'
						: 'Accept selected changes'}
				</button>
			)}
			{!canAccept && selectedCount > 0 && (
				<p className="mt-2 text-amber-300">
					Changes can be accepted only while the task is active.
					Current state: {task.status}.
				</p>
			)}
			{confirmation && (
				<div
					role="alertdialog"
					className="mt-2 rounded border border-amber-500/50 p-2 text-amber-200"
				>
					<p>
						Accepting these additions inherits destructive decisions
						for {confirmation.discard_units} discard units and{' '}
						{confirmation.move_units} move units (
						{confirmation.affected_bytes} bytes).
					</p>
					<div className="mt-2 flex gap-2">
						<button
							type="button"
							disabled={!canAccept || acceptChanges.isPending}
							onClick={() => void submit(true)}
							className="rounded bg-amber-600 px-2 py-1 text-xs text-white disabled:opacity-50"
						>
							Confirm and accept
						</button>
						<button
							type="button"
							onClick={() => setConfirmation(null)}
							className="border-app-line rounded border px-2 py-1 text-xs"
						>
							Cancel
						</button>
					</div>
				</div>
			)}
			{review.blockingReasons.length > 0 && (
				<ul className="mt-1 list-disc pl-4 text-amber-300">
					{review.blockingReasons.map((reason) => (
						<li key={reason}>{reason}</li>
					))}
				</ul>
			)}
		</section>
	);
}
