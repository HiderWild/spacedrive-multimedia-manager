import {
	ArrowLeft,
	ArrowRight,
	Columns,
	FilmStrip,
	Info,
	MagicWand,
	SidebarSimple,
	Tag as TagIcon
} from '@phosphor-icons/react';
import {CircleButton, CircleButtonGroup} from '@spacedrive/primitives';
import {getContentKind} from '@sd/ts-client';
import {useCallback, useEffect, useMemo, useState} from 'react';
import {useTranslation} from 'react-i18next';
import {TopBarItem, TopBarPortal} from '../../TopBar';
import {usePlatform} from '../../contexts/PlatformContext';
import {ExpandableSearchButton} from './components/ExpandableSearchButton';
import {PathBar} from './components/PathBar';
import {VirtualPathBar} from './components/VirtualPathBar';
import {useExplorer, type ViewMode} from './context';
import {useExplorerFiles} from './hooks/useExplorerFiles';
import {useVirtualListing} from './hooks/useVirtualListing';
import {canUseOrganizeView} from './organize/organizeAvailability';
import {ExplorerPaneBody, PaneLayout, usePanes} from './panes';
import {useSelection} from './SelectionContext';
import {SortMenu, SortMenuPanel} from './SortMenu';
import {ViewModeMenu, ViewModeMenuPanel} from './ViewModeMenu';
import {EmptyView} from './views/EmptyView';
import {ViewSettings, ViewSettingsPanel} from './ViewSettings';
import {WanderOverlay} from './wander';
import {openTranscodeDialog} from './transcode';

