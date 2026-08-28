import type {OrganizeItemDecisionProjection} from '@sd/ts-client';
import {useMemo} from 'react';
import type {ReactNode} from 'react';
import type {Model} from '@sd/ts-client';
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
	renderItem: (entry: OrganizeGridItem) => ReactNode;
}

export function OrganizeGrid({items, width, viewportHeight, scrollTop, minimumCardWidth = 180, gap = 16, rowHeight = 220, overscanRows = 2, renderItem}: OrganizeGridProps) {
	const columns = gridColumnCount(width, minimumCardWidth, gap);
	const rows = virtualRowCount(items.length, columns);
	const range = virtualRowRange(scrollTop, viewportHeight, rowHeight, rows, overscanRows);
	const visible = useMemo(() => items.slice(range.start * columns, range.end * columns), [items, columns, range.start, range.end]);
	return <div data-testid="organize-grid" data-organize-columns={columns} style={{display: 'grid', gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`, gap, minHeight: rows * rowHeight}}>
		{visible.map((entry, index) => <div key={entry.item.uuid} data-organize-item-id={entry.item.uuid} data-organize-row={range.start + Math.floor(index / columns)}>{renderItem(entry)}</div>)}
	</div>;
}
