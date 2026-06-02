import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import type { File } from "@sd/ts-client";
import { getContentKind } from "@sd/ts-client";

/**
 * Fisher-Yates shuffle over a slice of indices.
 *
 * Returns a new array so callers never mutate the source. Used to build a
 * shuffled play order without disturbing the underlying `files` array, which
 * stays in its fetched/sorted order for the rest of the explorer.
 */
function shuffleIndices(indices: number[]): number[] {
	const out = indices.slice();
	for (let i = out.length - 1; i > 0; i--) {
		const j = Math.floor(Math.random() * (i + 1));
		[out[i], out[j]] = [out[j], out[i]];
	}
	return out;
}

/**
 * Builds the play order (a permutation of `[0, fileCount)`).
 *
 * When `shuffle` is off the order is the identity sequence, so the slideshow
 * follows the explorer's current sort. When on, the indices are shuffled. The
 * order is rebuilt whenever the file count changes; new pages append their
 * fresh indices to the end so already-seen positions stay stable.
 */
function buildOrder(fileCount: number, shuffle: boolean): number[] {
	const identity = Array.from({ length: fileCount }, (_, i) => i);
	return shuffle ? shuffleIndices(identity) : identity;
}

function positionForStartIndex(
	order: number[],
	fileCount: number,
	startIndex: number,
): number {
	if (fileCount === 0) return 0;
	const clampedStart = Math.max(0, Math.min(startIndex, fileCount - 1));
	return Math.max(0, order.indexOf(clampedStart));
}

/**
 * Extends an existing order to cover newly loaded files.
 *
 * `fetchNextPage` grows `files`, so the order must gain the new indices without
 * reshuffling what the user has already wandered through. The appended indices
 * are shuffled among themselves when shuffle is active.
 */
function extendOrder(
	order: number[],
	prevCount: number,
	nextCount: number,
	shuffle: boolean,
): number[] {
	if (nextCount <= prevCount) return order;
	const fresh = Array.from(
		{ length: nextCount - prevCount },
		(_, i) => prevCount + i,
	);
	return order.concat(shuffle ? shuffleIndices(fresh) : fresh);
}

interface WanderState {
	/** Permutation of file indices defining playback order. */
	order: number[];
	/** Cursor into `order`; `files[order[position]]` is the current item. */
	position: number;
	/** File count the current `order` was built for. */
	knownCount: number;
	isPlaying: boolean;
	shuffle: boolean;
}

type WanderAction =
	| { type: "RESET"; fileCount: number; startIndex: number; shuffle: boolean }
	| { type: "SYNC_FILES"; fileCount: number }
	| { type: "NEXT" }
	| { type: "PREV" }
	| { type: "GO_TO"; position: number }
	| { type: "SET_PLAYING"; playing: boolean }
	| { type: "TOGGLE_PLAYING" }
	| { type: "TOGGLE_SHUFFLE" };

function wanderReducer(state: WanderState, action: WanderState | WanderAction): WanderState {
	// Allow lazy initial state to flow through unchanged.
	if (!("type" in action)) return action;

	switch (action.type) {
		case "RESET": {
			const order = buildOrder(action.fileCount, action.shuffle);
			const position = positionForStartIndex(
				order,
				action.fileCount,
				action.startIndex,
			);
			return {
				order,
				position,
				knownCount: action.fileCount,
				isPlaying: true,
				shuffle: action.shuffle,
			};
		}
		case "SYNC_FILES": {
			if (action.fileCount === state.knownCount) return state;
			const order = extendOrder(
				state.order,
				state.knownCount,
				action.fileCount,
				state.shuffle,
			);
			return { ...state, order, knownCount: action.fileCount };
		}
		case "NEXT": {
			if (state.order.length === 0) return state;
			// Modular wrap keeps the feed infinite even at the tail of the set.
			const position = (state.position + 1) % state.order.length;
			return { ...state, position };
		}
		case "PREV": {
			if (state.order.length === 0) return state;
			const position =
				(state.position - 1 + state.order.length) % state.order.length;
			return { ...state, position };
		}
		case "GO_TO": {
			if (state.order.length === 0) return state;
			const position = Math.max(
				0,
				Math.min(action.position, state.order.length - 1),
			);
			return { ...state, position };
		}
		case "SET_PLAYING":
			return { ...state, isPlaying: action.playing };
		case "TOGGLE_PLAYING":
			return { ...state, isPlaying: !state.isPlaying };
		case "TOGGLE_SHUFFLE": {
			const shuffle = !state.shuffle;
			// Rebuild the order but keep the current file on screen: find its
			// file index, then re-seat the cursor on that index in the new order.
			const currentFileIndex = state.order[state.position] ?? 0;
			const order = buildOrder(state.knownCount, shuffle);
			const position = Math.max(0, order.indexOf(currentFileIndex));
			return { ...state, shuffle, order, position };
		}
		default:
			return state;
	}
}

