import {describe, expect, test} from 'bun:test';
import {createSelectionState, reduceSelection, selectedIds, toWireSelection} from '../selection';

describe('organize selection reducer', () => {
	test('plain replaces, ctrl toggles, shift selects a contiguous range', () => {
		let state = createSelectionState();
		state = reduceSelection(state, {type: 'plainClick', itemId: 'b', orderedIds: ['a', 'b', 'c', 'd']});
		state = reduceSelection(state, {type: 'ctrlClick', itemId: 'd'});
		state = reduceSelection(state, {type: 'shiftClick', itemId: 'c', orderedIds: ['a', 'b', 'c', 'd']});
		expect(selectedIds(state)).toEqual(['b', 'c']);
	});

	test('select all keeps a direct-children scope and ctrl excludes one item', () => {
		let state = reduceSelection(createSelectionState(), {type: 'selectAll', parentItemId: 'root', filter: 'Unmarked'});
		state = reduceSelection(state, {type: 'ctrlClick', itemId: 'visible-9'});
		expect(toWireSelection(state)).toEqual({DirectChildren: {parent_item_id: 'root', filter: 'Unmarked', excluded_item_ids: ['visible-9']}});
	});

	test('blank plain click and directory changes clear transient selection', () => {
		let state = reduceSelection(createSelectionState(), {type: 'plainClick', itemId: 'a', orderedIds: ['a']});
		state = reduceSelection(state, {type: 'plainClick', itemId: null, orderedIds: []});
		expect(selectedIds(state)).toEqual([]);
		state = reduceSelection(state, {type: 'directoryChanged'});
		expect(selectedIds(state)).toEqual([]);
	});
});
