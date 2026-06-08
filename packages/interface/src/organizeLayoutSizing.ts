/**
 * Layout sizing calculations specific to organize mode.
 * Ensures the preview pane doesn't compress action buttons (Keep/Discard/Clear).
 */

import {INSPECTOR_MIN_WIDTH} from './shellLayoutSizing';

/**
 * Minimum width needed for organize center pane action buttons + content.
 * Based on: Keep (80px) + Discard (80px) + Clear (70px) + gaps (16px) + padding (24px) = ~270px
 */
export const ORGANIZE_ACTION_BUTTONS_MIN_WIDTH = 280;

/**
 * Minimum width for organize center content area (file grid/list view).
 * Allows at least 1-2 grid items or comfortable list view.
 */
export const ORGANIZE_CENTER_MIN_WIDTH = 360;

/**
 * Calculate the maximum allowed width for the organize preview pane.
 * Ensures action buttons in the center pane always have sufficient space.
 *
 * Layout: [Sidebar?] [Organize Left: 280px] [Center: buttons + content] [Preview]
 *
 * @param args.containerWidth - Total container width (window width)
 * @param args.sidebarWidth - Width of left sidebar (0 if hidden, typically 220px)
 * @param args.organizePaneWidth - Width of organize left pane (typically 280px)
 * @returns Maximum preview width that doesn't compress action buttons
 */
export function calculateOrganizePreviewMaxWidth(args: {
	containerWidth: number;
	sidebarWidth: number;
	organizePaneWidth: number;
}): number {
	const {containerWidth, sidebarWidth, organizePaneWidth} = args;

	// Calculate available space for center + preview
	const availableWidth = containerWidth - sidebarWidth - organizePaneWidth;

	// Reserve space for center pane (action buttons + minimum content)
	const centerReservedWidth = ORGANIZE_ACTION_BUTTONS_MIN_WIDTH;

	// Maximum preview width = available - reserved for center
	const maxPreviewWidth = availableWidth - centerReservedWidth;

	// Ensure preview is at least the inspector minimum
	return Math.max(INSPECTOR_MIN_WIDTH, maxPreviewWidth);
}

/**
 * Clamp the inspector width for organize mode, respecting dynamic constraints.
 *
 * @param args.containerWidth - Total container width
 * @param args.sidebarWidth - Sidebar width (0 if hidden)
 * @param args.organizePaneWidth - Organize left pane width
 * @param args.requestedWidth - Desired preview width
 * @returns Clamped preview width
 */
export function clampOrganizePreviewWidth(args: {
	containerWidth: number;
	sidebarWidth: number;
	organizePaneWidth: number;
	requestedWidth: number;
}): number {
	const {containerWidth, sidebarWidth, organizePaneWidth, requestedWidth} =
		args;

	const maxWidth = calculateOrganizePreviewMaxWidth({
		containerWidth,
		sidebarWidth,
		organizePaneWidth
	});

	// Clamp between minimum and dynamic maximum
	return Math.min(maxWidth, Math.max(INSPECTOR_MIN_WIDTH, requestedWidth));
}
