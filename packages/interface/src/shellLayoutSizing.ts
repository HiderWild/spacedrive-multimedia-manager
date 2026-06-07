export const INSPECTOR_MIN_WIDTH = 240;
export const INSPECTOR_MAX_WIDTH = 520;
export const INSPECTOR_SHELL_PADDING = 16;
export const INSPECTOR_RESIZER_WIDTH = 1;
export const MIN_CENTER_CONTENT_WIDTH = 360;

export function getInspectorReservedWidth(inspectorWidth: number) {
	return inspectorWidth + INSPECTOR_SHELL_PADDING + INSPECTOR_RESIZER_WIDTH;
}

export function clampInspectorWidth(args: {
	containerWidth: number;
	sidebarWidth: number;
	requestedWidth: number;
}) {
	const {containerWidth, sidebarWidth, requestedWidth} = args;
	const maxWidth = Math.max(
		INSPECTOR_MIN_WIDTH,
		Math.min(
			INSPECTOR_MAX_WIDTH,
			containerWidth -
				sidebarWidth -
				MIN_CENTER_CONTENT_WIDTH -
				INSPECTOR_SHELL_PADDING -
				INSPECTOR_RESIZER_WIDTH
		)
	);

	return Math.min(maxWidth, Math.max(INSPECTOR_MIN_WIDTH, requestedWidth));
}
