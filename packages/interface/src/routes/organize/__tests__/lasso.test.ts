import {describe, expect, test} from 'bun:test';
import {computeLassoSelection, edgeScrollVelocity, isLassoDrag, lassoRect} from '../selection';

	describe('organize lasso', () => {
	test('plain lasso replaces the pointer-down selection with current intersections', () => {
		expect(computeLassoSelection(new Set(), new Set(['a', 'b', 'c']), false)).toEqual(new Set(['a', 'b', 'c']));
		expect(computeLassoSelection(new Set(), new Set(['b']), false)).toEqual(new Set(['b']));
		expect(computeLassoSelection(new Set(['fixed']), new Set(['b']), false)).toEqual(new Set(['b']));
	});

	test('Ctrl lasso unions current intersections with the pointer-down selection', () => {
		const pointerDownSelection = new Set(['fixed', 'remove']);
		expect(computeLassoSelection(pointerDownSelection, new Set(['remove', 'add']), true)).toEqual(new Set(['fixed', 'remove', 'add']));
		expect(computeLassoSelection(pointerDownSelection, new Set(['add']), true)).toEqual(new Set(['fixed', 'remove', 'add']));
	});

	test('edge scrolling is directional and bounded', () => {
		const viewport = {top: 0, bottom: 500} as DOMRect;
		expect(edgeScrollVelocity(0, viewport)).toBe(-24);
		expect(edgeScrollVelocity(500, viewport)).toBe(24);
		expect(edgeScrollVelocity(250, viewport)).toBe(0);
	});

	test('pointer wiring normalizes backward drags and ignores click-sized movement', () => {
		const rect = lassoRect(100, 200, 40, 80);
		expect(rect).toMatchObject({left: 40, right: 100, top: 80, bottom: 200, width: 60, height: 120});
		expect(isLassoDrag(10, 10, 12, 12)).toBe(false);
		expect(isLassoDrag(10, 10, 14, 10)).toBe(true);
	});
});
