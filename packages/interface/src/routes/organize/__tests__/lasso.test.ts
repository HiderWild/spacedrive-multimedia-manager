import {describe, expect, test} from 'bun:test';
import {computeLassoSelection, edgeScrollVelocity, isLassoDrag, lassoRect} from '../selection';

	describe('organize lasso', () => {
	test('keeps the pointer-down selection during a default drag', () => {
		expect(computeLassoSelection(new Set(), new Set(['a', 'b', 'c']), false)).toEqual(new Set(['a', 'b', 'c']));
		expect(computeLassoSelection(new Set(), new Set(['b']), false)).toEqual(new Set(['b']));
		expect(computeLassoSelection(new Set(['fixed']), new Set(['b']), false)).toEqual(new Set(['fixed', 'b']));
	});

	test('uses the pointer-down selection as the baseline for Ctrl toggling', () => {
		expect(computeLassoSelection(new Set(['fixed', 'remove']), new Set(['remove', 'add']), true)).toEqual(new Set(['fixed', 'add']));
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
