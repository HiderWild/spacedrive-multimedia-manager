import {
	createContext,
	useContext,
	useReducer,
	useMemo,
	useEffect,
	useCallback,
	useId,
	useRef,
	type ReactNode,
} from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { useNormalizedQuery } from "../../contexts/SpacedriveContext";
import { useTabManager } from "../../components/TabManager/useTabManager";
import type {
	ViewMode as TabViewMode,
	SortBy as TabSortBy,
	TabExplorerState,
} from "../../components/TabManager/TabManagerContext";

import type {
	SdPath,
	File,
	Device,
	ListLibraryDevicesInput,
	DirectorySortBy,
	MediaSortBy,
	SearchFilters as ApiSearchFilters,
} from "@sd/ts-client";
import {
	useViewPreferencesStore,
	useSortPreferencesStore,
} from "@sd/ts-client";

export type SortBy = DirectorySortBy | MediaSortBy;
export type ViewMode =
	| "grid"
	| "list"
	| "media"
	| "masonry"
	| "column"
	| "size"
	| "knowledge"
	| "organize";

export interface ViewSettings {
	gridSize: number;
	gapSize: number;
	showFileSize: boolean;
	columnWidth: number;
	foldersFirst: boolean;
	sizeViewItemLimit: number;
}

export type SearchScope = "folder" | "location" | "library";

export interface SearchFilters {
	fileTypes?: string[];
	contentTypes?: string[];
	sizeMin?: number;
	sizeMax?: number;
	dateModifiedStart?: Date;
	dateModifiedEnd?: Date;
	tags?: string[];
}

export type ExplorerMode =
	| { type: "browse" }
	| { type: "search"; query: string; scope: SearchScope }
	| { type: "recents" }
	| { type: "tag"; tagId: string }
	| { type: "filtered"; filters: ApiSearchFilters; label: string };

export type NavigationTarget =
	| { type: "path"; path: SdPath }
	| {
			type: "view";
			view: string;
			id?: string;
			params?: Record<string, string>;
	  };

function targetToKey(target: NavigationTarget): string {
	if (target.type === "path") {
		const p = target.path;
		if ("Physical" in p && p.Physical) {
			return `path:${p.Physical.device_slug}:${p.Physical.path}`;
		}
		if ("Virtual" in p && p.Virtual) {
			return `path:virtual:${p.Virtual}`;
		}
		return `path:${JSON.stringify(p)}`;
	}
	return `view:${target.view}:${target.id || ""}`;
}

function targetsEqual(
	a: NavigationTarget | null,
	b: NavigationTarget | null,
): boolean {
	if (a === null || b === null) return a === b;
	return targetToKey(a) === targetToKey(b);
}

const MAX_HISTORY_SIZE = 100;

interface NavigationState {
	history: NavigationTarget[];
	index: number;
}

type NavigationAction =
	| { type: "NAVIGATE"; target: NavigationTarget }
	| { type: "GO_BACK" }
	| { type: "GO_FORWARD" }
	| { type: "SYNC"; target: NavigationTarget };

function navigationReducer(
	state: NavigationState,
	action: NavigationAction,
): NavigationState {
	switch (action.type) {
		case "NAVIGATE": {
			const current = state.history[state.index];
			if (current && targetsEqual(current, action.target)) {
				return state;
			}

			const newHistory = state.history.slice(0, state.index + 1);
			newHistory.push(action.target);

			const trimmedHistory = newHistory.slice(-MAX_HISTORY_SIZE);
			const indexAdjustment = newHistory.length - trimmedHistory.length;

			return {
				history: trimmedHistory,
				index: state.index + 1 - indexAdjustment,
			};
		}

		case "GO_BACK": {
			if (state.index <= 0) return state;
			return { ...state, index: state.index - 1 };
		}

		case "GO_FORWARD": {
			if (state.index >= state.history.length - 1) return state;
			return { ...state, index: state.index + 1 };
		}

		case "SYNC": {
			const current = state.history[state.index];
			if (current && targetsEqual(current, action.target)) {
				return state;
			}

			const newHistory = [
				...state.history.slice(0, state.index + 1),
				action.target,
			];
			const trimmedHistory = newHistory.slice(-MAX_HISTORY_SIZE);
			const indexAdjustment = newHistory.length - trimmedHistory.length;

			return {
				history: trimmedHistory,
				index: state.index + 1 - indexAdjustment,
			};
		}

		default:
			return state;
	}
}

