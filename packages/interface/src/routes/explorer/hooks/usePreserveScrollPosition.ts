import { useEffect, useLayoutEffect, useRef } from "react";
import { useExplorer } from "../context";

/**
 * Session-only scroll position for the explorer (per tab via TabManager).
 * Not written to disk — restores after QuickPreview / remount within the session.
 *
 * Attach the returned ref to the overflow scroll container.
 */
export function usePreserveScrollPosition<T extends HTMLElement>(
	options: {
		/** When false, skip restore (e.g. before layout is measured). Default true. */
		enabled?: boolean;
		/**
		 * Optional default when no saved offset exists (e.g. MediaView inverted start).
		 * Called at most once after the element is ready.
		 */
		onDefaultPosition?: (el: T) => void;
	} = {},
) {
	const { enabled = true, onDefaultPosition } = options;
	const { scrollPosition, setScrollPosition } = useExplorer();
	const ref = useRef<T | null>(null);
	const restoredRef = useRef(false);
	const defaultAppliedRef = useRef(false);
	// Snapshot of saved position at first enable — avoids fighting user scroll.
	const savedOnMount = useRef(scrollPosition);

	// Keep a fresh snapshot until we successfully restore once.
	useEffect(() => {
		if (!restoredRef.current) {
			savedOnMount.current = scrollPosition;
		}
	}, [scrollPosition]);

	const tryRestore = () => {
		if (!enabled || restoredRef.current) return;
		const el = ref.current;
		if (!el) return;

		const { top, left } = savedOnMount.current;
		const hasSaved = top > 0 || left > 0;

		if (hasSaved) {
			// Virtual lists may still be measuring; retry a couple of frames.
			el.scrollTop = top;
			el.scrollLeft = left;
			// Confirm apply (content may grow after first paint).
			if (Math.abs(el.scrollTop - top) < 2 || el.scrollHeight >= top) {
				restoredRef.current = true;
				defaultAppliedRef.current = true;
			}
			return;
		}

		if (onDefaultPosition && !defaultAppliedRef.current) {
			onDefaultPosition(el);
			defaultAppliedRef.current = true;
			restoredRef.current = true;
		}
	};

	useLayoutEffect(() => {
		tryRestore();
	});

	// Extra attempts after mount for async content / virtualizer measure.
	useEffect(() => {
		if (!enabled) return;
		const id1 = requestAnimationFrame(() => tryRestore());
		const id2 = window.setTimeout(() => tryRestore(), 50);
		const id3 = window.setTimeout(() => tryRestore(), 200);
		return () => {
			cancelAnimationFrame(id1);
			clearTimeout(id2);
			clearTimeout(id3);
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps -- restore once when enabled
	}, [enabled]);

	// Persist while scrolling; flush on unmount (e.g. opening QuickPreview).
	useEffect(() => {
		const el = ref.current;
		if (!el) return;

		let raf = 0;
		const persist = () => {
			setScrollPosition({ top: el.scrollTop, left: el.scrollLeft });
		};

		const onScroll = () => {
			if (raf) cancelAnimationFrame(raf);
			raf = requestAnimationFrame(persist);
		};

		el.addEventListener("scroll", onScroll, { passive: true });
		return () => {
			if (raf) cancelAnimationFrame(raf);
			persist();
			el.removeEventListener("scroll", onScroll);
		};
	}, [setScrollPosition]);

	return ref;
}
