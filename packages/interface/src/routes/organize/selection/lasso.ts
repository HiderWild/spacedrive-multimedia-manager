export function computeLassoSelection(pointerDownSelection: Set<string>, currentIntersections: Set<string>, ctrlKey: boolean): Set<string> {
	if (ctrlKey) return new Set([...pointerDownSelection, ...currentIntersections]);
	return new Set(currentIntersections);
}

export function lassoRect(startX: number, startY: number, currentX: number, currentY: number): DOMRect {
	const left = Math.min(startX, currentX);
	const right = Math.max(startX, currentX);
	const top = Math.min(startY, currentY);
	const bottom = Math.max(startY, currentY);
	return {left, right, top, bottom, width: right - left, height: bottom - top, x: left, y: top, toJSON: () => ({})} as DOMRect;
}

export function isLassoDrag(startX: number, startY: number, currentX: number, currentY: number, threshold = 3): boolean {
	return Math.hypot(currentX - startX, currentY - startY) >= threshold;
}

export function intersectRenderedCards(rect: DOMRect, cards: Iterable<HTMLElement>): Set<string> {
	const result = new Set<string>();
	for (const card of cards) {
		const cardRect = card.getBoundingClientRect();
		const intersects = cardRect.left < rect.right && cardRect.right > rect.left && cardRect.top < rect.bottom && cardRect.bottom > rect.top;
		const itemId = card.dataset.organizeItemId;
		if (intersects && itemId) result.add(itemId);
	}
	return result;
}

export function edgeScrollVelocity(pointerY: number, viewport: DOMRect, edgeSize = 72, maxVelocity = 24): number {
	if (pointerY < viewport.top + edgeSize) return -maxVelocity * (1 - Math.max(0, pointerY - viewport.top) / edgeSize);
	if (pointerY > viewport.bottom - edgeSize) return maxVelocity * (1 - Math.max(0, viewport.bottom - pointerY) / edgeSize);
	return 0;
}
