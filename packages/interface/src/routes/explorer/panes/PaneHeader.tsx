import { ArrowLeft, ArrowRight, X } from "@phosphor-icons/react";
import { CircleButton, CircleButtonGroup } from "@spacedrive/primitives";

import { PathBar } from "../components/PathBar";
import { VirtualPathBar } from "../components/VirtualPathBar";
import { useExplorer } from "../context";
import { ViewModeMenu } from "../ViewModeMenu";

interface PaneHeaderProps {
	onClose: () => void;
}

/**
 * Compact per-pane toolbar bound to the pane's own (isolated) explorer context.
 *
 * Gives every secondary pane independent navigation, view-mode switching, and a
 * close button without touching the global TopBar.
 */
export function PaneHeader({ onClose }: PaneHeaderProps) {
	const {
		goBack,
		goForward,
		canGoBack,
		canGoForward,
		currentPath,
		currentView,
		navigateToPath,
		devices,
		viewMode,
		setViewMode,
	} = useExplorer();

	return (
		<div className="flex h-9 shrink-0 items-center gap-2 border-b border-app-line bg-app/80 px-2">
			<CircleButtonGroup>
				<CircleButton
					icon={ArrowLeft}
					onClick={goBack}
					disabled={!canGoBack}
				/>
				<CircleButton
					icon={ArrowRight}
					onClick={goForward}
					disabled={!canGoForward}
				/>
			</CircleButtonGroup>

			<div className="min-w-0 flex-1 truncate">
				{currentPath ? (
					<PathBar
						path={currentPath}
						devices={devices}
						onNavigate={navigateToPath}
					/>
				) : currentView ? (
					<VirtualPathBar view={currentView} devices={devices} />
				) : null}
			</div>

			<ViewModeMenu
				viewMode={viewMode}
				onViewModeChange={(mode) => setViewMode(mode)}
			/>

			<CircleButton icon={X} onClick={onClose} />
		</div>
	);
}