export function ExplorerView() {
	const {t} = useTranslation('explorer');
	const {
		sidebarVisible,
		setSidebarVisible,
		inspectorVisible,
		setInspectorVisible,
		tagModeActive,
		setTagModeActive,
		viewMode,
		setViewMode,
		sortBy,
		setSortBy,
		viewSettings,
		setViewSettings,
		goBack,
		goForward,
		canGoBack,
		canGoForward,
		currentPath,
		currentView,
		currentTarget,
		navigateToPath,
		devices,
		quickPreviewFileId,
		mode,
		enterSearchMode,
		exitSearchMode,
		currentFiles,
		columnStack
	} = useExplorer();
	const platform = usePlatform();

	const {isVirtualView} = useVirtualListing();
	const isPreviewActive = !!quickPreviewFileId;

	const organizeAvailable = useMemo(
		() => canUseOrganizeView({ mode, currentPath, platform }),
		[mode, currentPath, platform]
	);

	// Multi-pane layout state. Defaults to a single pane, in which case
	// PaneLayout renders the primary content unchanged.
	const {panes, sizes, focusedId, splitPane, closePane, focusPane, resize} =
		usePanes();

	// Open a new pane to the right, seeded at the current location so the user
	// starts from where they are and can navigate it independently.
	const handleSplit = useCallback(() => {
		splitPane(currentTarget);
	}, [splitPane, currentTarget]);

	// Live media set + pagination for the immersive "wander" slideshow. Shares
	// the explorer's existing data hook so no separate data layer is introduced.
	const {
		files: wanderFiles,
		hasNextPage: wanderHasNextPage,
		fetchNextPage: wanderFetchNextPage
	} = useExplorerFiles();
	const {selectedFiles} = useSelection();
	const [wanderOpen, setWanderOpen] = useState(false);

	// Start the slideshow on the selected file (matched by id so it works even
	// when wander's paged set differs from the active view's), else the first.
	const wanderStartIndex = useMemo(() => {
		const firstSelectedId = selectedFiles[0]?.id;
		if (!firstSelectedId) return 0;
		const idx = wanderFiles.findIndex((f) => f.id === firstSelectedId);
		return idx >= 0 ? idx : 0;
	}, [selectedFiles, wanderFiles]);

	const canWander = wanderFiles.length > 0;

	// Transcode operates on the current selection, filtered to video entries.
	const selectedVideos = useMemo(
		() => selectedFiles.filter((f) => getContentKind(f) === 'video'),
		[selectedFiles]
	);
	const canTranscode = selectedVideos.length > 0;

	// In column view, the path bar should reflect the deepest column, not the root
	const pathBarPath = useMemo(() => {
		if (viewMode === 'column' && columnStack.length > 1) {
			return columnStack[columnStack.length - 1];
		}
		return currentPath;
	}, [viewMode, columnStack, currentPath]);

	const [searchValue, setSearchValue] = useState('');

	const handleSearchChange = useCallback(
		(value: string) => {
			setSearchValue(value);

			if (value.length >= 2) {
				const timeoutId = setTimeout(() => {
					enterSearchMode(value);
				}, 300);
				return () => clearTimeout(timeoutId);
			} else if (value.length === 0 && mode.type === 'search') {
				exitSearchMode();
			}
		},
		[enterSearchMode, exitSearchMode, mode.type]
	);

	const handleSearchClear = useCallback(() => {
		setSearchValue('');
		exitSearchMode();
	}, [exitSearchMode]);

	useEffect(() => {
		if (mode.type !== 'search') {
			setSearchValue('');
		}
	}, [mode.type]);

	// When leaving column view, navigate to the deepest column so the
	// new view shows the directory the user was actually looking at.
	const handleViewModeChange = useCallback(
		(newMode: string) => {
			if (
				viewMode === 'column' &&
				newMode !== 'column' &&
				columnStack.length > 1
			) {
				navigateToPath(columnStack[columnStack.length - 1]);
			}
			setViewMode(newMode as ViewMode);
		},
		[viewMode, columnStack, navigateToPath, setViewMode]
	);

	// Memoize submenu content to prevent infinite re-renders
	const viewModeSubmenu = useMemo(
		() => (
			<ViewModeMenuPanel
				viewMode={viewMode}
				onViewModeChange={handleViewModeChange}
				organizeAvailable={organizeAvailable}
			/>
		),
		[viewMode, handleViewModeChange, organizeAvailable]
	);

	const viewSettingsSubmenu = useMemo(
		() => (
			<ViewSettingsPanel
				viewSettings={viewSettings}
				setViewSettings={setViewSettings}
				viewMode={viewMode}
				totalFileCount={currentFiles.length}
			/>
		),
		[viewSettings, setViewSettings, viewMode, currentFiles.length]
	);

	const sortSubmenu = useMemo(
		() => (
			<SortMenuPanel
				sortBy={sortBy}
				onSortChange={setSortBy}
				viewMode={viewMode}
			/>
		),
		[sortBy, setSortBy, viewMode]
	);

	// Allow rendering if we have a currentPath, a virtual view, or a special mode
	// (tag/recents/filtered). Only the plain "browse" mode without a path is empty.
	if (!currentPath && !isVirtualView && mode.type === 'browse') {
		return <EmptyView />;
	}

	return (
		<>
			{!isPreviewActive && (
				<TopBarPortal
					left={
						<>
							<TopBarItem
								id="sidebar-toggle"
								label={t('topBar.sidebar')}
								priority="normal"
								onClick={() =>
									setSidebarVisible(!sidebarVisible)
								}
							>
								<CircleButton
									icon={SidebarSimple}
									onClick={() =>
										setSidebarVisible(!sidebarVisible)
									}
									active={!sidebarVisible}
								/>
							</TopBarItem>
							<TopBarItem
								id="navigation"
								label={t('topBar.navigation')}
								priority="high"
							>
								<CircleButtonGroup>
									<CircleButton
										icon={ArrowLeft}
										onClick={goBack}
										disabled={!canGoBack}
									/>
									<CircleButton
										icon={ArrowRight}
										onClick={goForward}
										disabled={!canGoForward}
									/>
								</CircleButtonGroup>
							</TopBarItem>
							{pathBarPath && (
								<TopBarItem
									id="path-bar"
									label={t('topBar.path')}
									priority="high"
								>
									<PathBar
										path={pathBarPath}
										devices={devices}
										onNavigate={navigateToPath}
									/>
								</TopBarItem>
							)}
							{currentView && (
								<TopBarItem
									id="virtual-path-bar"
									label={t('topBar.path')}
									priority="high"
								>
									<VirtualPathBar
										view={currentView}
										devices={devices}
									/>
								</TopBarItem>
							)}
						</>
					}
					right={
						<>
							<TopBarItem
								id="search"
								label={t('topBar.search')}
								priority="high"
							>
								<ExpandableSearchButton
									placeholder={
										currentPath
											? t('search.searchInFolder')
											: t('search.placeholder')
									}
									value={searchValue}
									onChange={handleSearchChange}
									onClear={handleSearchClear}
								/>
							</TopBarItem>
							<TopBarItem
								id="tag-mode"
								label={t('topBar.tags')}
								priority="low"
								onClick={() => setTagModeActive(!tagModeActive)}
							>
								<CircleButton
									icon={TagIcon}
									onClick={() =>
										setTagModeActive(!tagModeActive)
									}
									active={tagModeActive}
								/>
							</TopBarItem>
							<TopBarItem
								id="wander"
								label={t('topBar.wander')}
								priority="low"
								onClick={() =>
									canWander && setWanderOpen(true)
								}
							>
								<CircleButton
									icon={MagicWand}
									onClick={() =>
										canWander && setWanderOpen(true)
									}
									disabled={!canWander}
								/>
							</TopBarItem>
							<TopBarItem
								id="view-mode"
								label={t('topBar.views')}
								priority="normal"
								submenuContent={viewModeSubmenu}
							>
								<ViewModeMenu
									viewMode={viewMode}
									onViewModeChange={handleViewModeChange}
									organizeAvailable={organizeAvailable}
								/>
							</TopBarItem>
							<TopBarItem
								id="view-settings"
								label={t('topBar.viewSettings')}
								priority="low"
								submenuContent={viewSettingsSubmenu}
							>
								<ViewSettings
									totalFileCount={currentFiles.length}
								/>
							</TopBarItem>
							<TopBarItem
								id="sort"
								label={t('topBar.sort')}
								priority="low"
								submenuContent={sortSubmenu}
							>
								<SortMenu
									sortBy={sortBy}
									onSortChange={setSortBy}
									viewMode={viewMode}
								/>
							</TopBarItem>
							<TopBarItem
								id="split-pane"
								label={t('topBar.split')}
								priority="low"
								onClick={handleSplit}
							>
								<CircleButton
									icon={Columns}
									onClick={handleSplit}
								/>
							</TopBarItem>
							<TopBarItem
								id="transcode"
								label={t('topBar.transcode')}
								priority="low"
								onClick={() =>
									canTranscode &&
									openTranscodeDialog(selectedVideos)
								}
							>
								<CircleButton
									icon={FilmStrip}
									onClick={() =>
										canTranscode &&
										openTranscodeDialog(selectedVideos)
									}
									disabled={!canTranscode}
								/>
							</TopBarItem>
							<TopBarItem
								id="inspector-toggle"
								label={t('topBar.inspector')}
								priority="high"
								onClick={() =>
									setInspectorVisible(!inspectorVisible)
								}
							>
								<CircleButton
									icon={Info}
									onClick={() =>
										setInspectorVisible(!inspectorVisible)
									}
									active={!inspectorVisible}
								/>
							</TopBarItem>
						</>
					}
				/>
			)}

			<PaneLayout
				panes={panes}
				sizes={sizes}
				focusedId={focusedId}
				onFocus={focusPane}
				onClose={closePane}
				onResize={resize}
				primary={<ExplorerPaneBody />}
			/>

			{wanderOpen && (
				<WanderOverlay
					files={wanderFiles}
					startIndex={wanderStartIndex}
					hasNextPage={wanderHasNextPage}
					fetchNextPage={wanderFetchNextPage}
					onClose={() => setWanderOpen(false)}
				/>
			)}
		</>
	);
}
