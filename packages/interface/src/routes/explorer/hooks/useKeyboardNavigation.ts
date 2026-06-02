import { useEffect, useRef } from "react";
import { useExplorer } from "../context";
import { useSelection } from "../SelectionContext";
import { isInputFocused } from "../../../util/keybinds/platform";

/**
 * Grid-like views arrange items in rows/columns, so they support both
 * horizontal (←/→) and vertical (↑/↓) selection movement. List and column
 * views own their own navigation, so they are intentionally excluded.
 *
 * Typed as a string set so the future "masonry" view works without a
 * `ViewMode` union change.
 */
const GRID_LIKE_VIEWS: ReadonlySet<string> = new Set([
	"grid",
	"media",
	"masonry",
]);

const ARROW_KEYS = new Set([
	"ArrowLeft",
	"ArrowRight",
	"ArrowUp",
	"ArrowDown",
]);

/**
 * Infers the column count of the active grid from the laid-out DOM.
 *
 * Items render with `data-file-id`, so grouping them by their top offset
 * recovers rows; the widest row equals the column count. This avoids coupling
 * the hook to any view's width math and degrades gracefully to a single column
 * when nothing is rendered.
 */
function getRenderedColumnCount(): number {
	if (typeof document === "undefined") return 1;

	const items = document.querySelectorAll<HTMLElement>("[data-file-id]");
	if (items.length === 0) return 1;

	const rowCounts = new Map<number, number>();
	let maxColumns = 1;

	for (const item of items) {
		const top = Math.round(item.getBoundingClientRect().top);
		const next = (rowCounts.get(top) ?? 0) + 1;
		rowCounts.set(top, next);
		if (next > maxColumns) maxColumns = next;
	}

	return maxColumns;
}

function scrollFileIntoView(fileId: string): void {
	if (typeof document === "undefined") return;
	const element = document.querySelector(`[data-file-id="${fileId}"]`);
	element?.scrollIntoView({ block: "nearest" });
}

/**
 * Keyboard navigation for the explorer grid and the fullscreen QuickPreview.
 *
 * When the preview is closed, arrow keys move the selection across the current
 * ordered file list (`currentFiles`): ←/→ step by one, ↑/↓ step by a full row.
 * When the preview is open, ←/→ advance the previewed item (selection drives the
 * preview via `QuickPreviewSyncer`) and Esc closes it.
 *
 * The handler runs in the capture phase so it can claim the keystroke before the
 * per-view bubble-phase listeners, keeping this hook the single source of truth
 * and ensuring keys are never hijacked while typing in an input.
 */
export function useKeyboardNavigation(): void {
	const { currentFiles, viewMode, quickPreviewFileId, closeQuickPreview } =
		useExplorer();
	const { focusedIndex, selectFile } = useSelection();

	// Latest values for the long-lived listener, avoiding re-registration on
	// every selection change (which happens on each navigation step).
	const stateRef = useRef({
		currentFiles,
		viewMode,
		quickPreviewFileId,
		closeQuickPreview,
		focusedIndex,
		selectFile,
	});
	stateRef.current = {
		currentFiles,
		viewMode,
		quickPreviewFileId,
		closeQuickPreview,
		focusedIndex,
		selectFile,
	};

	useEffect(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			const {
				currentFiles,
				viewMode,
				quickPreviewFileId,
				closeQuickPreview,
				focusedIndex,
				selectFile,
			} = stateRef.current;

			const isArrow = ARROW_KEYS.has(e.key);

			// Never steal navigation keys from a focused text field. Suppress the
			// per-view bubble listeners (which lack this guard) for grid arrows so
			// the caret can move, but let the field handle the default action.
			if (isInputFocused()) {
				if (isArrow && GRID_LIKE_VIEWS.has(viewMode)) {
					e.stopPropagation();
				}
				return;
			}

			// Preview open: ←/→ advance the previewed item, Esc closes it.
			if (quickPreviewFileId) {
				if (e.key === "Escape") {
					e.preventDefault();
					e.stopPropagation();
					closeQuickPreview();
					return;
				}

				if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
					e.preventDefault();
					e.stopPropagation();

					const index = currentFiles.findIndex(
						(f) => f.id === quickPreviewFileId,
					);
					if (index === -1) return;

					const nextIndex =
						e.key === "ArrowRight" ? index + 1 : index - 1;
					if (nextIndex < 0 || nextIndex >= currentFiles.length) {
						return;
					}

					// Selection drives the preview through QuickPreviewSyncer.
					selectFile(currentFiles[nextIndex], currentFiles);
				}
				return;
			}

			// Preview closed: arrow-key selection within grid-like views only.
			if (!isArrow || !GRID_LIKE_VIEWS.has(viewMode)) return;
			if (currentFiles.length === 0) return;

			e.preventDefault();
			e.stopPropagation();

			const lastIndex = currentFiles.length - 1;
			let nextIndex: number;

			if (focusedIndex < 0) {
				// Nothing focused yet: any arrow selects the first item.
				nextIndex = 0;
			} else {
				const columns = getRenderedColumnCount();
				switch (e.key) {
					case "ArrowLeft":
						nextIndex = Math.max(0, focusedIndex - 1);
						break;
					case "ArrowRight":
						nextIndex = Math.min(lastIndex, focusedIndex + 1);
						break;
					case "ArrowUp":
						nextIndex = Math.max(0, focusedIndex - columns);
						break;
					case "ArrowDown":
						nextIndex = Math.min(lastIndex, focusedIndex + columns);
						break;
					default:
						return;
				}
			}

			if (nextIndex !== focusedIndex && currentFiles[nextIndex]) {
				selectFile(currentFiles[nextIndex], currentFiles);
				scrollFileIntoView(currentFiles[nextIndex].id);
			}
		};

		window.addEventListener("keydown", handleKeyDown, { capture: true });
		return () =>
			window.removeEventListener("keydown", handleKeyDown, {
				capture: true,
			});
	}, []);
}