const initialNavigationState: NavigationState = {
	history: [],
	index: -1,
};

interface UIState {
	viewMode: ViewMode;
	sortBy: SortBy;
	viewSettings: ViewSettings;
	sidebarVisible: boolean;
	inspectorVisible: boolean;
	quickPreviewFileId: string | null;
	tagModeActive: boolean;
	mode: ExplorerMode;
	searchFilters: SearchFilters;
}

type UIAction =
	| { type: "SET_VIEW_MODE"; mode: ViewMode }
	| { type: "SET_SORT_BY"; sort: SortBy }
	| { type: "SET_VIEW_SETTINGS"; settings: Partial<ViewSettings> }
	| { type: "SET_SIDEBAR_VISIBLE"; visible: boolean }
	| { type: "SET_INSPECTOR_VISIBLE"; visible: boolean }
	| { type: "SET_QUICK_PREVIEW"; fileId: string | null }
	| { type: "SET_TAG_MODE"; active: boolean }
	| { type: "ENTER_SEARCH_MODE"; query: string; scope: SearchScope }
	| { type: "EXIT_SEARCH_MODE" }
	| { type: "ENTER_RECENTS_MODE" }
	| { type: "EXIT_RECENTS_MODE" }
	| { type: "ENTER_FILTERED_MODE"; filters: ApiSearchFilters; label: string }
	| { type: "EXIT_FILTERED_MODE" }
	| { type: "ENTER_TAG_MODE"; tagId: string }
	| { type: "EXIT_TAG_MODE" }
	| { type: "SET_SEARCH_FILTERS"; filters: SearchFilters }
	| {
			type: "LOAD_PREFERENCES";
			viewMode: ViewMode;
			viewSettings?: Partial<ViewSettings>;
	  };

const defaultViewSettings: ViewSettings = {
	gridSize: 120,
	gapSize: 16,
	showFileSize: true,
	columnWidth: 256,
	foldersFirst: false,
	sizeViewItemLimit: 500,
};

function uiReducer(state: UIState, action: UIAction): UIState {
	switch (action.type) {
		case "SET_VIEW_MODE":
			return { ...state, viewMode: action.mode };

		case "SET_SORT_BY":
			return { ...state, sortBy: action.sort };

		case "SET_VIEW_SETTINGS":
			return {
				...state,
				viewSettings: { ...state.viewSettings, ...action.settings },
			};

		case "SET_SIDEBAR_VISIBLE":
			return { ...state, sidebarVisible: action.visible };

		case "SET_INSPECTOR_VISIBLE":
			return { ...state, inspectorVisible: action.visible };

		case "SET_QUICK_PREVIEW":
			return { ...state, quickPreviewFileId: action.fileId };

		case "SET_TAG_MODE":
			return { ...state, tagModeActive: action.active };

		case "ENTER_SEARCH_MODE":
			return {
				...state,
				mode: { type: "search", query: action.query, scope: action.scope },
			};

		case "EXIT_SEARCH_MODE":
			return {
				...state,
				mode: { type: "browse" },
				searchFilters: {},
			};

		case "ENTER_RECENTS_MODE":
			return {
				...state,
				mode: { type: "recents" },
			};

		case "EXIT_RECENTS_MODE":
			return {
				...state,
				mode: { type: "browse" },
			};

		case "ENTER_FILTERED_MODE":
			return {
				...state,
				mode: {
					type: "filtered",
					filters: action.filters,
					label: action.label,
				},
			};

		case "EXIT_FILTERED_MODE":
			return {
				...state,
				mode: { type: "browse" },
			};

		case "ENTER_TAG_MODE":
			return {
				...state,
				mode: { type: "tag", tagId: action.tagId },
			};

		case "EXIT_TAG_MODE":
			return {
				...state,
				mode: { type: "browse" },
			};

		case "SET_SEARCH_FILTERS":
			return {
				...state,
				searchFilters: action.filters,
			};

		case "LOAD_PREFERENCES":
			return {
				...state,
				viewMode: action.viewMode,
				viewSettings: action.viewSettings
					? { ...state.viewSettings, ...action.viewSettings }
					: state.viewSettings,
			};

		default:
			return state;
	}
}

