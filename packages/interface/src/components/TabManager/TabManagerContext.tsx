import type {
	OrganizeItemFilter,
	OrganizeItemSort,
	OrganizeSortDirection
} from '@sd/ts-client';
import {
	createContext,
	useCallback,
	useEffect,
	useMemo,
	useState,
	type ReactNode
} from 'react';
import {createBrowserRouter, type RouteObject} from 'react-router-dom';
import {i18n} from '../../i18n';

type Router = ReturnType<typeof createBrowserRouter>;

/**
 * Derives a tab title from the current route pathname and search params
 */
function deriveTitleFromPath(pathname: string, search: string): string {
	const t = (key: string) => i18n.t(key, {ns: 'sidebar'});

	const routeTitles: Record<string, string> = {
		'/': t('palette.overview'),
		'/favorites': t('palette.favorites'),
		'/recents': t('palette.recents'),
		'/file-kinds': t('palette.fileKinds'),
		'/search': i18n.t('search.placeholder', {ns: 'explorer'}) || 'Search',
		'/jobs': t('jobs.title'),
		'/daemon': t('navigation.daemon')
	};

	if (routeTitles[pathname]) {
		return routeTitles[pathname];
	}

	if (pathname.startsWith('/tag/')) {
		const tagId = pathname.split('/')[2];
		return tagId
			? t('fallbacks.tagShort').replace('{{id}}', tagId.slice(0, 8))
			: t('fallbacks.tag');
	}

	if (pathname === '/explorer' && search) {
		const params = new URLSearchParams(search);

		const view = params.get('view');
		if (view === 'device') {
			return t('fallbacks.thisDevice');
		}

		const pathParam = params.get('path');
		if (pathParam) {
			try {
				const sdPath = JSON.parse(decodeURIComponent(pathParam));
				if (sdPath?.Physical?.path) {
					const fullPath = sdPath.Physical.path as string;
					const parts = fullPath.split('/').filter(Boolean);
					return parts[parts.length - 1] || t('fallbacks.explorer');
				}
			} catch {
				// Fall through
			}
		}
		return t('fallbacks.explorer');
	}

	return t('fallbacks.spacedrive');
}

// ============================================================================
// Types
// ============================================================================

export type ViewMode =
	| 'grid'
	| 'list'
	| 'column'
	| 'media'
	| 'masonry'
	| 'size';
export type SortBy =
	| 'name'
	| 'size'
	| 'date_modified'
	| 'date_created'
	| 'kind';

export interface Tab {
	id: string;
	title: string;
	icon: string | null;
	isPinned: boolean;
	lastActive: number;
	savedPath: string;
}

/**
 * All explorer-related state for a single tab.
 * This is the single source of truth - no sync effects needed.
 */
export interface TabExplorerState {
	// View settings
	viewMode: ViewMode;
	sortBy: SortBy;
	gridSize: number;
	gapSize: number;
	foldersFirst: boolean;

	// Column view state (serialized SdPath[] as JSON strings)
	columnStack: string[];

	// Scroll position
	scrollTop: number;
	scrollLeft: number;

	// Size view transform (zoom + pan)
	sizeViewTransform: {k: number; x: number; y: number};
}

export interface OrganizeTabState {
	currentItemId: string | null;
	viewMode: 'grid' | 'list';
	filter: OrganizeItemFilter;
	sort: OrganizeItemSort;
	direction: OrganizeSortDirection;
	scrollTop: number;
}

const DEFAULT_ORGANIZE_STATE: OrganizeTabState = {
	currentItemId: null,
	viewMode: 'grid',
	filter: 'All',
	sort: 'Name',
	direction: 'Asc',
	scrollTop: 0
};

/** Default explorer state for new tabs */
const DEFAULT_EXPLORER_STATE: TabExplorerState = {
	viewMode: 'grid',
	sortBy: 'name',
	gridSize: 120,
	gapSize: 16,
	foldersFirst: true,
	columnStack: [],
	scrollTop: 0,
	scrollLeft: 0,
	sizeViewTransform: {k: 1, x: 0, y: 0}
};

// ============================================================================
// Persistence
// ============================================================================

const STORAGE_KEY = 'sd-tabs-state';

