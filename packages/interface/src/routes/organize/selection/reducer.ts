import type {OrganizeItemFilter, OrganizeSelectionInput} from '@sd/ts-client';

export type OrganizeSelectionState =
	| {kind: 'items'; itemIds: Set<string>; focusId: string | null; anchorId: string | null}
	| {kind: 'directChildren'; parentItemId: string; filter: OrganizeItemFilter; excludedItemIds: Set<string>; focusId: string | null; anchorId: string | null};

export type OrganizeSelectionEvent =
	| {type: 'plainClick'; itemId: string | null; orderedIds: string[]}
	| {type: 'ctrlClick'; itemId: string; orderedIds?: string[]}
	| {type: 'shiftClick'; itemId: string; orderedIds: string[]}
	| {type: 'selectAll'; parentItemId: string; filter: OrganizeItemFilter}
	| {type: 'directoryChanged'}
	| {type: 'clear'};

export function createSelectionState(): OrganizeSelectionState {
	return {kind: 'items', itemIds: new Set(), focusId: null, anchorId: null};
}

function rangeIds(orderedIds: string[], anchorId: string | null, itemId: string): Set<string> {
	const anchorIndex = anchorId === null ? -1 : orderedIds.indexOf(anchorId);
	const itemIndex = orderedIds.indexOf(itemId);
	if (itemIndex < 0) return new Set([itemId]);
	if (anchorIndex < 0) return new Set([itemId]);
	const start = Math.min(anchorIndex, itemIndex);
	const end = Math.max(anchorIndex, itemIndex);
	return new Set(orderedIds.slice(start, end + 1));
}

export function reduceSelection(state: OrganizeSelectionState, event: OrganizeSelectionEvent): OrganizeSelectionState {
	switch (event.type) {
		case 'directoryChanged':
		case 'clear':
			return createSelectionState();
		case 'selectAll':
			return {kind: 'directChildren', parentItemId: event.parentItemId, filter: event.filter, excludedItemIds: new Set(), focusId: null, anchorId: null};
		case 'plainClick':
			return event.itemId === null
				? createSelectionState()
				: {kind: 'items', itemIds: new Set([event.itemId]), focusId: event.itemId, anchorId: event.itemId};
		case 'ctrlClick':
			if (state.kind === 'directChildren') {
				const excludedItemIds = new Set(state.excludedItemIds);
				if (excludedItemIds.has(event.itemId)) excludedItemIds.delete(event.itemId);
				else excludedItemIds.add(event.itemId);
				return {...state, excludedItemIds, focusId: event.itemId};
			}
			{
				const itemIds = new Set(state.itemIds);
				if (itemIds.has(event.itemId)) itemIds.delete(event.itemId);
				else itemIds.add(event.itemId);
				return {...state, itemIds, focusId: event.itemId};
			}
		case 'shiftClick':
			if (state.kind === 'directChildren') return {...state, focusId: event.itemId};
			return {kind: 'items', itemIds: rangeIds(event.orderedIds, state.anchorId, event.itemId), focusId: event.itemId, anchorId: state.anchorId ?? event.itemId};
	}
}

export function selectedIds(state: OrganizeSelectionState): string[] {
	return state.kind === 'items' ? [...state.itemIds].sort() : [];
}

export function toWireSelection(state: OrganizeSelectionState): OrganizeSelectionInput {
	if (state.kind === 'items') return {Items: {item_ids: selectedIds(state)}};
	return {DirectChildren: {parent_item_id: state.parentItemId, filter: state.filter, excluded_item_ids: [...state.excludedItemIds].sort()}};
}
