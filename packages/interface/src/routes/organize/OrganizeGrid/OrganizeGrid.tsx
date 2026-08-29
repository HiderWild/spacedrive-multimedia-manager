import type {Model, OrganizeItemDecisionProjection} from '@sd/ts-client';
import {useVirtualizer} from '@tanstack/react-virtual';
import {useEffect, useRef, useState} from 'react';
import type {
	CSSProperties,
	ReactNode,
	PointerEvent as ReactPointerEvent,
	RefObject
} from 'react';
import {
	computeLassoSelection,
	edgeScrollVelocity,
	intersectRenderedCards,
	isLassoDrag,
	lassoRect
} from '../selection';
import {gridColumnCount, virtualRowCount} from '../virtualization';

export function gridLayoutWidth(
	measuredWidth: number | undefined,
	fallbackWidth: number
): number {
	return measuredWidth && measuredWidth > 0
		? measuredWidth
		: Math.max(0, fallbackWidth);
}

export function shouldClearBlankSelection(
	isBlank: boolean,
	isModified: boolean
): boolean {
	return isBlank && !isModified;
}

export interface OrganizeGridItem {
	item: Model;
	projection?: OrganizeItemDecisionProjection;
}

export interface OrganizeGridProps {
	items: OrganizeGridItem[];
	width: number;
	viewportHeight: number;
	scrollTop: number;
	minimumCardWidth?: number;
	gap?: number;
	rowHeight?: number;
	overscanRows?: number;
	selectedItemIds: ReadonlySet<string>;
	onLassoSelectionChange: (itemIds: Set<string>) => void;
	onEndReached?: () => void;
	scrollContainerRef: RefObject<HTMLElement | null>;
	renderItem: (entry: OrganizeGridItem) => ReactNode;
}

interface LassoState {
	startX: number;
	startY: number;
	currentX: number;
	currentY: number;
	pointerId: number;
	ctrlKey: boolean;
	pointerDownSelection: Set<string>;
}

