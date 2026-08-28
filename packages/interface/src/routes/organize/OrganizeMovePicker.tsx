import type {SdPath, OrganizeTaskSummary} from '@sd/ts-client';
import {useState} from 'react';
import {useLibraryMutation} from '../../contexts/SpacedriveContext';
import {buildMoveDestinationRows, type LocationMoveDestination, type PinnedMoveDestination, type RecentMoveDestination} from './decision/moveDestinations';
import {buildSetDecisionInput, conflictDialogModel, type OrganizeSelectionState} from './decision/contracts';

export function OrganizeMovePicker(props: {
	locations: LocationMoveDestination[];
	recent?: RecentMoveDestination[];
	pinned?: PinnedMoveDestination[];
	task: Pick<OrganizeTaskSummary, 'id' | 'revision'>;
	selection: OrganizeSelectionState;
	onStale: () => void;
	onApplied: () => void;
	onBrowse: () => void;
	browseAvailable?: boolean;
}) {
	const rows = buildMoveDestinationRows({
		recent: props.recent ?? [],
		locations: props.locations,
		pinned: props.pinned ?? [],
	});
	const setDecision = useLibraryMutation('organize.set_decision');
	const [confirmation, setConfirmation] = useState<ReturnType<typeof conflictDialogModel> | null>(null);
	const [pendingDestination, setPendingDestination] = useState<SdPath | null>(null);

	const chooseDestination = async (
		destination: SdPath,
		confirmationKind?: ReturnType<typeof conflictDialogModel>['kind'],
	) => {
		const result = await setDecision.mutateAsync(buildSetDecisionInput(props.task.id, props.task.revision, props.selection, {Move: {destination}}, confirmationKind));
		if ('StaleRevision' in result) {
			setConfirmation(null);
			setPendingDestination(null);
			return props.onStale();
		}
		if ('ConfirmationRequired' in result) {
			setPendingDestination(destination);
			return setConfirmation(conflictDialogModel(result));
		}
		setConfirmation(null);
		setPendingDestination(null);
		props.onApplied();
	};
	const confirmDestination = () => {
		if (!pendingDestination || !confirmation) return;
		void chooseDestination(pendingDestination, confirmation.kind);
	};

	return (
		<div role="listbox" aria-label="Move destination" className="space-y-1">
			{rows.map((row) => (
				<button
					key={row.key}
					disabled={row.kind === 'browse' && props.browseAvailable === false}
					className="flex w-full items-center rounded-md px-3 py-2 text-left text-sm hover:bg-app-hover"
					onClick={() => row.kind === 'browse' ? props.onBrowse() : void chooseDestination(row.destination)}
					role="option"
				>
					<span>{row.name}</span>
				</button>
			))}
			{confirmation && (
				<div role="alertdialog" aria-label="Confirm move override" className="rounded-md border border-amber-500/50 p-3 text-sm text-amber-200">
					<p>This move affects existing decisions and needs explicit confirmation.</p>
					<ul className="mt-2 space-y-1 text-xs">
						<li>Keep units: {confirmation.keepUnits}</li>
						<li>Discard units: {confirmation.discardUnits}</li>
						<li>Move units: {confirmation.moveUnits}</li>
						<li>Unmarked units: {confirmation.unmarkedUnits}</li>
						<li>Affected bytes: {confirmation.affectedBytes}</li>
					</ul>
					<div className="mt-3 flex gap-2">
						<button type="button" disabled={setDecision.isPending} onClick={confirmDestination} className="rounded bg-amber-600 px-2 py-1 text-xs text-white disabled:opacity-50">Confirm move</button>
						<button type="button" disabled={setDecision.isPending} onClick={() => { setConfirmation(null); setPendingDestination(null); }} className="rounded border border-app-line px-2 py-1 text-xs">Cancel</button>
					</div>
				</div>
			)}
		</div>
	);
}
