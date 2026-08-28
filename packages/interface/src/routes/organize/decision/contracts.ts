import type {
	OrganizeCommitInput,
	OrganizeCommitPlanOutput,
	OrganizeDecisionInput,
	OrganizeItemFilter,
	OrganizeProgressSummary,
	OrganizeSelectionInput,
	OrganizeSetDecisionInput,
	OrganizeFinishInput,
	OrganizeTaskSummary,
	SdPath,
} from '@sd/ts-client';

export type OrganizeSelectionState =
	| {
			kind: 'items';
			itemIds: Set<string>;
			focusId: string | null;
			anchorId: string | null;
	  }
	| {
			kind: 'directChildren';
			parentItemId: string;
			filter: OrganizeItemFilter;
			excludedItemIds: Set<string>;
			focusId: string | null;
			anchorId: string | null;
	  };

export function toWireSelection(state: OrganizeSelectionState): OrganizeSelectionInput {
	if (state.kind === 'items') {
		return {Items: {item_ids: [...state.itemIds]}};
	}

	return {
		DirectChildren: {
			parent_item_id: state.parentItemId,
			filter: state.filter,
			excluded_item_ids: [...state.excludedItemIds],
		},
	};
}

export function buildSetDecisionInput(
	taskId: string,
	revision: number,
	selection: OrganizeSelectionState,
	decision: OrganizeDecisionInput | null,
): OrganizeSetDecisionInput {
	return {
		task_id: taskId,
		expected_revision: revision,
		selection: toWireSelection(selection),
		decision,
		confirm_descendant_override: false,
		confirm_ancestor_split: false,
	};
}

export interface ConflictDialogModel {
	kind: 'descendant_override' | 'ancestor_split';
	keepUnits: number;
	discardUnits: number;
	moveUnits: number;
	unmarkedUnits: number;
	affectedBytes: number;
	conflictingRoots: string[];
	destructive: boolean;
}

export function conflictDialogModel(outcome: {
	ConfirmationRequired: {
		conflict_kind: ConflictDialogModel['kind'];
		keep_units: number;
		discard_units: number;
		move_units: number;
		unmarked_units: number;
		affected_bytes: number;
		conflicting_roots: string[];
	};
}): ConflictDialogModel {
	const conflict = outcome.ConfirmationRequired;
	return {
		kind: conflict.conflict_kind,
		keepUnits: conflict.keep_units,
		discardUnits: conflict.discard_units,
		moveUnits: conflict.move_units,
		unmarkedUnits: conflict.unmarked_units,
		affectedBytes: conflict.affected_bytes,
		conflictingRoots: conflict.conflicting_roots,
		destructive: conflict.discard_units > 0,
	};
}

export function progressSegments(progress: OrganizeProgressSummary) {
	const total = Math.max(progress.total_units, 1);
	return [
		{kind: 'keep' as const, fraction: progress.keep_units / total},
		{kind: 'discard' as const, fraction: progress.discard_units / total},
		{kind: 'move' as const, fraction: progress.move_units / total},
		{kind: 'unmarked' as const, fraction: progress.unmarked_units / total},
	];
}

export function buildCommitInput(
	taskId: string,
	revision: number,
	permanentDeleteConfirmed: boolean,
	allowCurrentSubtreeDrift = false,
): OrganizeCommitInput {
	return {
		task_id: taskId,
		expected_revision: revision,
		permanent_delete_confirmed: permanentDeleteConfirmed,
		move_conflict_policy: 'AutoModifyName',
		allow_current_subtree_drift: allowCurrentSubtreeDrift,
	};
}

export function buildFinishInput(
	taskId: string,
	revision: number,
	confirmUnmarked: boolean,
): OrganizeFinishInput {
	return {
		task_id: taskId,
		expected_revision: revision,
		confirm_unmarked: confirmUnmarked,
	};
}

export function canShowCommit(plan: OrganizeCommitPlanOutput | undefined): boolean {
	return plan?.can_commit === true;
}

export function taskIsReadOnly(task: OrganizeTaskSummary): boolean {
	return task.status === 'completed' || task.status === 'committing';
}

export function sameSdPath(a: SdPath, b: SdPath): boolean {
	return JSON.stringify(a) === JSON.stringify(b);
}
