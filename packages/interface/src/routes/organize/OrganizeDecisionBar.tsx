import {useState} from 'react';
import type {
	OrganizeDecisionInput,
	OrganizeProgressSummary,
	OrganizeTaskSummary,
} from '@sd/ts-client';
import {useLibraryMutation, useLibraryQuery} from '../../contexts/SpacedriveContext';
import {buildCommitInput, buildFinishInput, buildSetDecisionInput, conflictDialogModel, type OrganizeSelectionState} from './decision/contracts';
import {OrganizeProgress} from './OrganizeProgress';

export function OrganizeDecisionBar(props: {
	task: OrganizeTaskSummary;
	selection: OrganizeSelectionState;
	progress: OrganizeProgressSummary;
	onStale: () => void;
	onApplied: () => void;
	onChooseMove: () => void;
}) {
	const [confirmation, setConfirmation] = useState<ReturnType<typeof conflictDialogModel> | null>(null);
	const [pendingDecision, setPendingDecision] = useState<OrganizeDecisionInput | null>(null);
	const [permanentDeleteConfirmed, setPermanentDeleteConfirmed] = useState(false);
	const [confirmUnmarked, setConfirmUnmarked] = useState(false);
	const setDecision = useLibraryMutation('organize.set_decision');
	const commitPlan = useLibraryQuery({type: 'organize.commit_plan', input: {task_id: props.task.id, expected_revision: props.task.revision}});
	const commit = useLibraryMutation('organize.commit');
	const finish = useLibraryMutation('organize.finish');
	const readOnly = props.task.status === 'completed' || props.task.status === 'committing';

	const submitDecision = async (decision: OrganizeDecisionInput | null, override?: ReturnType<typeof conflictDialogModel>) => {
		const input = buildSetDecisionInput(props.task.id, props.task.revision, props.selection, decision);
		if (override) {
			input.confirm_descendant_override = override.kind === 'descendant_override';
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

	const submitCommit = async () => {
		if (!commitPlan.data?.can_commit || !permanentDeleteConfirmed) return;
		const result = await commit.mutateAsync(buildCommitInput(props.task.id, props.task.revision, permanentDeleteConfirmed));
		if (typeof result !== 'string' && 'StaleRevision' in result) props.onStale();
		else props.onApplied();
	};

	const submitFinish = async () => {
		const result = await finish.mutateAsync(buildFinishInput(props.task.id, props.task.revision, confirmUnmarked));
		if ('StaleRevision' in result) props.onStale();
		else if ('ConfirmationRequired' in result) setConfirmUnmarked(true);
		else props.onApplied();
	};

	return (
		<section className="space-y-3 border-b border-app-line p-3" aria-label="Organize decisions">
			<OrganizeProgress progress={props.progress} />
			<div className="flex flex-wrap gap-2">
				<button disabled={readOnly || props.selection.kind === 'items' && props.selection.itemIds.size === 0} onClick={() => void submitDecision('Keep')}>Keep</button>
				<button disabled={readOnly || props.selection.kind === 'items' && props.selection.itemIds.size === 0} onClick={() => void submitDecision('Discard')}>Discard</button>
				<button disabled={readOnly || props.selection.kind === 'items' && props.selection.itemIds.size === 0} onClick={props.onChooseMove}>Move…</button>
			</div>
			{confirmation && (
				<div role="alertdialog" className="space-y-2 rounded-md border border-amber-500/40 p-3">
					<p>This decision affects {confirmation.keepUnits + confirmation.discardUnits + confirmation.moveUnits} already reviewed units.</p>
					<button disabled={pendingDecision === null} onClick={() => pendingDecision !== null && void submitDecision(pendingDecision, confirmation)}>Confirm override</button>
					<button onClick={() => { setConfirmation(null); setPendingDecision(null); }}>Cancel</button>
				</div>
			)}
			<div className="flex flex-wrap gap-2">
				<label><input type="checkbox" checked={permanentDeleteConfirmed} onChange={(event) => setPermanentDeleteConfirmed(event.target.checked)} /> Permanently delete discarded files</label>
				<button disabled={readOnly || !commitPlan.data?.can_commit || !permanentDeleteConfirmed} onClick={() => void submitCommit()}>Commit</button>
				<button disabled={readOnly} onClick={() => void submitFinish()}>Finish</button>
			</div>
			{confirmUnmarked && (
				<div role="alertdialog" className="space-y-2">
					<p>Confirm that unmarked items should remain untouched before finishing.</p>
					<button onClick={() => void submitFinish()}>Confirm finish</button>
					<button onClick={() => setConfirmUnmarked(false)}>Cancel</button>
				</div>
			)}
		</section>
	);
}
