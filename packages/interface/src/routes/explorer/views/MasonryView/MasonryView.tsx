import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useExplorer } from "../../context";
import { useSelection } from "../../SelectionContext";
import { useExplorerFiles } from "../../hooks/useExplorerFiles";
import {
	useJustifiedLayout,
	type JustifiedBox,
} from "../../hooks/useJustifiedLayout";
import { useEmptySpaceContextMenu } from "../../hooks/useEmptySpaceContextMenu";
import { MasonryViewItem } from "./MasonryViewItem";

const CONTAINER_PADDING = 12;
const BOX_SPACING = 6;
// Maps the user's grid-size slider onto a justified target row height so the
// existing "size" control still scales masonry rows sensibly.
const MIN_ROW_HEIGHT = 120;

interface MasonryRow {
	top: number;
	height: number;
	boxes: JustifiedBox[];
}

/**
 * Groups absolutely-positioned boxes into rows so the layout can be windowed by
 * row. The solver emits boxes left-to-right, top-to-bottom, so a new row begins
 * whenever a box's `top` advances past the current row.
 */
function groupBoxesIntoRows(boxes: JustifiedBox[]): MasonryRow[] {
	const rows: MasonryRow[] = [];
	let current: MasonryRow | null = null;

	for (const box of boxes) {
		if (!current || box.top > current.top + 1) {
			current = { top: box.top, height: box.height, boxes: [box] };
			rows.push(current);
		} else {
			current.boxes.push(box);
			current.height = Math.max(current.height, box.height);
		}
	}

	return rows;
}

export function MasonryView() {
	const { viewSettings, setCurrentFiles } = useExplorer();
	const {
		isSelected,
		focusedIndex,
		selectedFiles,
		selectFile,
		clearSelection,
		restoreSelectionFromFiles,
	} = useSelection();
	const emptySpaceContextMenu = useEmptySpaceContextMenu();

	const { files, isLoading, source, hasNextPage, fetchNextPage } =
		useExplorerFiles();

	useEffect(() => {
		setCurrentFiles(files);
	}, [files, setCurrentFiles]);

	useEffect(() => {
		restoreSelectionFromFiles(files);
	}, [files, restoreSelectionFromFiles]);

	const parentRef = useRef<HTMLDivElement>(null);
	const [containerWidth, setContainerWidth] = useState(0);
	const [isInitialized, setIsInitialized] = useState(false);

	// Measure synchronously before paint and reflow on every resize so the
	// justified layout always matches the current container width.
	useLayoutEffect(() => {
		const element = parentRef.current;
		if (!element) return;

		const updateWidth = () => {
			const newWidth = element.offsetWidth;
			if (newWidth > 0) {
				setContainerWidth(newWidth);
				setIsInitialized(true);
			}
		};

		const resizeObserver = new ResizeObserver(updateWidth);
		resizeObserver.observe(element);
		updateWidth();

		return () => resizeObserver.disconnect();
	}, []);

	const targetRowHeight = Math.max(MIN_ROW_HEIGHT, viewSettings.gridSize * 2);

	const { boxes, containerHeight } = useJustifiedLayout(
		files,
		containerWidth,
		{
			targetRowHeight,
			boxSpacing: BOX_SPACING,
			containerPadding: CONTAINER_PADDING,
		},
	);

	const rows = useMemo(() => groupBoxesIntoRows(boxes), [boxes]);

	const rowVirtualizer = useVirtualizer({
		count: rows.length,
		getScrollElement: () => parentRef.current,
		estimateSize: (index) => rows[index].height + BOX_SPACING,
		overscan: 4,
	});

	const virtualRows = rowVirtualizer.getVirtualItems();

	// Infinite loading: fetch the next page when the user nears the last rows of
	// the currently loaded set, mirroring GridView.
	useEffect(() => {
		if (!hasNextPage) return;
		const lastRow = virtualRows[virtualRows.length - 1];
		if (!lastRow) return;
		if (lastRow.index >= rows.length - 2) {
			fetchNextPage();
		}
	}, [hasNextPage, fetchNextPage, virtualRows, rows.length]);

	const handleContainerClick = (e: React.MouseEvent) => {
		if (e.target === e.currentTarget) {
			clearSelection();
		}
	};

	const handleContainerContextMenu = async (e: React.MouseEvent) => {
		if (e.target === e.currentTarget) {
			e.preventDefault();
			e.stopPropagation();
			await emptySpaceContextMenu.show(e);
		}
	};

	if (source === "tag" && files.length === 0 && !isLoading) {
		return (
			<div className="flex items-center justify-center h-full">
				<div className="text-center">
					<div className="text-ink-dull text-lg font-medium mb-1">
						No tagged files
					</div>
					<div className="text-ink-dull text-sm">
						Files tagged with this tag will appear here
					</div>
				</div>
			</div>
		);
	}

	return (
		<div
			ref={parentRef}
			className="h-full overflow-auto"
			onClick={handleContainerClick}
			onContextMenu={handleContainerContextMenu}
		>
			<div
				className="relative w-full"
				style={{
					height: `${containerHeight}px`,
					minHeight: "100%",
					opacity: isInitialized ? 1 : 0,
					transition: "opacity 0.1s",
				}}
				onClick={handleContainerClick}
				onContextMenu={handleContainerContextMenu}
			>
				{virtualRows.map((virtualRow) => {
					const row = rows[virtualRow.index];
					if (!row) return null;

					return (
						<div
							key={virtualRow.key}
							className="absolute left-0 w-full"
							style={{
								top: `${row.top}px`,
								height: `${row.height}px`,
							}}
						>
							{row.boxes.map((box) => (
								<div
									key={box.file.id}
									className="absolute"
									style={{
										left: `${box.left}px`,
										top: `${box.top - row.top}px`,
									}}
								>
									<MasonryViewItem
										file={box.file}
										allFiles={files}
										selected={isSelected(box.file.id)}
										focused={box.index === focusedIndex}
										onSelect={selectFile}
										width={box.width}
										height={box.height}
									/>
								</div>
							))}
						</div>
					);
				})}
			</div>
		</div>
	);
}
