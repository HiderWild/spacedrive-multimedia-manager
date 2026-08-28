import type {OrganizeDecisionOutcome, SdPath, OrganizeTaskSummary} from '@sd/ts-client';
import {useLibraryMutation} from '../../contexts/SpacedriveContext';
import {buildMoveDestinationRows, type LocationMoveDestination, type PinnedMoveDestination, type RecentMoveDestination} from './decision/moveDestinations';
import {buildSetDecisionInput, type OrganizeSelectionState} from './decision/contracts';

export function OrganizeMovePicker(props: {
	recent: RecentMoveDestination[];
	locations: LocationMoveDestination[];
	pinned: PinnedMoveDestination[];
	task: Pick<OrganizeTaskSummary, 'id' | 'revision'>;
	selection: OrganizeSelectionState;
	onStale: () => void;
	onApplied: () => void;
	onConfirmationRequired?: (outcome: Extract<OrganizeDecisionOutcome, {ConfirmationRequired: unknown}>) => void;
	onBrowse: () => void;
}) {
	const rows = buildMoveDestinationRows(props);
	const setDecision = useLibraryMutation('organize.set_decision');

	const chooseDestination = async (destination: SdPath) => {
		const result = await setDecision.mutateAsync(buildSetDecisionInput(props.task.id, props.task.revision, props.selection, {Move: {destination}}));
		if ('StaleRevision' in result) return props.onStale();
		if ('ConfirmationRequired' in result) return props.onConfirmationRequired?.(result);
		props.onApplied();
	};

	return (
		<div role="listbox" aria-label="Move destination" className="space-y-1">
			{rows.map((row) => (
				<button
					key={row.key}
					className="flex w-full items-center rounded-md px-3 py-2 text-left text-sm hover:bg-app-hover"
					onClick={() => row.kind === 'browse' ? props.onBrowse() : void chooseDestination(row.destination)}
					role="option"
				>
					<span>{row.name}</span>
				</button>
			))}
		</div>
	);
}
