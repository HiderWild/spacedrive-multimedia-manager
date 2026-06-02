import type { NavigationTarget } from "../context";

/**
 * Describes a single explorer pane inside the multi-pane layout.
 *
 * The first pane (`PRIMARY_PANE_ID`) always renders the existing, shared
 * explorer context so the default single-pane experience is unchanged.
 * Additional panes are fully independent and seed their own isolated explorer
 * context from `initialTarget`.
 */
export interface PaneDescriptor {
	id: string;
	/**
	 * Directory/view the pane opens at. `null` for the primary pane, which
	 * inherits the shared context's current location.
	 */
	initialTarget: NavigationTarget | null;
}

export const PRIMARY_PANE_ID = "primary";