const initialUIState: UIState = {
	viewMode: "grid",
	sortBy: "name",
	viewSettings: defaultViewSettings,
	sidebarVisible: true,
	inspectorVisible: true,
	quickPreviewFileId: null,
	tagModeActive: false,
	mode: { type: "browse" },
	searchFilters: {},
};

function targetToUrl(target: NavigationTarget): string {
	if (target.type === "path") {
		const encoded = encodeURIComponent(JSON.stringify(target.path));
		return `/explorer?path=${encoded}`;
	}

	const params = new URLSearchParams({ view: target.view });
	if (target.id) params.set("id", target.id);
	if (target.params) {
		Object.entries(target.params).forEach(([k, v]) => params.set(k, v));
	}
	return `/explorer?${params.toString()}`;
}

function urlToTarget(search: string): NavigationTarget | null {
	const params = new URLSearchParams(search);

	const pathParam = params.get("path");
	if (pathParam) {
		try {
			const path = JSON.parse(decodeURIComponent(pathParam)) as SdPath;
			return { type: "path", path };
		} catch {
			return null;
		}
	}

	const view = params.get("view");
	if (view) {
		const id = params.get("id") || undefined;
		const extraParams: Record<string, string> = {};
		params.forEach((v, k) => {
			if (k !== "view" && k !== "id") extraParams[k] = v;
		});
		return {
			type: "view",
			view,
			id,
			params:
				Object.keys(extraParams).length > 0 ? extraParams : undefined,
		};
	}

	return null;
}

function getSpaceItemKey(pathname: string, search: string): string {
	if (pathname === "/") return "overview";
	if (pathname === "/recents") return "recents";
	if (pathname === "/favorites") return "favorites";
	if (pathname === "/file-kinds") return "file-kinds";
	if (pathname.startsWith("/tag/")) return `tag:${pathname.slice(5)}`;
	if (pathname === "/explorer" && search) return `explorer:${search}`;
	return pathname;
}

function getPathKey(target: NavigationTarget | null): string {
	if (!target) return "null";
	return targetToKey(target);
}

/**
 * Resolve parent directory path, handling Windows and Unix paths.
 * Returns null if already at root or no parent exists.
 */
function getParentPath(path: string): string | null {
	if (!path) return null;

	// Detect path separator (Windows uses \, Unix uses /)
	const separator = path.includes('\\') ? '\\' : '/';
	const parts = path.split(separator).filter(Boolean);

	// No parts or single part means we're at root
	if (parts.length === 0) return null;
	if (parts.length === 1) {
		// Windows drive root (e.g., "C:") or Unix root
		return null;
	}

	// Remove last part to get parent
	parts.pop();

	// Reconstruct path
	if (separator === '\\') {
		// Windows path
		if (parts[0].endsWith(':')) {
			// Preserve drive letter format: ["C:", "Users"] -> "C:\Users"
			return parts.join(separator);
		}
		// UNC path: ["", "", "server", "share"] -> "\\server\share"
		return parts.join(separator);
	} else {
		// Unix path: always starts with /
		return separator + parts.join(separator);
	}
}

interface ExplorerContextValue {
	currentTarget: NavigationTarget | null;
	currentPath: SdPath | null;
	currentView: {
		view: string;
		id?: string;
		params?: Record<string, string>;
	} | null;

	navigateToPath: (path: SdPath) => void;
	navigateToView: (
		view: string,
		id?: string,
		params?: Record<string, string>,
	) => void;
	goBack: () => void;
	goForward: () => void;
	canGoBack: boolean;
	canGoForward: boolean;
	navigateToParent: () => void;

	viewMode: ViewMode;
	setViewMode: (mode: ViewMode) => void;
	sortBy: SortBy;
	setSortBy: (sort: SortBy) => void;
	viewSettings: ViewSettings;
	setViewSettings: (settings: Partial<ViewSettings>) => void;

