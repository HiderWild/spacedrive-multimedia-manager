import { ExplorerProvider } from "../context";
import { SelectionProvider } from "../SelectionContext";
import { ExplorerPaneBody } from "./ExplorerPaneBody";
import { PaneHeader } from "./PaneHeader";
import type { PaneDescriptor } from "./types";

interface SecondaryPaneProps {
	descriptor: PaneDescriptor;
	onClose: () => void;
}

/**
 * A fully independent explorer instance.
 *
 * Wraps its content in its own isolated `SelectionProvider` and
 * `ExplorerProvider` so its location, view mode, sort, scroll, and selection
 * are scoped to this pane and never leak into the shared tab state or the
 * router URL. The primary pane keeps using the outer shared context, so adding
 * panes never disturbs the default experience.
 */
export function SecondaryPane({ descriptor, onClose }: SecondaryPaneProps) {
	return (
		<SelectionProvider isolated isActiveTab={false}>
			<ExplorerProvider isolated initialTarget={descriptor.initialTarget}>
				<div className="flex h-full w-full flex-col overflow-hidden">
					<PaneHeader onClose={onClose} />
					<div className="min-h-0 flex-1">
						<ExplorerPaneBody />
					</div>
				</div>
			</ExplorerProvider>
		</SelectionProvider>
	);
}
