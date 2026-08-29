import type {
	OrganizeDecisionInput,
	OrganizeProgressSummary,
	OrganizeTaskSummary
} from '@sd/ts-client';
import {useState} from 'react';
import {useLibraryMutation} from '../../contexts/SpacedriveContext';
import {
	buildSetDecisionInput,
	conflictDialogModel,
	type OrganizeSelectionState
} from './decision/contracts';
import {OrganizeProgress} from './OrganizeProgress';

export function OrganizeDecisionBar(props: {
	task: OrganizeTaskSummary;
	selection: OrganizeSelectionState;
	progress: OrganizeProgressSummary;
	onStale: () => void;
	onApplied: () => void;
	onChooseMove: () => void;
}) {
	const [confirmation, setConfirmation] = useState<ReturnType<
		typeof conflictDialogModel
	> | null>(null);
	const [pendingDecision, setPendingDecision] =
		useState<OrganizeDecisionInput | null>(null);
	const setDecision = useLibraryMutation('organize.set_decision');
	// The backend accepts decisions only while the snapshot is active. Keep the
	// controls visible during scans and failures so the user can understand why
	// they cannot act, but prevent requests that the lifecycle will reject.
	const readOnly = props.task.status !== 'active';

	const submitDecision = async (
		decision: OrganizeDecisionInput | null,
		override?: ReturnType<typeof conflictDialogModel>
	) => {
		const input = buildSetDecisionInput(
			props.task.id,
			props.task.revision,
			props.selection,
			decision
		);
		if (override) {
			input.confirm_descendant_override =
				override.kind === 'descendant_override';
			input.confirm_ancestor_split = override.kind === 'ancestor_split';
		}
		const result = await setDecision.mutateAsync(input);
		if ('StaleRevision' in result) return props.onStale();
		if ('ConfirmationRequired' in result) {
			setPendingDecision(decision);
			return setConfirmation(conflictDialogModel(result));
		}
		setConfirmation(null);
		props.onApplied();
	};

	return (
		<section
			className="border-app-line space-y-3 border-b p-3"
			aria-label="Organize decisions"
			data-testid="organize-decision-bar"
		>
			<OrganizeProgress progress={props.progress} />
			<div className="flex flex-wrap gap-2">
				<button
					type="button"
					disabled={
						readOnly ||
						(props.selection.kind === 'items' &&
							props.selection.itemIds.size === 0)
					}
					onClick={() => void submitDecision('Keep')}
				>
					Keep
				</button>
				<button
					type="button"
					disabled={
						readOnly ||
						(props.selection.kind === 'items' &&
							props.selection.itemIds.size === 0)
					}
					onClick={() => void submitDecision('Discard')}
				>
					Discard
				</button>
				<button
					type="button"
					disabled={
						readOnly ||
						(props.selection.kind === 'items' &&
							props.selection.itemIds.size === 0)
					}
					onClick={props.onChooseMove}
				>
					Move…
				</button>
			</div>
			{confirmation && (
				<div
					role="alertdialog"
					data-testid="organize-decision-conflict"
					className="space-y-2 rounded-md border border-amber-500/40 p-3"
				>
					<p>
						This decision affects{' '}
						{confirmation.keepUnits +
							confirmation.discardUnits +
							confirmation.moveUnits}{' '}
						already reviewed units.
					</p>
					<button
						disabled={pendingDecision === null}
						onClick={() =>
							pendingDecision !== null &&
							void submitDecision(pendingDecision, confirmation)
						}
					>
						Confirm override
					</button>
					<button
						onClick={() => {
							setConfirmation(null);
							setPendingDecision(null);
						}}
					>
						Cancel
					</button>
				</div>
			)}
		</section>
	);
}