export interface UseWanderStreamOptions {
	/** The media set to wander through (already filtered/sorted by the view). */
	files: File[];
	/** Index into `files` to start from (e.g. the current selection). */
	startIndex?: number;
	/** Auto-advance interval for non-video items, in milliseconds. */
	intervalMs?: number;
	/** Whether the stream is active (overlay open). Pauses all timers when false. */
	enabled: boolean;
	/** Start in shuffled order. */
	shuffle?: boolean;
	/** How many upcoming items to resolve + preload. */
	preloadCount?: number;
	/** How close to the tail of the order before requesting another page. */
	fetchThreshold?: number;
	/** Whether more files can be loaded from the active source. */
	hasNextPage: boolean;
	/** Pull the next page from the underlying infinite query. */
	fetchNextPage: () => void;
	/**
	 * Resolves a playable/displayable URL for a file. Supplied by the view so
	 * the engine reuses the explorer's existing media-URL resolver instead of
	 * inventing URLs. Returning null skips preloading that item.
	 */
	resolveMediaUrl: (file: File) => string | null;
}

export interface WanderStream {
	/** The file currently on screen, or null when the set is empty. */
	current: File | null;
	/** Cursor position within the play order. */
	position: number;
	/** Number of items in the play order. */
	total: number;
	isPlaying: boolean;
	shuffle: boolean;
	/** True when the current item is a video (caller drives end-based advance). */
	currentIsVideo: boolean;
	/** Resolved URLs for the next `preloadCount` items (warmed via `Image()`). */
	upcomingUrls: string[];
	next: () => void;
	prev: () => void;
	goTo: (position: number) => void;
	togglePlay: () => void;
	setPlaying: (playing: boolean) => void;
	toggleShuffle: () => void;
	/** Called by the view when the current video reports `ended`. */
	onVideoEnded: () => void;
}

const DEFAULT_INTERVAL_MS = 5000;
const DEFAULT_PRELOAD_COUNT = 3;
const DEFAULT_FETCH_THRESHOLD = 5;

/**
 * Stream engine for the "wander" immersive slideshow.
 *
 * Owns the ordered/shuffleable sequence, the playback cursor, the auto-advance
 * timer, next-item preloading, and paging the underlying infinite query as the
 * cursor nears the tail. The view layer stays thin: it renders `current`,
 * forwards keyboard/button intents to the returned controls, and calls
 * `onVideoEnded` so videos advance when they finish instead of on a timer.
 *
 * Preloading: the next `preloadCount` files are resolved through the caller's
 * `resolveMediaUrl` and their URLs are warmed with `new Image()`, so the next
 * image is decoded before it is shown. The same URLs are returned as
 * `upcomingUrls` so the view can additionally warm video elements if it wants.
 *
 * Paging: whenever the cursor lands within `fetchThreshold` of the end of the
 * order and `hasNextPage` is true, `fetchNextPage` is invoked. New files are
 * folded into the order via `SYNC_FILES` without disturbing prior positions.
 */