	// Column view state (per-tab, stored in TabManager)
	columnStack: SdPath[];
	setColumnStack: (columns: SdPath[]) => void;

	// Scroll position (per-tab, stored in TabManager)
	scrollPosition: { top: number; left: number };
	setScrollPosition: (pos: { top: number; left: number }) => void;

	// Size view transform (per-tab, stored in TabManager)
	sizeViewTransform: { k: number; x: number; y: number };
	setSizeViewTransform: (transform: { k: number; x: number; y: number }) => void;

	sidebarVisible: boolean;
	setSidebarVisible: (visible: boolean) => void;
	inspectorVisible: boolean;
	setInspectorVisible: (visible: boolean) => void;

	quickPreviewFileId: string | null;
	openQuickPreview: (fileId: string) => void;
	closeQuickPreview: () => void;

	currentFiles: File[];
	setCurrentFiles: (files: File[]) => void;

	tagModeActive: boolean;
	setTagModeActive: (active: boolean) => void;

	mode: ExplorerMode;
	enterSearchMode: (query: string, scope?: SearchScope) => void;
	exitSearchMode: () => void;
	enterRecentsMode: () => void;
	exitRecentsMode: () => void;
	enterFilteredMode: (filters: ApiSearchFilters, label: string) => void;
	exitFilteredMode: () => void;
	enterTagMode: (tagId: string) => void;
	exitTagMode: () => void;
	searchFilters: SearchFilters;
	setSearchFilters: (filters: SearchFilters) => void;

	devices: Map<string, Device>;

	loadPreferencesForSpaceItem: (id: string) => void;

	// Tab info
	activeTabId: string;
}

const ExplorerContext = createContext<ExplorerContextValue | null>(null);

// Default per-view state for an isolated (multi-pane) provider. Mirrors the
// TabManager defaults so an isolated pane behaves like a fresh tab without
// touching the shared TabManager store.
const ISOLATED_TAB_DEFAULT: TabExplorerState = {
	viewMode: "grid",
	sortBy: "name",
	gridSize: 120,
	gapSize: 16,
	foldersFirst: true,
	columnStack: [],
	scrollTop: 0,
	scrollLeft: 0,
	sizeViewTransform: { k: 1, x: 0, y: 0 },
};

function isolatedTabReducer(
	state: TabExplorerState,
	patch: Partial<TabExplorerState>,
): TabExplorerState {
	return { ...state, ...patch };
}

interface ExplorerProviderProps {
	children: ReactNode;
	/** Reserved for Phase 2: Will control whether this tab's context should process events/updates */
	isActiveTab?: boolean;
	/**
	 * When true the provider is a self-contained explorer pane: its per-view
	 * state lives in local component state instead of the shared TabManager, and
	 * navigation does NOT touch the router URL. Used to host multiple independent
	 * panes side by side without them fighting over the global tab/URL state.
	 */
	isolated?: boolean;
	/** Starting directory/view for an isolated pane. */
	initialTarget?: NavigationTarget | null;
}

