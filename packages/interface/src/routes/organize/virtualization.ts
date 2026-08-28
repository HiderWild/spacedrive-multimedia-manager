export function gridColumnCount(width: number, minimumCardWidth: number, gap: number): number {
	if (width <= 0 || minimumCardWidth <= 0) return 1;
	return Math.max(1, Math.floor((width + gap) / (minimumCardWidth + gap)));
}

export function virtualRowCount(itemCount: number, columnCount: number): number {
	if (itemCount <= 0) return 0;
	return Math.ceil(itemCount / Math.max(1, columnCount));
}

export function virtualRowRange(scrollTop: number, viewportHeight: number, rowHeight: number, rowCount: number, overscanRows = 2): {start: number; end: number} {
	if (rowCount <= 0) return {start: 0, end: 0};
	const first = Math.max(0, Math.floor(Math.max(0, scrollTop) / rowHeight) - overscanRows);
	const last = Math.min(rowCount, Math.ceil((Math.max(0, scrollTop) + Math.max(0, viewportHeight)) / rowHeight) + overscanRows);
	return {start: first, end: Math.max(first, last)};
}

export function createThumbnailCacheKey(path: string, sizeBytes: number, modifiedAt: string, isDirectory: boolean): string {
	const normalizedPath = path.replaceAll('\\', '/').replace(/\/+$/, '').toLowerCase();
	return `${isDirectory ? 'dir' : 'file'}:${normalizedPath}@${sizeBytes}@${modifiedAt}`;
}