export function useWanderStream({
	files,
	startIndex = 0,
	intervalMs = DEFAULT_INTERVAL_MS,
	enabled,
	shuffle = false,
	preloadCount = DEFAULT_PRELOAD_COUNT,
	fetchThreshold = DEFAULT_FETCH_THRESHOLD,
	hasNextPage,
	fetchNextPage,
	resolveMediaUrl,
}: UseWanderStreamOptions): WanderStream {
	const [state, dispatch] = useReducer(
		wanderReducer,
		undefined,
		(): WanderState => {
			const order = buildOrder(files.length, shuffle);
			return {
				order,
				position: positionForStartIndex(order, files.length, startIndex),
				knownCount: files.length,
				isPlaying: true,
				shuffle,
			};
		},
	);

	// Re-seed the stream each time it is (re)opened so it always starts from the
	// requested item. Keyed on `enabled` rising edge plus the start index.
	const wasEnabled = useRef(false);
	useEffect(() => {
		if (enabled && !wasEnabled.current) {
			dispatch({
				type: "RESET",
				fileCount: files.length,
				startIndex,
				shuffle,
			});
		}
		wasEnabled.current = enabled;
		// Only re-run on the enabled edge; start params are read at reset time.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [enabled]);

	// Fold newly fetched files into the existing order.
	useEffect(() => {
		dispatch({ type: "SYNC_FILES", fileCount: files.length });
	}, [files.length]);

	const total = state.order.length;
	const currentFileIndex =
		total > 0 ? state.order[state.position] ?? null : null;
	const current =
		currentFileIndex !== null ? files[currentFileIndex] ?? null : null;
	const currentIsVideo = current ? getContentKind(current) === "video" : false;

	const next = useCallback(() => dispatch({ type: "NEXT" }), []);
	const prev = useCallback(() => dispatch({ type: "PREV" }), []);
	const goTo = useCallback(
		(position: number) => dispatch({ type: "GO_TO", position }),
		[],
	);
	const togglePlay = useCallback(
		() => dispatch({ type: "TOGGLE_PLAYING" }),
		[],
	);
	const setPlaying = useCallback(
		(playing: boolean) => dispatch({ type: "SET_PLAYING", playing }),
		[],
	);
	const toggleShuffle = useCallback(
		() => dispatch({ type: "TOGGLE_SHUFFLE" }),
		[],
	);

	// Videos advance when they end; ignore the call if playback is paused so a
	// looping/paused video does not skip ahead on its own.
	const onVideoEnded = useCallback(() => {
		if (state.isPlaying) dispatch({ type: "NEXT" });
	}, [state.isPlaying]);

	// Auto-advance timer. Skipped for videos (they advance on `ended`) and when
	// paused, disabled, or the set is empty.
	useEffect(() => {
		if (!enabled || !state.isPlaying || total === 0) return;
		if (currentIsVideo) return;
		const timer = setTimeout(() => dispatch({ type: "NEXT" }), intervalMs);
		return () => clearTimeout(timer);
	}, [
		enabled,
		state.isPlaying,
		state.position,
		total,
		currentIsVideo,
		intervalMs,
	]);

	// Page in more files as the cursor approaches the tail of the order.
	useEffect(() => {
		if (!enabled || !hasNextPage || total === 0) return;
		if (state.position >= total - fetchThreshold) {
			fetchNextPage();
		}
	}, [
		enabled,
		hasNextPage,
		fetchNextPage,
		state.position,
		total,
		fetchThreshold,
	]);

	// Resolve URLs for the upcoming items and warm images so transitions are
	// instant. Videos cannot be reliably preloaded with `Image()`, so they are
	// only resolved here; the view warms them via a hidden <video> if desired.
	const upcomingUrls = useMemo(() => {
		if (total === 0) return [];
		const urls: string[] = [];
		for (let offset = 1; offset <= preloadCount; offset++) {
			const orderPos = (state.position + offset) % total;
			const fileIndex = state.order[orderPos];
			const file = files[fileIndex];
			if (!file) continue;
			const url = resolveMediaUrl(file);
			if (url) urls.push(url);
		}
		return urls;
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [state.position, state.order, total, preloadCount, files, resolveMediaUrl]);

	useEffect(() => {
		if (!enabled) return;
		const images = upcomingUrls.map((url) => {
			const img = new Image();
			img.src = url;
			return img;
		});
		return () => {
			// Drop references so the browser can reclaim if not yet needed.
			for (const img of images) img.src = "";
		};
	}, [enabled, upcomingUrls]);

	return {
		current,
		position: state.position,
		total,
		isPlaying: state.isPlaying,
		shuffle: state.shuffle,
		currentIsVideo,
		upcomingUrls,
		next,
		prev,
		goTo,
		togglePlay,
		setPlaying,
		toggleShuffle,
		onVideoEnded,
	};
}
