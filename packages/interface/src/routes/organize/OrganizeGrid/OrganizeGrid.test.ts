import {describe, expect, test} from 'bun:test';
import {gridLayoutWidth, shouldClearBlankSelection} from './OrganizeGrid';

describe('OrganizeGrid layout and blank selection contracts', () => {
	test('uses the measured container width when it is available', () => {
		expect(gridLayoutWidth(1280, 900)).toBe(1280);
	});

	test('falls back without a measured DOM width', () => {
		expect(gridLayoutWidth(0, 900)).toBe(900);
		expect(gridLayoutWidth(undefined, 900)).toBe(900);
	});

	test('clears selection only for an unmodified blank click', () => {
		expect(shouldClearBlankSelection(true, false)).toBe(true);
		expect(shouldClearBlankSelection(true, true)).toBe(false);
		expect(shouldClearBlankSelection(false, true)).toBe(false);
	});
});
