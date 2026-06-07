import {describe, expect, test} from 'bun:test';
import {
	clampInspectorWidth,
	getInspectorReservedWidth,
	INSPECTOR_MAX_WIDTH,
	INSPECTOR_MIN_WIDTH
} from '../shellLayoutSizing';

describe('shellLayoutSizing', () => {
	test('returns the panel width plus shell chrome for reserved layout width', () => {
		expect(getInspectorReservedWidth(320)).toBeGreaterThan(320);
	});

	test('clamps inspector width to the configured minimum and maximum', () => {
		expect(
			clampInspectorWidth({
				containerWidth: 1600,
				sidebarWidth: 220,
				requestedWidth: 120
			})
		).toBe(INSPECTOR_MIN_WIDTH);

		expect(
			clampInspectorWidth({
				containerWidth: 1600,
				sidebarWidth: 0,
				requestedWidth: 999
			})
		).toBe(INSPECTOR_MAX_WIDTH);
	});

	test('preserves minimum center content width when resizing the inspector', () => {
		expect(
			clampInspectorWidth({
				containerWidth: 900,
				sidebarWidth: 220,
				requestedWidth: 500
			})
		).toBeLessThan(500);
	});
});
