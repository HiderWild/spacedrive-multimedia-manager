import type {Model, OrganizeAcceptChangesInput} from '@sd/ts-client';

export type OrganizeChangeItem = Pick<
	Model,
	'uuid' | 'name' | 'relative_path' | 'membership_state' | 'external_state'
>;

export interface OrganizeChangeSelection {
	additions: string[];
	changed: string[];
	missing: string[];
}

export function mergeOrganizeChangeItems(
	pages: readonly (readonly OrganizeChangeItem[])[],
): OrganizeChangeItem[] {
	const byId = new Map<string, OrganizeChangeItem>();
	for (const page of pages) {
		for (const item of page) {
			if (!byId.has(item.uuid)) byId.set(item.uuid, item);
		}
	}
	return [...byId.values()];
}

export function partitionOrganizeChanges(
	items: readonly OrganizeChangeItem[],
): OrganizeChangeSelection {
	return items.reduce<OrganizeChangeSelection>(
		(selection, item) => {
			if (item.membership_state === 'pending_addition') {
				selection.additions.push(item.uuid);
			} else if (item.external_state === 'changed') {
				selection.changed.push(item.uuid);
			} else if (item.external_state === 'missing') {
				selection.missing.push(item.uuid);
			}
			return selection;
		},
		{additions: [], changed: [], missing: []},
	);
}

export function buildAcceptChangesInput(
	taskId: string,
	revision: number,
	selection: OrganizeChangeSelection,
	confirmInheritedDestructive = false,
): OrganizeAcceptChangesInput {
	return {
		task_id: taskId,
		expected_revision: revision,
		include_addition_ids: selection.additions,
		remove_missing_ids: selection.missing,
		refresh_changed_ids: selection.changed,
		preserve_changed_decisions: true,
		confirm_inherited_destructive: confirmInheritedDestructive,
	};
}