export function ExplorerProvider({
	children,
	isActiveTab: _isActiveTab = true,
	isolated = false,
	initialTarget = null,
}: ExplorerProviderProps) {
	const routerNavigate = useNavigate();
	const location = useLocation();
	const viewPrefs = useViewPreferencesStore();
	const sortPrefs = useSortPreferencesStore();

	// Get per-tab state from TabManager
	const { activeTabId, getExplorerState, updateExplorerState } =
		useTabManager();

	// Local per-view store for isolated panes. Never read/written when the
	// provider is in its default (non-isolated) mode, so the single-pane path is
	// byte-for-byte unchanged.
	const generatedTabId = useId();
	const [isolatedTabState, isolatedTabDispatch] = useReducer(
		isolatedTabReducer,
		ISOLATED_TAB_DEFAULT,
	);

	const effectiveTabId = isolated ? generatedTabId : activeTabId;

	// Memoize tabState to ensure it updates when activeTabId or explorerStates change
	const tabState = useMemo(
		() => (isolated ? isolatedTabState : getExplorerState(activeTabId)),
		[isolated, isolatedTabState, activeTabId, getExplorerState],
	);

	// Routes per-view writes either to the shared TabManager (default) or the
	// isolated local store (multi-pane), keeping every call site identical.
	const updateTabState = useCallback(
		(id: string, patch: Partial<TabExplorerState>) => {
			if (isolated) {
				isolatedTabDispatch(patch);
			} else {
				updateExplorerState(id, patch);
			}
		},
		[isolated, updateExplorerState],
	);

	const [navState, navDispatch] = useReducer(
		navigationReducer,
		initialNavigationState,
	);

	// Seed an isolated pane at its starting directory exactly once.
	const seededRef = useRef(false);
	useEffect(() => {
		if (!isolated || seededRef.current) return;
		seededRef.current = true;
		if (initialTarget) {
			navDispatch({ type: "NAVIGATE", target: initialTarget });
		}
	}, [isolated, initialTarget]);
	const [uiState, uiDispatch] = useReducer(uiReducer, initialUIState);
	const [currentFiles, setCurrentFiles] = useReducer(
		(_: File[], files: File[]) => files,
		[] as File[],
	);

	// Parse columnStack from TabManager (stored as JSON strings)
	// Must depend on activeTabId to recalculate when switching tabs
	const columnStack = useMemo((): SdPath[] => {
		if (!tabState.columnStack || tabState.columnStack.length === 0) {
			return [];
		}
		try {
			return tabState.columnStack.map((s) => JSON.parse(s) as SdPath);
		} catch {
			return [];
		}
	}, [activeTabId, tabState.columnStack]);

	const setColumnStack = useCallback(
		(columns: SdPath[]) => {
			updateTabState(effectiveTabId, {
				columnStack: columns.map((c) => JSON.stringify(c)),
			});
		},
		[effectiveTabId, updateTabState],
	);

	const scrollPosition = useMemo(
		() => ({
			top: tabState.scrollTop,
			left: tabState.scrollLeft,
		}),
		[activeTabId, tabState.scrollTop, tabState.scrollLeft],
	);

	const setScrollPosition = useCallback(
		(pos: { top: number; left: number }) => {
			updateTabState(effectiveTabId, {
				scrollTop: pos.top,
				scrollLeft: pos.left,
			});
		},
		[effectiveTabId, updateTabState],
	);

	const sizeViewTransform = useMemo(
		() => tabState.sizeViewTransform ?? { k: 1, x: 0, y: 0 },
		[activeTabId, tabState.sizeViewTransform],
	);

	const setSizeViewTransform = useCallback(
		(transform: { k: number; x: number; y: number }) => {
			updateTabState(effectiveTabId, {
				sizeViewTransform: transform,
			});
		},
		[effectiveTabId, updateTabState],
	);

	const currentTarget = navState.history[navState.index] ?? null;
	const canGoBack = navState.index > 0;
	const canGoForward = navState.index < navState.history.length - 1;

	const currentPath = useMemo(() => {
		if (currentTarget?.type === "path") return currentTarget.path;
		return null;
	}, [currentTarget]);

	const currentView = useMemo(() => {
		if (currentTarget?.type === "view") {
			return {
				view: currentTarget.view,
				id: currentTarget.id,
				params: currentTarget.params,
			};
		}
		return null;
	}, [currentTarget]);

	const devicesQuery = useNormalizedQuery<ListLibraryDevicesInput, Device[]>({
		query: "devices.list",
		input: { include_offline: true, include_details: false },
		resourceType: "device",
	});

	const devices = useMemo(() => {
		const list = devicesQuery.data ?? [];
		return new Map(list.map((d) => [d.id, d]));
	}, [devicesQuery.data]);

	// Exclude currentTarget from deps to prevent infinite sync loops.
	useEffect(() => {
		// Isolated panes own their navigation and never read the shared URL.
		if (isolated) return;
		const target = urlToTarget(location.search);
		if (target && !targetsEqual(target, currentTarget)) {
			navDispatch({ type: "SYNC", target });
		}
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [location.search, isolated]);

	const pathKey = getPathKey(currentTarget);

	useEffect(() => {
		const savedSort = sortPrefs.getPreferences(pathKey);
		if (savedSort) {
			uiDispatch({ type: "SET_SORT_BY", sort: savedSort as SortBy });
		}
	}, [pathKey, sortPrefs]);

	// "datetaken" only applies to media view; fall back to "modified" elsewhere.
	useEffect(() => {
		if (isolated) return;
		if (uiState.viewMode === "media" && uiState.sortBy === "type") {
			uiDispatch({ type: "SET_SORT_BY", sort: "datetaken" });
			sortPrefs.setPreferences(pathKey, "datetaken");
		} else if (
			uiState.viewMode !== "media" &&
			uiState.sortBy === "datetaken"
		) {
			uiDispatch({ type: "SET_SORT_BY", sort: "modified" });
			sortPrefs.setPreferences(pathKey, "modified");
		}
	}, [uiState.viewMode, uiState.sortBy, pathKey, sortPrefs]);

	const navigateToPath = useCallback(
		(path: SdPath) => {
			const target: NavigationTarget = { type: "path", path };
			navDispatch({ type: "NAVIGATE", target });
			if (!isolated) routerNavigate(targetToUrl(target));
			// Exit special modes when navigating to a path
			uiDispatch({ type: "EXIT_SEARCH_MODE" });
			uiDispatch({ type: "EXIT_TAG_MODE" });
		},
		[routerNavigate, isolated],
	);

	const navigateToView = useCallback(
		(view: string, id?: string, params?: Record<string, string>) => {
			const target: NavigationTarget = { type: "view", view, id, params };
			navDispatch({ type: "NAVIGATE", target });
			if (!isolated) routerNavigate(targetToUrl(target));
			// Exit special modes when navigating
			uiDispatch({ type: "EXIT_SEARCH_MODE" });
			uiDispatch({ type: "EXIT_TAG_MODE" });
		},
		[routerNavigate, isolated],
	);

	const goBack = useCallback(() => {
		navDispatch({ type: "GO_BACK" });
		const targetIndex = navState.index - 1;
		if (targetIndex >= 0) {
			const target = navState.history[targetIndex];
			if (!isolated)
				routerNavigate(targetToUrl(target), { replace: true });
			// Exit special modes when navigating
			uiDispatch({ type: "EXIT_SEARCH_MODE" });
			uiDispatch({ type: "EXIT_TAG_MODE" });
		}
	}, [navState.index, navState.history, routerNavigate, isolated]);

	const goForward = useCallback(() => {
		navDispatch({ type: "GO_FORWARD" });
		const targetIndex = navState.index + 1;
		if (targetIndex < navState.history.length) {
			const target = navState.history[targetIndex];
			if (!isolated)
				routerNavigate(targetToUrl(target), { replace: true });
			// Exit special modes when navigating
			uiDispatch({ type: "EXIT_SEARCH_MODE" });
			uiDispatch({ type: "EXIT_TAG_MODE" });
		}
	}, [navState.index, navState.history, routerNavigate, isolated]);

	const navigateToParent = useCallback(() => {
		if (!currentTarget || currentTarget.type !== 'path') {
			return;
		}

		const pathObj = currentTarget.path;
		let currentPathString: string | null = null;
		let pathType: 'Physical' | 'Cloud' | null = null;

		if ('Physical' in pathObj && pathObj.Physical) {
			currentPathString = pathObj.Physical.path;
			pathType = 'Physical';
		} else if ('Cloud' in pathObj && pathObj.Cloud) {
			currentPathString = pathObj.Cloud.path;
			pathType = 'Cloud';
		}

		if (!currentPathString || !pathType) return;

		const parentPath = getParentPath(currentPathString);
		if (!parentPath) {
			// Already at root, silent no-op
			return;
		}

		// Navigate to parent using the same format as current path
		if (pathType === 'Physical' && 'Physical' in pathObj) {
			navigateToPath({
				Physical: {
					device_slug: pathObj.Physical.device_slug,
					path: parentPath
				}
			});
		} else if (pathType === 'Cloud' && 'Cloud' in pathObj) {
			navigateToPath({
				Cloud: {
					service: pathObj.Cloud.service,
					identifier: pathObj.Cloud.identifier,
					path: parentPath
				}
			});
		}
	}, [currentTarget, navigateToPath]);

	const spaceKey = getSpaceItemKey(location.pathname, location.search);

	// View settings from TabManager (per-tab)
	const viewMode = tabState.viewMode as ViewMode;
	const sortByValue = tabState.sortBy as SortBy;
	const viewSettings: ViewSettings = useMemo(
		() => ({
			gridSize: tabState.gridSize,
			gapSize: tabState.gapSize,
			foldersFirst: tabState.foldersFirst,
			showFileSize: uiState.viewSettings.showFileSize,
			columnWidth: uiState.viewSettings.columnWidth,
			sizeViewItemLimit: uiState.viewSettings.sizeViewItemLimit,
		}),
		[
			activeTabId,
			tabState.gridSize,
			tabState.gapSize,
			tabState.foldersFirst,
			uiState.viewSettings.showFileSize,
			uiState.viewSettings.columnWidth,
			uiState.viewSettings.sizeViewItemLimit,
		],
	);

	const setViewMode = useCallback(
		(mode: ViewMode) => {
			updateTabState(effectiveTabId, {
				viewMode: mode as TabViewMode,
			});
			if (!isolated) viewPrefs.setPreferences(spaceKey, { viewMode: mode });
		},
		[effectiveTabId, updateTabState, spaceKey, viewPrefs, isolated],
	);

	const setSortBy = useCallback(
		(sort: SortBy) => {
			updateTabState(effectiveTabId, {
				sortBy: sort as TabSortBy,
			});
			if (!isolated) sortPrefs.setPreferences(pathKey, sort);
		},
		[effectiveTabId, updateTabState, pathKey, sortPrefs, isolated],
	);

	const setViewSettings = useCallback(
		(settings: Partial<ViewSettings>) => {
			// Update tab state for tab-specific settings
			updateTabState(effectiveTabId, {
				gridSize: settings.gridSize ?? tabState.gridSize,
				gapSize: settings.gapSize ?? tabState.gapSize,
				foldersFirst: settings.foldersFirst ?? tabState.foldersFirst,
			});

			// Update UI state for global settings (showFileSize, sizeViewItemLimit)
			if (settings.showFileSize !== undefined || settings.sizeViewItemLimit !== undefined) {
				uiDispatch({
					type: "SET_VIEW_SETTINGS",
					settings,
				});
			}

			// Save to preferences
			if (!isolated) {
				viewPrefs.setPreferences(spaceKey, {
					viewSettings: { ...viewSettings, ...settings },
				});
			}
		},
		[
			effectiveTabId,
			updateTabState,
			tabState,
			spaceKey,
			viewSettings,
			viewPrefs,
		],
	);

	const setSidebarVisible = useCallback((visible: boolean) => {
		uiDispatch({ type: "SET_SIDEBAR_VISIBLE", visible });
	}, []);

	const setInspectorVisible = useCallback((visible: boolean) => {
		uiDispatch({ type: "SET_INSPECTOR_VISIBLE", visible });
	}, []);

	const openQuickPreview = useCallback((fileId: string) => {
		uiDispatch({ type: "SET_QUICK_PREVIEW", fileId });
	}, []);

	const closeQuickPreview = useCallback(() => {
		uiDispatch({ type: "SET_QUICK_PREVIEW", fileId: null });
	}, []);

	const setTagModeActive = useCallback((active: boolean) => {
		uiDispatch({ type: "SET_TAG_MODE", active });
	}, []);

	const enterSearchMode = useCallback(
		(query: string, scope: SearchScope = "folder") => {
			uiDispatch({ type: "ENTER_SEARCH_MODE", query, scope });
		},
		[],
	);

	const exitSearchMode = useCallback(() => {
		uiDispatch({ type: "EXIT_SEARCH_MODE" });
	}, []);

	const enterRecentsMode = useCallback(() => {
		uiDispatch({ type: "ENTER_RECENTS_MODE" });
	}, []);

	const exitRecentsMode = useCallback(() => {
		uiDispatch({ type: "EXIT_RECENTS_MODE" });
	}, []);

	const enterFilteredMode = useCallback(
		(filters: ApiSearchFilters, label: string) => {
			uiDispatch({ type: "ENTER_FILTERED_MODE", filters, label });
		},
		[],
	);

	const exitFilteredMode = useCallback(() => {
		uiDispatch({ type: "EXIT_FILTERED_MODE" });
	}, []);

	const enterTagMode = useCallback((tagId: string) => {
		uiDispatch({ type: "ENTER_TAG_MODE", tagId });
	}, []);

	const exitTagMode = useCallback(() => {
		uiDispatch({ type: "EXIT_TAG_MODE" });
	}, []);

	const setSearchFilters = useCallback((filters: SearchFilters) => {
		uiDispatch({ type: "SET_SEARCH_FILTERS", filters });
	}, []);

	const loadPreferencesForSpaceItem = useCallback(
		(id: string) => {
			const prefs = viewPrefs.getPreferences(id);
			if (prefs) {
				uiDispatch({
					type: "LOAD_PREFERENCES",
					viewMode: prefs.viewMode,
					viewSettings: prefs.viewSettings,
				});
			}
		},
		[viewPrefs],
	);

	const value = useMemo<ExplorerContextValue>(
		() => ({
			currentTarget,
			currentPath,
			currentView,
			navigateToPath,
			navigateToView,
			goBack,
			goForward,
			canGoBack,
			canGoForward,
			navigateToParent,
			viewMode,
			setViewMode,
			sortBy: sortByValue,
			setSortBy,
			viewSettings,
			setViewSettings,
			columnStack,
			setColumnStack,
			scrollPosition,
			setScrollPosition,
			sizeViewTransform,
			setSizeViewTransform,
			sidebarVisible: uiState.sidebarVisible,
			setSidebarVisible,
			inspectorVisible: uiState.inspectorVisible,
			setInspectorVisible,
			quickPreviewFileId: uiState.quickPreviewFileId,
			openQuickPreview,
			closeQuickPreview,
			currentFiles,
			setCurrentFiles,
			tagModeActive: uiState.tagModeActive,
			setTagModeActive,
			mode: uiState.mode,
			enterSearchMode,
			exitSearchMode,
			enterRecentsMode,
			exitRecentsMode,
			enterFilteredMode,
			exitFilteredMode,
			enterTagMode,
			exitTagMode,
			searchFilters: uiState.searchFilters,
			setSearchFilters,
			devices,
			loadPreferencesForSpaceItem,
			activeTabId: effectiveTabId,
		}),
		[
			currentTarget,
			currentPath,
			currentView,
			navigateToPath,
			navigateToView,
			goBack,
			goForward,
			canGoBack,
			canGoForward,
			navigateToParent,
			viewMode,
			setViewMode,
			sortByValue,
			setSortBy,
			viewSettings,
			setViewSettings,
			columnStack,
			setColumnStack,
			scrollPosition,
			setScrollPosition,
			sizeViewTransform,
			setSizeViewTransform,
			uiState.sidebarVisible,
			setSidebarVisible,
			uiState.inspectorVisible,
			setInspectorVisible,
			uiState.quickPreviewFileId,
			openQuickPreview,
			closeQuickPreview,
			currentFiles,
			uiState.tagModeActive,
			setTagModeActive,
			uiState.mode,
			enterSearchMode,
			exitSearchMode,
			enterRecentsMode,
			exitRecentsMode,
			enterFilteredMode,
			exitFilteredMode,
			enterTagMode,
			exitTagMode,
			uiState.searchFilters,
			setSearchFilters,
			devices,
			loadPreferencesForSpaceItem,
			activeTabId,
		],
	);

	return (
		<ExplorerContext.Provider value={value}>
			{children}
		</ExplorerContext.Provider>
	);
}

export function useExplorer(): ExplorerContextValue {
	const context = useContext(ExplorerContext);
	if (!context) {
		throw new Error("useExplorer must be used within an ExplorerProvider");
	}
	return context;
}

export {
	getSpaceItemKey,
	getSpaceItemKey as getSpaceItemKeyFromRoute,
	targetToKey,
	targetsEqual,
};

export type VirtualView = {
	view: string;
	id?: string;
	params?: Record<string, string>;
};