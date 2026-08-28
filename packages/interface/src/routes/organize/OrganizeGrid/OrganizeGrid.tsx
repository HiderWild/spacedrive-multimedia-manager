import type {OrganizeItemDecisionProjection} from '@sd/ts-client';
import {useEffect, useMemo, useRef, useState} from 'react';
import type {CSSProperties, PointerEvent as ReactPointerEvent, ReactNode, RefObject} from 'react';
import type {Model} from '@sd/ts-client';
import {computeLassoSelection, edgeScrollVelocity, intersectRenderedCards, isLassoDrag, lassoRect} from '../selection';
import {gridColumnCount, virtualRowCount, virtualRowRange} from '../virtualization';

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

export function OrganizeGrid({items, width, viewportHeight, scrollTop, minimumCardWidth = 180, gap = 16, rowHeight = 220, overscanRows = 2, selectedItemIds, onLassoSelectionChange, scrollContainerRef, renderItem}: OrganizeGridProps) {
	const columns = gridColumnCount(width, minimumCardWidth, gap);
	const rows = virtualRowCount(items.length, columns);
	const range = virtualRowRange(scrollTop, viewportHeight, rowHeight, rows, overscanRows);
	const visible = useMemo(() => items.slice(range.start * columns, range.end * columns), [items, columns, range.start, range.end]);
	const surfaceRef = useRef<HTMLDivElement>(null);
	const lassoRef = useRef<LassoState | null>(null);
	const [lasso, setLasso] = useState<LassoState | null>(null);
	const suppressClickRef = useRef(false);
	const frameRef = useRef<number | null>(null);
	const selectionCallbackRef = useRef(onLassoSelectionChange);
	const updateLassoRef = useRef<(clientX: number, clientY: number) => void>(() => undefined);

	useEffect(() => {
		selectionCallbackRef.current = onLassoSelectionChange;
	}, [onLassoSelectionChange]);

	const updateLasso = (clientX: number, clientY: number) => {
		const current = lassoRef.current;
		const surface = surfaceRef.current;
		if (!current || !surface || !isLassoDrag(current.startX, current.startY, clientX, clientY)) return;

		current.currentX = clientX;
		current.currentY = clientY;
		setLasso({...current, pointerDownSelection: new Set(current.pointerDownSelection)});
		const intersections = intersectRenderedCards(
			lassoRect(current.startX, current.startY, clientX, clientY),
			surface.querySelectorAll<HTMLElement>('[data-organize-item-id]'),
		);
		selectionCallbackRef.current(computeLassoSelection(current.pointerDownSelection, intersections, current.ctrlKey));
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

		const velocity = edgeScrollVelocity(current.currentY, viewport.getBoundingClientRect());
		if (velocity !== 0) {
			viewport.scrollTop += velocity;
			updateLassoRef.current(current.currentX, current.currentY);
		}
		frameRef.current = requestAnimationFrame(edgeScrollFrame);
	};

	const startEdgeScroll = () => {
		if (frameRef.current === null) frameRef.current = requestAnimationFrame(edgeScrollFrame);
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
			pointerDownSelection: new Set(selectedItemIds),
		};
		lassoRef.current = current;
		suppressClickRef.current = false;
		setLasso(current);
		event.currentTarget.setPointerCapture(event.pointerId);
		startEdgeScroll();
	};

	const moveLasso = (event: ReactPointerEvent<HTMLDivElement>) => {
		if (lassoRef.current?.pointerId !== event.pointerId) return;
		if (isLassoDrag(lassoRef.current.startX, lassoRef.current.startY, event.clientX, event.clientY)) event.preventDefault();
		updateLasso(event.clientX, event.clientY);
	};

	const endLasso = (event: ReactPointerEvent<HTMLDivElement>) => {
		if (lassoRef.current?.pointerId !== event.pointerId) return;
		const current = lassoRef.current;
		if (isLassoDrag(current.startX, current.startY, event.clientX, event.clientY)) {
			updateLasso(event.clientX, event.clientY);
			suppressClickRef.current = true;
		}
		lassoRef.current = null;
		setLasso(null);
		stopEdgeScroll();
		if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
	};

	const lassoStyle: CSSProperties | null = lasso && isLassoDrag(lasso.startX, lasso.startY, lasso.currentX, lasso.currentY)
		? (() => {
			const rect = lassoRect(lasso.startX, lasso.startY, lasso.currentX, lasso.currentY);
			return {
				position: 'fixed' as const,
				left: rect.left,
				top: rect.top,
				width: rect.width,
				height: rect.height,
				zIndex: 20,
				pointerEvents: 'none' as const,
				border: '1px solid var(--color-accent)',
				background: 'color-mix(in srgb, var(--color-accent) 12%, transparent)',
			};
		})()
		: null;

	return <div ref={surfaceRef} data-testid="organize-grid" data-organize-lasso-surface data-organize-columns={columns} onPointerDown={beginLasso} onPointerMove={moveLasso} onPointerUp={endLasso} onPointerCancel={endLasso} onClickCapture={(event) => { if (!suppressClickRef.current) return; event.preventDefault(); event.stopPropagation(); suppressClickRef.current = false; }} style={{position: 'relative', display: 'grid', gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`, gap, minHeight: rows * rowHeight, touchAction: 'pan-y'}}>
		{lassoStyle && <div data-testid="organize-lasso" style={lassoStyle} />}
		{visible.map((entry, index) => <div key={entry.item.uuid} data-organize-item-id={entry.item.uuid} data-organize-row={range.start + Math.floor(index / columns)}>
			{renderItem(entry)}
			{entry.item.kind === 'directory' && entry.projection && <DirectoryProgress projection={entry.projection} />}
		</div>)}
	</div>;
}

function DirectoryProgress({projection}: {projection: OrganizeItemDecisionProjection}) {
	const {processed_units: processed, total_units: total} = projection.progress;
	const fraction = total > 0 ? Math.min(1, processed / total) : 0;
	return <div data-organize-directory-progress aria-label={`${processed} of ${total} processed`}>
		<div role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={processed} style={{width: `${fraction * 100}%`}} />
	</div>;
}
