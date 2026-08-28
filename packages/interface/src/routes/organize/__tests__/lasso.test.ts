import {describe, expect, test} from 'bun:test';
import {computeLassoSelection, edgeScrollVelocity} from '../selection';

	describe('organize lasso', () => {
	test('recomputes from the pointer-down baseline so backward shrink removes cards', () => {
		expect(computeLassoSelection(new Set(), new Set(['a', 'b', 'c']), false)).toEqual(new Set(['a', 'b', 'c']));
		expect(computeLassoSelection(new Set(), new Set(['b']), false)).toEqual(new Set(['b']));
		expect(computeLassoSelection(new Set(['fixed']), new Set(['b']), true)).toEqual(new Set(['fixed', 'b']));
	});

	test('edge scrolling is directional and bounded', () => {
		const viewport = {top: 0, bottom: 500} as DOMRect;
		expect(edgeScrollVelocity(0, viewport)).toBe(-24);
		expect(edgeScrollVelocity(500, viewport)).toBe(24);
		expect(edgeScrollVelocity(250, viewport)).toBe(0);
	});
});