interface PersistedState {
	tabs: Tab[];
	activeTabId: string;
	explorerStates: Record<string, TabExplorerState>;
	organizeStates: Record<string, OrganizeTabState>;
	defaultNewTabPath: string;
}

function loadPersistedState(): PersistedState | null {
	try {
		const stored = localStorage.getItem(STORAGE_KEY);
		if (!stored) return null;

		const parsed = JSON.parse(stored) as PersistedState;

		// Validate structure
		if (
			!Array.isArray(parsed.tabs) ||
			typeof parsed.activeTabId !== 'string' ||
			typeof parsed.explorerStates !== 'object' ||
			parsed.explorerStates === null
		) {
			return null;
		}

		if (
			typeof parsed.organizeStates !== 'object' ||
			parsed.organizeStates === null
		) {
			parsed.organizeStates = {};
		}

		for (const state of Object.values(parsed.explorerStates)) {
			if ((state.viewMode as string) === 'organize') {
				state.viewMode = 'grid';
			}
		}

		return parsed;
	} catch {
		return null;
	}
}

function savePersistedState(state: PersistedState): void {
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
	} catch {
		// Silently fail if localStorage is unavailable
	}
}

// ============================================================================
// Context
// ============================================================================

interface TabManagerContextValue {
	// Tab management
	tabs: Tab[];
	activeTabId: string;
	router: Router;
	createTab: (title?: string, path?: string) => void;
	closeTab: (tabId: string) => void;
	switchTab: (tabId: string) => void;
	updateTabTitle: (tabId: string, title: string) => void;
	updateTabPath: (tabId: string, path: string) => void;
	reorderTabs: (activeId: string, overId: string) => void;
	nextTab: () => void;
	previousTab: () => void;
	selectTabAtIndex: (index: number) => void;
	setDefaultNewTabPath: (path: string) => void;

	// Explorer state (per-tab)
	getExplorerState: (tabId: string) => TabExplorerState;
	updateExplorerState: (
		tabId: string,
		updates: Partial<TabExplorerState>
	) => void;
	getOrganizeState: (tabId: string, taskId: string) => OrganizeTabState;
	updateOrganizeState: (
		tabId: string,
		taskId: string,
		updates: Partial<OrganizeTabState>
	) => void;

	// Selection state (per-tab, ephemeral - not persisted)
	getSelectionIds: (tabId: string) => string[];
	updateSelectionIds: (tabId: string, fileIds: string[]) => void;
}

const TabManagerContext = createContext<TabManagerContextValue | null>(null);

// ============================================================================
// Provider
// ============================================================================

interface TabManagerProviderProps {
	children: ReactNode;
	routes: RouteObject[];
}

