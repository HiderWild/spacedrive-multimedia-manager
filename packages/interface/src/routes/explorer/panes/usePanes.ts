import { useCallback, useMemo, useState } from "react";

import type { NavigationTarget } from "../context";
import { PRIMARY_PANE_ID, type PaneDescriptor } from "./types";

/**
 * Owns the list of explorer panes, their focus, and their relative widths.
 *
 * State lives in `ExplorerView` so the toolbar "Split" button and the
 * `PaneLayout` share a single source of truth. The default state is a single
 * primary pane, which `PaneLayout` renders without any wrapping, keeping the
 * single-pane behavior identical to before.
 */
export interface PanesApi {
	panes: PaneDescriptor[];
	/** Per-pane width as a percentage of the container. Sums to ~100. */
	sizes: number[];
	focusedId: string;
	isSplit: boolean;
	/** Add a pane to the right, seeded at `target`. */
	splitPane: (target: NavigationTarget | null) => void;
	closePane: (id: string) => void;
	focusPane: (id: string) => void;
	/** Replace the size of the two panes either side of a resizer. */
	resize: (boundaryIndex: number, leftPercent: number, rightPercent: number) => void;
}

const MIN_PANE_PERCENT = 12;

function evenSizes(count: number): number[] {
	return Array.from({ length: count }, () => 100 / count);
}

let paneCounter = 0;

export function usePanes(): PanesApi {
	const [panes, setPanes] = useState<PaneDescriptor[]>([
		{ id: PRIMARY_PANE_ID, initialTarget: null },
	]);
	const [sizes, setSizes] = useState<number[]>([100]);
	const [focusedId, setFocusedId] = useState<string>(PRIMARY_PANE_ID);

	const splitPane = useCallback((target: NavigationTarget | null) => {
		paneCounter += 1;
		const id = `pane-${paneCounter}`;
		setPanes((prev) => [...prev, { id, initialTarget: target }]);
		setSizes((prev) => evenSizes(prev.length + 1));
		setFocusedId(id);
	}, []);

	const closePane = useCallback((id: string) => {
		if (id === PRIMARY_PANE_ID) return;
		setPanes((prev) => {
			const next = prev.filter((p) => p.id !== id);
			setSizes(evenSizes(next.length));
			return next;
		});
		setFocusedId((current) => (current === id ? PRIMARY_PANE_ID : current));
	}, []);

	const focusPane = useCallback((id: string) => {
		setFocusedId(id);
	}, []);

	const resize = useCallback(
		(boundaryIndex: number, leftPercent: number, rightPercent: number) => {
			setSizes((prev) => {
				if (boundaryIndex < 0 || boundaryIndex + 1 >= prev.length) {
					return prev;
				}
				const next = [...prev];
				const clampedLeft = Math.max(MIN_PANE_PERCENT, leftPercent);
				const clampedRight = Math.max(MIN_PANE_PERCENT, rightPercent);
				next[boundaryIndex] = clampedLeft;
				next[boundaryIndex + 1] = clampedRight;
				return next;
			});
		},
		[],
	);

	return useMemo(
		() => ({
			panes,
			sizes,
			focusedId,
			isSplit: panes.length > 1,
			splitPane,
			closePane,
			focusPane,
			resize,
		}),
		[panes, sizes, focusedId, splitPane, closePane, focusPane, resize],
	);
}
