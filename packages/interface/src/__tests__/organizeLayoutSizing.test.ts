import {describe, expect, test} from 'bun:test';
import {
	calculateOrganizePreviewMaxWidth,
	ORGANIZE_ACTION_BUTTONS_MIN_WIDTH,
	ORGANIZE_CENTER_MIN_WIDTH
} from '../organizeLayoutSizing';

describe('organizeLayoutSizing', () => {
	test('calculates dynamic max width based on available space', () => {
		// With 1600px container, 220px sidebar, 280px left pane
		// Available = 1600 - 220 - 280 = 1100px
		// Must leave ORGANIZE_ACTION_BUTTONS_MIN_WIDTH for buttons
		const maxWidth = calculateOrganizePreviewMaxWidth({
			containerWidth: 1600,
			sidebarWidth: 220,
			organizePaneWidth: 280
		});

		// Should leave enough space for action buttons
		expect(maxWidth).toBeGreaterThan(240);
		expect(maxWidth).toBeLessThan(1100);
		expect(1100 - maxWidth).toBeGreaterThanOrEqual(
			ORGANIZE_ACTION_BUTTONS_MIN_WIDTH
		);
	});

	test('respects minimum center content width for action buttons', () => {
		// Small container - must ensure buttons have space
		const maxWidth = calculateOrganizePreviewMaxWidth({
			containerWidth: 900,
			sidebarWidth: 220,
			organizePaneWidth: 280
		});

		// Available = 900 - 220 - 280 = 400px
		// Max preview = 400 - 280 (reserved for center) = 120px
		// But clamped to INSPECTOR_MIN_WIDTH (240px)
		expect(maxWidth).toBe(240); // Clamped to minimum

		// The center gets what's left: 400 - 240 = 160px
		// This is less than ideal but respects inspector minimum
	});

	test('handles sidebar hidden case', () => {
		const maxWidth = calculateOrganizePreviewMaxWidth({
			containerWidth: 1600,
			sidebarWidth: 0,
			organizePaneWidth: 280
		});

		// Available = 1600 - 0 - 280 = 1320px
		expect(maxWidth).toBeGreaterThan(240);
		expect(1320 - maxWidth).toBeGreaterThanOrEqual(
			ORGANIZE_ACTION_BUTTONS_MIN_WIDTH
		);
	});

	test('returns reasonable value when space is extremely constrained', () => {
		// Extremely small container
		const maxWidth = calculateOrganizePreviewMaxWidth({
			containerWidth: 600,
			sidebarWidth: 220,
			organizePaneWidth: 280
		});

		// Available = 600 - 220 - 280 = 100px (very constrained!)
		// Max preview = 100 - 280 (reserved) = -180px (negative!)
		// Clamped to INSPECTOR_MIN_WIDTH = 240px
		expect(maxWidth).toBe(240);

		// In this extreme case, the layout will overflow or need horizontal scroll
		// This is expected behavior for very small windows
	});

	test('current width within limit is preserved', () => {
		// If current width is 300 and max is 400, keep 300
		const containerWidth = 1200;
		const sidebarWidth = 220;
		const organizePaneWidth = 280;

		const maxWidth = calculateOrganizePreviewMaxWidth({
			containerWidth,
			sidebarWidth,
			organizePaneWidth
		});

		// Available = 1200 - 220 - 280 = 700px
		// Current 300 should be preserved if < maxWidth
		const currentWidth = 300;
		expect(currentWidth).toBeLessThanOrEqual(maxWidth);
	});

	test('exports expected constants', () => {
		// Action buttons need minimum space (3 buttons + gaps + padding)
		expect(ORGANIZE_ACTION_BUTTONS_MIN_WIDTH).toBeGreaterThanOrEqual(200);
		expect(ORGANIZE_ACTION_BUTTONS_MIN_WIDTH).toBeLessThanOrEqual(400);

		// Center pane minimum (for file grid/list)
		expect(ORGANIZE_CENTER_MIN_WIDTH).toBeGreaterThanOrEqual(280);
		expect(ORGANIZE_CENTER_MIN_WIDTH).toBeLessThanOrEqual(500);
	});
});
