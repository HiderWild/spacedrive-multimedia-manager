import clsx from "clsx";

import { useExplorer } from "../context";
import { SearchToolbar } from "../SearchToolbar";
import { TabNavigationGuard } from "../TabNavigationGuard";
import { ColumnView } from "../views/ColumnView";
import { GridView } from "../views/GridView";
import { KnowledgeView } from "../views/KnowledgeView";
import { ListView } from "../views/ListView";
import { MasonryView } from "../views/MasonryView";
import { MediaView } from "../views/MediaView";
import { SearchView } from "../views/SearchView";
import { SizeView } from "../views/SizeView";

/**
 * The scrollable view area of an explorer (everything below the toolbar).
 *
 * It reads everything from the surrounding explorer context, so the exact same
 * component renders both the primary pane (shared context) and every secondary
 * pane (its own isolated context) without any prop wiring.
 */
export function ExplorerPaneBody() {
	const { viewMode, mode } = useExplorer();

	return (
		<div
			className={clsx(
				"relative flex h-full w-full flex-col overflow-hidden pt-1.5",
				viewMode === "size" ? "bg-transparent" : "bg-app/80",
			)}
		>
			{mode.type === "search" && <SearchToolbar />}
			<div
				className={clsx(
					"flex-1",
					viewMode === "size" ? "overflow-visible" : "overflow-auto",
				)}
			>
				<TabNavigationGuard>
					{mode.type === "search" ? (
						<SearchView />
					) : viewMode === "grid" ? (
						<GridView />
					) : viewMode === "list" ? (
						<ListView />
					) : viewMode === "column" ? (
						<ColumnView />
					) : viewMode === "size" ? (
						<SizeView />
					) : viewMode === "knowledge" ? (
						<KnowledgeView />
					) : viewMode === "masonry" ? (
						<MasonryView />
					) : (
						<MediaView />
					)}
				</TabNavigationGuard>
			</div>
		</div>
	);
}