export function TabManagerProvider({
	children,
	routes
}: TabManagerProviderProps) {
	const router = useMemo(() => createBrowserRouter(routes), [routes]);

	const [tabs, setTabs] = useState<Tab[]>(() => {
		const persisted = loadPersistedState();
		if (persisted && persisted.tabs.length > 0) {
			return persisted.tabs;
		}

		const initialTabId = crypto.randomUUID();
		return [
			{
				id: initialTabId,
				title: i18n.t('palette.overview', {ns: 'sidebar'}),
				icon: null,
				isPinned: false,
				lastActive: Date.now(),
				savedPath: '/'
			}
		];
	});

	const [activeTabId, setActiveTabId] = useState<string>(() => {
		const persisted = loadPersistedState();
		if (persisted && persisted.activeTabId) {
			// Verify the activeTabId exists in tabs
			const tabExists = persisted.tabs.some(
				(t) => t.id === persisted.activeTabId
			);
			if (tabExists) return persisted.activeTabId;
		}
		return tabs[0].id;
	});

	const [explorerStates, setExplorerStates] = useState<
		Map<string, TabExplorerState>
	>(() => {
		const persisted = loadPersistedState();
		if (persisted && persisted.explorerStates) {
			return new Map(Object.entries(persisted.explorerStates));
		}

		const initialMap = new Map<string, TabExplorerState>();
		initialMap.set(tabs[0].id, {...DEFAULT_EXPLORER_STATE});
		return initialMap;
	});

	const [organizeStates, setOrganizeStates] = useState<
		Record<string, OrganizeTabState>
	>(() => {
		const persisted = loadPersistedState();
		return persisted?.organizeStates ?? {};
	});

	// Per-tab selection state (ephemeral, not persisted to localStorage)
	const [selectionStates, setSelectionStates] = useState<
		Map<string, string[]>
	>(() => {
		const initialMap = new Map<string, string[]>();
		// Initialize with empty selection for first tab
		initialMap.set(tabs[0].id, []);
		return initialMap;
	});

	const [defaultNewTabPath, setDefaultNewTabPathState] = useState<string>(
		() => {
			const persisted = loadPersistedState();
			return persisted?.defaultNewTabPath ?? '/';
		}
	);

	// ========================================================================
	// Persistence
	// ========================================================================

	useEffect(() => {
		const explorerStatesObject = Object.fromEntries(explorerStates);

		savePersistedState({
			tabs,
			activeTabId,
			explorerStates: explorerStatesObject,
			organizeStates,
			defaultNewTabPath
		});
	}, [tabs, activeTabId, explorerStates, organizeStates, defaultNewTabPath]);

	// ========================================================================
	// Tab management
	// ========================================================================

	const setDefaultNewTabPath = useCallback((path: string) => {
		setDefaultNewTabPathState(path);
	}, []);

	const createTab = useCallback(
		(title?: string, path?: string) => {
			const tabPath = path ?? defaultNewTabPath;
			const [pathname, search = ''] = tabPath.split('?');
			const derivedTitle =
				title ||
				deriveTitleFromPath(pathname, search ? `?${search}` : '');

			const newTab: Tab = {
				id: crypto.randomUUID(),
				title: derivedTitle,
				icon: null,
				isPinned: false,
				lastActive: Date.now(),
				savedPath: tabPath
			};

			// Initialize explorer state for the new tab
			setExplorerStates((prev) =>
				new Map(prev).set(newTab.id, {...DEFAULT_EXPLORER_STATE})
			);

			// Initialize empty selection state for the new tab
			setSelectionStates((prev) => new Map(prev).set(newTab.id, []));

			setTabs((prev) => [...prev, newTab]);
			setActiveTabId(newTab.id);
		},
		[defaultNewTabPath]
	);

	const closeTab = useCallback(
		(tabId: string) => {
			setTabs((prev) => {
				const filtered = prev.filter((t) => t.id !== tabId);

				if (filtered.length === 0) {
					return prev;
				}

				if (tabId === activeTabId) {
					const currentIndex = prev.findIndex((t) => t.id === tabId);
					const newIndex = Math.max(0, currentIndex - 1);
					const newActiveTab = filtered[newIndex] || filtered[0];
					if (newActiveTab) {
						setActiveTabId(newActiveTab.id);
					}
				}

				return filtered;
			});

			// Clean up explorer state for closed tab
			setExplorerStates((prev) => {
				const next = new Map(prev);
				next.delete(tabId);
				return next;
			});

			setOrganizeStates((prev) => {
				const prefix = `${tabId}:`;
				return Object.fromEntries(
					Object.entries(prev).filter(
						([key]) => !key.startsWith(prefix)
					)
				);
			});

			// Clean up selection state for closed tab
			setSelectionStates((prev) => {
				const next = new Map(prev);
				next.delete(tabId);
				return next;
			});
		},
		[activeTabId]
	);

	const switchTab = useCallback(
		(newTabId: string) => {
			if (newTabId === activeTabId) {
				return;
			}

			setTabs((prev) =>
				prev.map((tab) =>
					tab.id === newTabId ? {...tab, lastActive: Date.now()} : tab
				)
			);

			setActiveTabId(newTabId);
		},
		[activeTabId]
	);

	const updateTabTitle = useCallback((tabId: string, title: string) => {
		setTabs((prev) =>
			prev.map((tab) => (tab.id === tabId ? {...tab, title} : tab))
		);
	}, []);

	const updateTabPath = useCallback((tabId: string, path: string) => {
		setTabs((prev) =>
			prev.map((tab) =>
				tab.id === tabId ? {...tab, savedPath: path} : tab
			)
		);
	}, []);

	const reorderTabs = useCallback((activeId: string, overId: string) => {
		setTabs((prev) => {
			const oldIndex = prev.findIndex((tab) => tab.id === activeId);
			const newIndex = prev.findIndex((tab) => tab.id === overId);

			if (oldIndex === -1 || newIndex === -1 || oldIndex === newIndex) {
				return prev;
			}

			const newTabs = [...prev];
			const [movedTab] = newTabs.splice(oldIndex, 1);
			newTabs.splice(newIndex, 0, movedTab);

			return newTabs;
		});
	}, []);

	const nextTab = useCallback(() => {
		const currentIndex = tabs.findIndex((t) => t.id === activeTabId);
		const nextIndex = (currentIndex + 1) % tabs.length;
		switchTab(tabs[nextIndex].id);
	}, [tabs, activeTabId, switchTab]);

	const previousTab = useCallback(() => {
		const currentIndex = tabs.findIndex((t) => t.id === activeTabId);
		const prevIndex = (currentIndex - 1 + tabs.length) % tabs.length;
		switchTab(tabs[prevIndex].id);
	}, [tabs, activeTabId, switchTab]);

	const selectTabAtIndex = useCallback(
		(index: number) => {
			if (index >= 0 && index < tabs.length) {
				switchTab(tabs[index].id);
			}
		},
		[tabs, switchTab]
	);

	// ========================================================================
	// Explorer state (per-tab)
	// ========================================================================

	const getExplorerState = useCallback(
		(tabId: string): TabExplorerState => {
			return explorerStates.get(tabId) ?? {...DEFAULT_EXPLORER_STATE};
		},
		[explorerStates]
	);

	const updateExplorerState = useCallback(
		(tabId: string, updates: Partial<TabExplorerState>) => {
			setExplorerStates((prev) => {
				const current = prev.get(tabId) ?? {
					...DEFAULT_EXPLORER_STATE
				};
				return new Map(prev).set(tabId, {...current, ...updates});
			});
		},
		[]
	);

	const getOrganizeState = useCallback(
		(tabId: string, taskId: string): OrganizeTabState => ({
			...DEFAULT_ORGANIZE_STATE,
			...(organizeStates[`${tabId}:${taskId}`] ?? {})
		}),
		[organizeStates]
	);

	const updateOrganizeState = useCallback(
		(tabId: string, taskId: string, updates: Partial<OrganizeTabState>) => {
			const key = `${tabId}:${taskId}`;
			setOrganizeStates((prev) => ({
				...prev,
				[key]: {
					...DEFAULT_ORGANIZE_STATE,
					...(prev[key] ?? {}),
					...updates
				}
			}));
		},
		[]
	);

	// ========================================================================
	// Selection state (per-tab)
	// ========================================================================

	const getSelectionIds = useCallback(
		(tabId: string): string[] => {
			return selectionStates.get(tabId) ?? [];
		},
		[selectionStates]
	);

	const updateSelectionIds = useCallback(
		(tabId: string, fileIds: string[]) => {
			setSelectionStates((prev) => new Map(prev).set(tabId, fileIds));
		},
		[]
	);

	// ========================================================================
	// Context value
	// ========================================================================

	const value = useMemo<TabManagerContextValue>(
		() => ({
			tabs,
			activeTabId,
			router,
			createTab,
			closeTab,
			switchTab,
			updateTabTitle,
			updateTabPath,
			reorderTabs,
			nextTab,
			previousTab,
			selectTabAtIndex,
			setDefaultNewTabPath,
			getExplorerState,
			updateExplorerState,
			getOrganizeState,
			updateOrganizeState,
			getSelectionIds,
			updateSelectionIds
		}),
		[
			tabs,
			activeTabId,
			router,
			createTab,
			closeTab,
			switchTab,
			updateTabTitle,
			updateTabPath,
			reorderTabs,
			nextTab,
			previousTab,
			selectTabAtIndex,
			setDefaultNewTabPath,
			getExplorerState,
			updateExplorerState,
			getOrganizeState,
			updateOrganizeState,
			getSelectionIds,
			updateSelectionIds
		]
	);

	return (
		<TabManagerContext.Provider value={value}>
			{children}
		</TabManagerContext.Provider>
	);
}

export {TabManagerContext};
