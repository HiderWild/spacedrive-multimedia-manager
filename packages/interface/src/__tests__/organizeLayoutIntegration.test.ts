import {describe, expect, test} from 'bun:test';
import {clampInspectorWidth} from '../shellLayoutSizing';
import {clampOrganizePreviewWidth} from '../organizeLayoutSizing';

describe('organize layout integration', () => {
	test('organize mode gives more space to center pane than normal mode', () => {
		const containerWidth = 1600;
		const sidebarWidth = 220;
		const organizePaneWidth = 280;

		// Normal inspector mode - calculate its max (has hardcoded INSPECTOR_MAX_WIDTH = 520)
		const normalMaxWidth = clampInspectorWidth({
			containerWidth,
			sidebarWidth,
			requestedWidth: 9999 // Request very high to find the max
		});

		// Organize mode (with left pane) - calculate its max
		const organizeMaxWidth = clampOrganizePreviewWidth({
			containerWidth,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 9999 // Request very high to find the max
		});

		// Normal inspector has hardcoded max of 520px
		expect(normalMaxWidth).toBe(520);

		// Organize calculates dynamically based on available space
		// Available: 1600 - 220 - 280 = 1100px
		// Reserve 280px for center buttons/content = 820px max for preview
		expect(organizeMaxWidth).toBe(820);

		// Organize mode actually allows LARGER preview (820 > 520)
		// because it's calculated from available space, not a hardcoded limit
		expect(organizeMaxWidth).toBeGreaterThan(normalMaxWidth);
	});

	test('sidebar visibility affects organize max width', () => {
		const containerWidth = 1600;
		const organizePaneWidth = 280;

		// With sidebar - request high to find max
		const withSidebar = clampOrganizePreviewWidth({
			containerWidth,
			sidebarWidth: 220,
			organizePaneWidth,
			requestedWidth: 9999 // Request very high
		});

		// Without sidebar - request high to find max
		const withoutSidebar = clampOrganizePreviewWidth({
			containerWidth,
			sidebarWidth: 0,
			organizePaneWidth,
			requestedWidth: 9999 // Request very high
		});

		// No sidebar means more space available for preview
		expect(withoutSidebar).toBeGreaterThan(withSidebar);

		// With sidebar: 1600 - 220 - 280 - 280 = 820
		// Without sidebar: 1600 - 0 - 280 - 280 = 1040
		// Difference: 220 (sidebar width)
		expect(withSidebar).toBe(820);
		expect(withoutSidebar).toBe(1040);
		expect(withoutSidebar - withSidebar).toBe(220);
	});

	test('current width preserved when within dynamic limits', () => {
		const containerWidth = 1600;
		const sidebarWidth = 220;
		const organizePaneWidth = 280;

		// Request 350px - should be preserved
		const result1 = clampOrganizePreviewWidth({
			containerWidth,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 350
		});
		expect(result1).toBe(350);

		// Request 600px - should be preserved (available = 1100, max = 820)
		const result2 = clampOrganizePreviewWidth({
			containerWidth,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 600
		});
		expect(result2).toBe(600);

		// Request 900px - should be clamped to max
		const result3 = clampOrganizePreviewWidth({
			containerWidth,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 900
		});
		expect(result3).toBeLessThan(900);
		expect(result3).toBeLessThanOrEqual(820);
	});

	test('window resize updates max width dynamically', () => {
		const sidebarWidth = 220;
		const organizePaneWidth = 280;

		// Large window - request high to find max
		const largeMax = clampOrganizePreviewWidth({
			containerWidth: 1920,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 9999
		});

		// Medium window - request high to find max
		const mediumMax = clampOrganizePreviewWidth({
			containerWidth: 1280,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 9999
		});

		// Small window - request high to find max
		const smallMax = clampOrganizePreviewWidth({
			containerWidth: 1024,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 9999
		});

		// Max width should decrease as window shrinks
		expect(largeMax).toBeGreaterThan(mediumMax);
		expect(mediumMax).toBeGreaterThan(smallMax);

		// Large window should allow more than 500px
		expect(largeMax).toBeGreaterThan(500);

		// Test that 500px is preserved in all these window sizes
		const largeWith500 = clampOrganizePreviewWidth({
			containerWidth: 1920,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 500
		});
		const mediumWith500 = clampOrganizePreviewWidth({
			containerWidth: 1280,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 500
		});
		const smallWith500 = clampOrganizePreviewWidth({
			containerWidth: 1024,
			sidebarWidth,
			organizePaneWidth,
			requestedWidth: 500
		});

		expect(largeWith500).toBe(500);
		expect(mediumWith500).toBe(500);
		// Small window: 1024 - 220 - 280 = 524 available, reserve 280 = 244 max
		// So 500px request gets clamped to 244px
		expect(smallWith500).toBe(244);
	});
});