export function OrganizeGrid({
	items,
	width,
	viewportHeight,
	scrollTop,
	minimumCardWidth = 180,
	gap = 16,
	rowHeight = 220,
	overscanRows = 2,
	selectedItemIds,
	onLassoSelectionChange,
	onEndReached,
	scrollContainerRef,
	renderItem
}: OrganizeGridProps) {
	const surfaceRef = useRef<HTMLDivElement>(null);
	const [measuredWidth, setMeasuredWidth] = useState<number>();
	useEffect(() => {
		const surface = surfaceRef.current;
		if (!surface || typeof ResizeObserver === 'undefined') return;
		const observer = new ResizeObserver(([entry]) => {
			const nextWidth = entry?.contentRect.width ?? 0;
			setMeasuredWidth((currentWidth) =>
				currentWidth === nextWidth ? currentWidth : nextWidth
			);
		});
		observer.observe(surface);
		return () => observer.disconnect();
	}, []);
	const layoutWidth = gridLayoutWidth(measuredWidth, width);
	const columns = gridColumnCount(layoutWidth, minimumCardWidth, gap);
	const rows = virtualRowCount(items.length, columns);
	const rowVirtualizer = useVirtualizer({
		count: rows,
		getScrollElement: () => scrollContainerRef.current,
		estimateSize: () => rowHeight,
		initialOffset: scrollTop,
		overscan: overscanRows
	});
	const virtualRows = rowVirtualizer.getVirtualItems();
	void viewportHeight;
	useEffect(() => {
		if (virtualRows.at(-1)?.index === rows - 1) onEndReached?.();
	}, [onEndReached, rows, virtualRows]);
	const lassoRef = useRef<LassoState | null>(null);
	const [lasso, setLasso] = useState<LassoState | null>(null);
	const suppressClickRef = useRef(false);
	const frameRef = useRef<number | null>(null);
	const selectionCallbackRef = useRef(onLassoSelectionChange);
	const updateLassoRef = useRef<(clientX: number, clientY: number) => void>(
		() => undefined
	);

	useEffect(() => {
		selectionCallbackRef.current = onLassoSelectionChange;
	}, [onLassoSelectionChange]);

	const updateLasso = (clientX: number, clientY: number) => {
		const current = lassoRef.current;
		const surface = surfaceRef.current;
		if (
			!current ||
			!surface ||
			!isLassoDrag(current.startX, current.startY, clientX, clientY)
		)
			return;

		current.currentX = clientX;
		current.currentY = clientY;
		setLasso({
			...current,
			pointerDownSelection: new Set(current.pointerDownSelection)
		});
		const intersections = intersectRenderedCards(
			lassoRect(current.startX, current.startY, clientX, clientY),
			surface.querySelectorAll<HTMLElement>('[data-organize-item-id]')
		);
		selectionCallbackRef.current(
			computeLassoSelection(
				current.pointerDownSelection,
				intersections,
				current.ctrlKey
			)
		);
	};
	updateLassoRef.current = updateLasso;

	const stopEdgeScroll = () => {
		if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
		frameRef.current = null;
	};

	const edgeScrollFrame = () => {
		const current = lassoRef.current;
		const viewport = scrollContainerRef.current;
		if (!current || !viewport) {
			frameRef.current = null;
			return;
		}

		const velocity = edgeScrollVelocity(
			current.currentY,
			viewport.getBoundingClientRect()
		);
		if (velocity !== 0) {
			viewport.scrollTop += velocity;
			updateLassoRef.current(current.currentX, current.currentY);
		}
		frameRef.current = requestAnimationFrame(edgeScrollFrame);
	};

	const startEdgeScroll = () => {
		if (frameRef.current === null)
			frameRef.current = requestAnimationFrame(edgeScrollFrame);
	};

	useEffect(() => stopEdgeScroll, []);

	const beginLasso = (event: ReactPointerEvent<HTMLDivElement>) => {
		if (event.button !== 0 || !event.isPrimary) return;
		const current: LassoState = {
			startX: event.clientX,
			startY: event.clientY,
			currentX: event.clientX,
			currentY: event.clientY,
			pointerId: event.pointerId,
			ctrlKey: event.ctrlKey || event.metaKey,
			pointerDownSelection: new Set(selectedItemIds)
		};
		lassoRef.current = current;
		suppressClickRef.current = false;
		setLasso(current);
		event.currentTarget.setPointerCapture(event.pointerId);
		startEdgeScroll();
	};

	const moveLasso = (event: ReactPointerEvent<HTMLDivElement>) => {
		if (lassoRef.current?.pointerId !== event.pointerId) return;
		if (
			isLassoDrag(
				lassoRef.current.startX,
				lassoRef.current.startY,
				event.clientX,
				event.clientY
			)
		)
			event.preventDefault();
		updateLasso(event.clientX, event.clientY);
	};

	const endLasso = (event: ReactPointerEvent<HTMLDivElement>) => {
		if (lassoRef.current?.pointerId !== event.pointerId) return;
		const current = lassoRef.current;
		if (
			isLassoDrag(
				current.startX,
				current.startY,
				event.clientX,
				event.clientY
			)
		) {
			updateLasso(event.clientX, event.clientY);
			suppressClickRef.current = true;
		}
		lassoRef.current = null;
		setLasso(null);
		stopEdgeScroll();
		if (event.currentTarget.hasPointerCapture(event.pointerId))
			event.currentTarget.releasePointerCapture(event.pointerId);
	};

	const lassoStyle: CSSProperties | null =
		lasso &&
		isLassoDrag(lasso.startX, lasso.startY, lasso.currentX, lasso.currentY)
			? (() => {
					const rect = lassoRect(
						lasso.startX,
						lasso.startY,
						lasso.currentX,
						lasso.currentY
					);
					return {
						position: 'fixed' as const,
						left: rect.left,
						top: rect.top,
						width: rect.width,
						height: rect.height,
						zIndex: 20,
						pointerEvents: 'none' as const,
						border: '1px solid var(--color-accent)',
						background:
							'color-mix(in srgb, var(--color-accent) 12%, transparent)'
					};
				})()
			: null;

	return (
		<div
			ref={surfaceRef}
			data-testid="organize-grid"
			data-organize-lasso-surface
			data-organize-columns={columns}
			onPointerDown={beginLasso}
			onPointerMove={moveLasso}
			onPointerUp={endLasso}
			onPointerCancel={endLasso}
			onClickCapture={(event) => {
				if (suppressClickRef.current) {
					event.preventDefault();
					event.stopPropagation();
					suppressClickRef.current = false;
					return;
				}
				const target = event.target as Element | null;
				const clickedItem = target?.closest?.('[data-organize-item-id]');
				if (
					shouldClearBlankSelection(
						!clickedItem,
						event.ctrlKey || event.metaKey
					)
				)
					onLassoSelectionChange(new Set());
			}}
			style={{
				position: 'relative',
				minHeight: rowVirtualizer.getTotalSize(),
				touchAction: 'pan-y'
			}}
		>
			{lassoStyle && (
				<div data-testid="organize-lasso" style={lassoStyle} />
			)}
			{virtualRows.map((virtualRow) => {
				const startIndex = virtualRow.index * columns;
				const rowItems = items.slice(startIndex, startIndex + columns);
				return (
					<div
						key={virtualRow.key}
						data-organize-row={virtualRow.index}
						style={{
							position: 'absolute',
							top: virtualRow.start,
							left: 0,
							width: '100%',
							height: rowHeight,
							display: 'grid',
							gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
							gap
						}}
					>
						{rowItems.map((entry) => (
							<div
								key={entry.item.uuid}
								data-organize-item-id={entry.item.uuid}
							>
								{renderItem(entry)}
								{entry.item.kind === 'directory' &&
									entry.projection && (
										<DirectoryProgress
											projection={entry.projection}
										/>
									)}
							</div>
						))}
					</div>
				);
			})}
		</div>
	);
}

function DirectoryProgress({
	projection
}: {
	projection: OrganizeItemDecisionProjection;
}) {
	const {processed_units: processed, total_units: total} =
		projection.progress;
	const fraction = total > 0 ? Math.min(1, processed / total) : 0;
	return (
		<div
			data-organize-directory-progress
			aria-label={`${processed} of ${total} processed`}
		>
			<div
				role="progressbar"
				aria-valuemin={0}
				aria-valuemax={total}
				aria-valuenow={processed}
				style={{width: `${fraction * 100}%`}}
			/>
		</div>
	);
}
