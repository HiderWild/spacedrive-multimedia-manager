import {ArrowLeft, CaretRight, FolderOpen} from '@phosphor-icons/react';
import {useSidebarStore} from '@sd/ts-client';
import type {
	File,
	LocationsListOutput,
	LocationsListQueryInput,
	Model,
	OrganizeItemFilter,
	OrganizeItemSort,
	OrganizeSortDirection,
	SdPath,
	SpaceLayout
} from '@sd/ts-client';
import {useEffect, useMemo, useRef, useState} from 'react';
import {useNavigate, useParams} from 'react-router-dom';
import {useSpaceLayout} from '../../components/SpacesSidebar/hooks/useSpaces';
import {useTabManager} from '../../components/TabManager';
import {usePlatform} from '../../contexts/PlatformContext';
import {
	useLibraryMutation,
	useLibraryQuery,
	useNormalizedQuery
} from '../../contexts/SpacedriveContext';
import {File as FileComponent} from '../explorer/File';
import type {OrganizeSelectionState} from './decision/contracts';
import {buildSetDecisionInput} from './decision/contracts';
import {
	mapLocationsToMoveDestinations,
	physicalDestination
} from './decision/moveDestinations';
import type {RecentMoveDestination} from './decision/moveDestinations';
import {OrganizeChangesPanel} from './OrganizeChangesPanel';
import {OrganizeCommitDialog} from './OrganizeCommitDialog';
import {OrganizeDecisionBar} from './OrganizeDecisionBar';
import {OrganizeGrid} from './OrganizeGrid';
import {OrganizeLifecycleDialogs} from './OrganizeLifecycleDialogs';
import {OrganizeMovePicker} from './OrganizeMovePicker';
import {OrganizePreviewPane} from './OrganizePreviewPane';
import {OrganizeProgress} from './OrganizeProgress';
import {
	createSelectionState,
	reduceSelection,
	type OrganizeSelectionEvent
} from './selection';
import {useOrganizeTask} from './useOrganizeTask';

const ORGANIZE_FILTERS: readonly OrganizeItemFilter[] = [
	'All',
	'Unmarked',
	'Keep',
	'Discard',
	'Move',
	'Failed',
	'Changed',
	'Missing'
];
const ORGANIZE_SORTS: readonly OrganizeItemSort[] = [
	'Name',
	'Modified',
	'Size',
	'Progress'
];
const ORGANIZE_DIRECTIONS: readonly OrganizeSortDirection[] = ['Asc', 'Desc'];

function chooseValue<T extends string>(
	values: readonly T[],
	value: string
): T | undefined {
	return values.find((candidate) => candidate === value);
}

export function OrganizeTaskPage() {
	const {taskId = ''} = useParams();
	const navigate = useNavigate();
	const {activeTabId, getOrganizeState, updateOrganizeState} =
		useTabManager();
	const {currentSpaceId} = useSidebarStore();
	const platform = usePlatform();
	const organizeStateKey = `${activeTabId}:${taskId}`;
	const restoredOrganizeState = useMemo(
		() => getOrganizeState(activeTabId, taskId),
		[activeTabId, getOrganizeState, taskId]
	);
	const [parentItemId, setParentItemId] = useState<string | null>(
		restoredOrganizeState.currentItemId
	);
	const [organizeFilter, setOrganizeFilter] = useState<OrganizeItemFilter>(
		restoredOrganizeState.filter
	);
	const [organizeSort, setOrganizeSort] = useState<OrganizeItemSort>(
		restoredOrganizeState.sort
	);
	const [organizeDirection, setOrganizeDirection] =
		useState<OrganizeSortDirection>(restoredOrganizeState.direction);
	const [organizeViewMode, setOrganizeViewMode] = useState<'grid' | 'list'>(
		restoredOrganizeState.viewMode
	);
	const {
		task,
		taskSummary,
		children,
		hasNextPage,
		fetchNextPage,
		isLoading,
		error,
		refetch
	} = useOrganizeTask(taskId, parentItemId, {
		filter: organizeFilter,
		sort: organizeSort,
		direction: organizeDirection
	});
	const [selection, setSelection] = useState<OrganizeSelectionState>(
		createSelectionState()
	);
	const [scrollTop, setScrollTop] = useState(restoredOrganizeState.scrollTop);
	const [focusedItemId, setFocusedItemId] = useState<string | null>(null);
	const [movePickerOpen, setMovePickerOpen] = useState(false);
	const [movePath, setMovePath] = useState('');
	const [commitDialogOpen, setCommitDialogOpen] = useState(false);
	const scrollRef = useRef<HTMLElement | null>(null);
	const restoredStateKeyRef = useRef<string | null>(null);
	const setDecision = useLibraryMutation('organize.set_decision');
	const commit = useLibraryMutation('organize.commit');
	const scanChanges = useLibraryMutation('organize.scan_changes');
	const retrySnapshot = useLibraryMutation('organize.retry_snapshot');
	const finish = useLibraryMutation('organize.finish');
	const reopen = useLibraryMutation('organize.reopen');
	const deleteTaskRecord = useLibraryMutation('organize.delete_task');
	const commitPlan = useLibraryQuery(
		{
			type: 'organize.commit_plan',
			input: {
				task_id: taskId,
				expected_revision: taskSummary?.revision ?? 0
			}
		},
		{enabled: Boolean(taskSummary)}
	);
	const locationsQuery = useNormalizedQuery<
		LocationsListQueryInput,
		LocationsListOutput
	>({query: 'locations.list', input: null, resourceType: 'location'});
	const spaceLayoutQuery = useSpaceLayout(currentSpaceId);
	const changeItemsQuery = useLibraryQuery(
		{
			type: 'organize.children',
			input: {
				task_id: taskId,
				parent_item_id: task?.root_item_id ?? '',
				cursor: null,
				limit: 200,
				sort: 'Name',
				direction: 'Asc',
				filter: 'All'
			}
		},
		{enabled: Boolean(task?.root_item_id)}
	);
	const items = children?.items ?? [];
	const projections = useMemo(
		() =>
			new Map(
				(children?.decision_projections ?? []).map((projection) => [
					projection.item_id,
					projection
				])
			),
		[children]
	);
	const focusedItem =
		items.find((item) => item.uuid === focusedItemId) ?? null;
	const focusedEntryUuid = focusedItem?.entry_uuid ?? '';
	const focusedFileQuery = useLibraryQuery(
		{type: 'files.by_id', input: {file_id: focusedEntryUuid}},
		{enabled: focusedEntryUuid.length > 0}
	);
	const pinnedDestinations = useMemo(() => {
		const layout = spaceLayoutQuery.data as SpaceLayout | undefined;
		const items = [
			...(layout?.space_items ?? []),
			...(layout?.groups ?? []).flatMap((group) => group.items)
		];
		return items.flatMap((item) => {
			if (
				typeof item.item_type !== 'object' ||
				item.item_type === null ||
				!('Path' in item.item_type)
			)
				return [];
			return [
				{
					id: item.id,
					name: item.resolved_file?.name ?? 'Pinned path',
					sdPath: item.item_type.Path.sd_path
				}
			];
		});
	}, [spaceLayoutQuery.data]);

	useEffect(() => {
		if (!taskId || restoredStateKeyRef.current === organizeStateKey) return;
		restoredStateKeyRef.current = organizeStateKey;
		const saved = getOrganizeState(activeTabId, taskId);
		setParentItemId(saved.currentItemId);
		setOrganizeFilter(saved.filter);
		setOrganizeSort(saved.sort);
		setOrganizeDirection(saved.direction);
		setOrganizeViewMode(saved.viewMode);
		setScrollTop(saved.scrollTop);
		if (scrollRef.current) scrollRef.current.scrollTop = saved.scrollTop;
		setFocusedItemId(null);
		setSelection(createSelectionState());
	}, [activeTabId, getOrganizeState, organizeStateKey, taskId]);

	useEffect(() => {
		if (!taskId) return;
		updateOrganizeState(activeTabId, taskId, {
			currentItemId: parentItemId,
			viewMode: organizeViewMode,
			filter: organizeFilter,
			sort: organizeSort,
			direction: organizeDirection,
			scrollTop
		});
	}, [
		activeTabId,
		organizeDirection,
		organizeFilter,
		organizeSort,
		organizeViewMode,
		parentItemId,
		scrollTop,
		taskId,
		updateOrganizeState
	]);

	if (isLoading)
		return (
			<div className="text-ink-dull flex h-full items-center justify-center text-sm">
				Loading organize task…
			</div>
		);
	if (error || !taskSummary || !task)
		return (
			<div className="text-ink-dull flex h-full flex-col items-center justify-center gap-3 text-sm">
				<p>Could not load this organize task.</p>
				<button
					type="button"
					onClick={() => navigate('/organize')}
					className="bg-app-box text-ink rounded px-3 py-1.5"
				>
					Back to tasks
				</button>
			</div>
		);

	const orderedIds = items.map((item) => item.uuid);
	const selectedItemIds =
		selection.kind === 'items'
			? selection.itemIds
			: new Set(
					items
						.filter(
							(item) => !selection.excludedItemIds.has(item.uuid)
						)
						.map((item) => item.uuid)
				);
	const select = (event: OrganizeSelectionEvent) => {
		setSelection((current) => reduceSelection(current, event));
		if ('itemId' in event && event.itemId) setFocusedItemId(event.itemId);
	};
	const selectLasso = (itemIds: Set<string>) => {
		setSelection((current) => ({
			kind: 'items',
			itemIds,
			focusId: current.focusId,
			anchorId: current.anchorId
		}));
	};
	const selectedFile = focusedFileQuery.data ?? null;
	const moveDeviceSlug =
		'Physical' in taskSummary.root_sd_path
			? taskSummary.root_sd_path.Physical.device_slug
			: 'local';
	const locationDestinations = mapLocationsToMoveDestinations(
		locationsQuery.data?.locations ?? []
	);
	const changeItems = changeItemsQuery.data?.items ?? [];
	const changeProjections = useMemo(
		() =>
			new Map(
				(changeItemsQuery.data?.decision_projections ?? []).map(
					(projection) => [projection.item_id, projection]
				)
			),
		[changeItemsQuery.data]
	);
	const recentDestinations = useMemo<RecentMoveDestination[]>(
		() =>
			changeItems.flatMap((item) => {
				const destination = changeProjections.get(
					item.uuid
				)?.move_destination;
				return destination
					? [{destination, updated_at: item.updated_at}]
					: [];
			}),
		[changeItems, changeProjections]
	);
	const applyMove = async (destination: SdPath) => {
		const result = await setDecision.mutateAsync(
			buildSetDecisionInput(taskId, taskSummary.revision, selection, {
				Move: {destination}
			})
		);
		if ('StaleRevision' in result) {
			setSelection(createSelectionState());
			await refetch();
			return;
		}
		setMovePickerOpen(false);
		await refetch();
	};
	const refreshAfter = async (run: () => Promise<unknown>) => {
		await run();
		await refetch();
	};
	const handleBrowse = async () => {
		if (!platform.openDirectoryPickerDialog) return;
		const selectedPath = await platform.openDirectoryPickerDialog({
			title: 'Choose move destination',
			multiple: false
		});
		if (typeof selectedPath === 'string') setMovePath(selectedPath);
		else if (Array.isArray(selectedPath) && selectedPath.length > 0)
			setMovePath(selectedPath[0]);
	};
	const resetScroll = () => {
		setScrollTop(0);
		if (scrollRef.current) scrollRef.current.scrollTop = 0;
	};
	const changeFilter = (value: string) => {
		const next = chooseValue(ORGANIZE_FILTERS, value);
		if (next) {
			setOrganizeFilter(next);
			setSelection(createSelectionState());
			setFocusedItemId(null);
			resetScroll();
		}
	};
	const changeSort = (value: string) => {
		const next = chooseValue(ORGANIZE_SORTS, value);
		if (next) {
			setOrganizeSort(next);
			setSelection(createSelectionState());
			setFocusedItemId(null);
			resetScroll();
		}
	};
	const changeDirection = (value: string) => {
		const next = chooseValue(ORGANIZE_DIRECTIONS, value);
		if (next) {
			setOrganizeDirection(next);
			setSelection(createSelectionState());
			setFocusedItemId(null);
			resetScroll();
		}
	};
	const changeViewMode = (value: 'grid' | 'list') => {
		setOrganizeViewMode(value);
		resetScroll();
	};

	return (
		<main className="text-ink flex h-full min-h-0 flex-col">
			<header className="border-app-line flex shrink-0 items-center gap-3 border-b px-4 py-3">
				<button
					type="button"
					onClick={() => navigate('/organize')}
					className="text-ink-dull hover:bg-app-hover rounded p-1"
					aria-label="Back to organize tasks"
				>
					<ArrowLeft size={18} />
				</button>
				<div className="min-w-0 flex-1">
					<h1 className="truncate text-lg font-semibold">
						{taskSummary.name}
					</h1>
					<p className="text-ink-faint truncate text-xs">
						{taskSummary.root_path}
					</p>
				</div>
				<div className="hidden min-w-[12rem] md:block">
					<OrganizeProgress progress={taskSummary.progress} />
				</div>
			</header>
			<OrganizeChangesPanel
				plan={commitPlan.data}
				task={taskSummary}
				items={changeItems}
				onStale={() => {
					setSelection(createSelectionState());
					void refetch();
				}}
				onApplied={() => void refetch()}
			/>
			<div className="border-app-line text-ink-dull flex flex-wrap items-center gap-3 border-b px-4 py-2 text-xs">
				<label className="flex items-center gap-1.5">
					Show
					<select
						value={organizeFilter}
						onChange={(event) =>
							changeFilter(event.currentTarget.value)
						}
						className="border-app-line bg-app-box text-ink rounded border px-2 py-1"
					>
						<option value="All">All</option>
						<option value="Unmarked">Unmarked</option>
						<option value="Keep">Keep</option>
						<option value="Discard">Discard</option>
						<option value="Move">Move</option>
						<option value="Failed">Failed</option>
						<option value="Changed">Changed</option>
						<option value="Missing">Missing</option>
					</select>
				</label>
				<label className="flex items-center gap-1.5">
					Sort
					<select
						value={organizeSort}
						onChange={(event) =>
							changeSort(event.currentTarget.value)
						}
						className="border-app-line bg-app-box text-ink rounded border px-2 py-1"
					>
						<option value="Name">Name</option>
						<option value="Modified">Modified</option>
						<option value="Size">Size</option>
						<option value="Progress">Progress</option>
					</select>
				</label>
				<label className="flex items-center gap-1.5">
					Order
					<select
						value={organizeDirection}
						onChange={(event) =>
							changeDirection(event.currentTarget.value)
						}
						className="border-app-line bg-app-box text-ink rounded border px-2 py-1"
					>
						<option value="Asc">Ascending</option>
						<option value="Desc">Descending</option>
					</select>
				</label>
				<span
					className="flex items-center gap-1"
					role="group"
					aria-label="Organize layout"
				>
					<button
						type="button"
						aria-pressed={organizeViewMode === 'grid'}
						onClick={() => changeViewMode('grid')}
						className="border-app-line text-ink rounded border px-2 py-1"
					>
						Grid
					</button>
					<button
						type="button"
						aria-pressed={organizeViewMode === 'list'}
						onClick={() => changeViewMode('list')}
						className="border-app-line text-ink rounded border px-2 py-1"
					>
						List
					</button>
				</span>
			</div>
			<div className="border-app-line flex flex-wrap items-center justify-between gap-2 border-b px-4 py-2">
				<OrganizeLifecycleDialogs
					task={taskSummary}
					onScan={() =>
						void refreshAfter(() =>
							scanChanges.mutateAsync({
								task_id: taskId,
								expected_revision: taskSummary.revision
							})
						)
					}
					onRetrySnapshot={() =>
						void refreshAfter(() =>
							retrySnapshot.mutateAsync({
								task_id: taskId,
								expected_revision: taskSummary.revision
							})
						)
					}
					onFinish={() => {
						if (
							taskSummary.progress.unmarked_units > 0 &&
							!window.confirm(
								`Finish with ${taskSummary.progress.unmarked_units} unmarked units?`
							)
						)
							return;
						void refreshAfter(() =>
							finish.mutateAsync({
								task_id: taskId,
								expected_revision: taskSummary.revision,
								confirm_unmarked:
									taskSummary.progress.unmarked_units > 0
							})
						);
					}}
					onReopen={() =>
						void refreshAfter(() =>
							reopen.mutateAsync({
								task_id: taskId,
								expected_revision: taskSummary.revision
							})
						)
					}
					onDelete={() => {
						if (
							window.confirm(
								'Delete this task record? Files will not be deleted.'
							)
						)
							void refreshAfter(() =>
								deleteTaskRecord.mutateAsync({
									task_id: taskId,
									expected_revision: taskSummary.revision
								})
							);
					}}
				/>
				<button
					type="button"
					disabled={
						!commitPlan.data || taskSummary.status !== 'active'
					}
					onClick={() => setCommitDialogOpen(true)}
					className="bg-accent rounded px-3 py-1.5 text-xs text-white disabled:opacity-50"
				>
					Review commit
				</button>
			</div>
			<OrganizeCommitDialog
				plan={commitPlan.data}
				open={commitDialogOpen}
				taskId={taskId}
				onCancel={() => setCommitDialogOpen(false)}
				onConfirm={(input) => {
					setCommitDialogOpen(false);
					void commit.mutateAsync(input).then(() => refetch());
				}}
			/>
			<OrganizeDecisionBar
				task={taskSummary}
				selection={selection}
				progress={taskSummary.progress}
				onStale={() => {
					setSelection(createSelectionState());
					void refetch();
				}}
				onApplied={() => void refetch()}
				onChooseMove={() => setMovePickerOpen(true)}
			/>
			{movePickerOpen && (
				<div className="border-app-line bg-app-box/50 border-b p-3">
					<div className="mb-2 flex items-center justify-between text-sm font-medium">
						<span>Move selected items to…</span>
						<button
							type="button"
							onClick={() => setMovePickerOpen(false)}
							className="text-ink-faint hover:text-ink text-xs"
						>
							Cancel
						</button>
					</div>
					<OrganizeMovePicker
						locations={locationDestinations}
						recent={recentDestinations}
						pinned={pinnedDestinations}
						task={taskSummary}
						selection={selection}
						onStale={() => {
							setSelection(createSelectionState());
							void refetch();
						}}
						onApplied={() => {
							setMovePickerOpen(false);
							void refetch();
						}}
						onBrowse={() => void handleBrowse()}
						browseAvailable={Boolean(
							platform.openDirectoryPickerDialog
						)}
					/>
					<div className="mt-2 flex gap-2">
						<input
							value={movePath}
							onChange={(event) =>
								setMovePath(event.target.value)
							}
							placeholder="C:\\Sorted\\Keep"
							className="border-app-line bg-app-box min-w-0 flex-1 rounded border px-2 py-1.5 text-sm"
						/>
						<button
							type="button"
							disabled={!movePath.trim() || setDecision.isPending}
							onClick={() =>
								void applyMove(
									physicalDestination(
										moveDeviceSlug,
										movePath
									)
								)
							}
							className="bg-accent rounded px-3 py-1.5 text-sm text-white disabled:opacity-50"
						>
							Set destination
						</button>
					</div>
					{!platform.openDirectoryPickerDialog && (
						<p className="text-ink-faint mt-2 text-xs">
							Native folder browsing is unavailable on this
							platform.
						</p>
					)}
				</div>
			)}
			<div className="flex min-h-0 flex-1">
				<section
					ref={scrollRef}
					className="min-w-0 flex-1 overflow-auto p-4"
					tabIndex={0}
					onScroll={(event) =>
						setScrollTop(event.currentTarget.scrollTop)
					}
					onKeyDown={(event) => {
						if (
							(event.ctrlKey || event.metaKey) &&
							event.key.toLowerCase() === 'a'
						) {
							event.preventDefault();
							select({
								type: 'selectAll',
								parentItemId: parentItemId ?? task.root_item_id,
								filter: organizeFilter
							});
						}
					}}
				>
					<div className="text-ink-faint mb-3 flex items-center gap-2 text-xs">
						<FolderOpen size={15} />
						<span>{items.length} direct children</span>
						{parentItemId && (
							<>
								<CaretRight size={13} />
								<button
									type="button"
									onClick={() => {
										setParentItemId(null);
										setScrollTop(0);
										select({type: 'directoryChanged'});
									}}
									className="hover:text-ink"
								>
									Back to task root
								</button>
							</>
						)}
						<span className="ml-auto">
							Recursive progress is included above
						</span>
					</div>
					<OrganizeGrid
						items={items.map((item) => ({
							item,
							projection: projections.get(item.uuid)
						}))}
						width={900}
						viewportHeight={700}
						scrollTop={scrollTop}
						minimumCardWidth={
							organizeViewMode === 'list' ? 1000 : 180
						}
						rowHeight={organizeViewMode === 'list' ? 96 : 220}
						selectedItemIds={selectedItemIds}
						onLassoSelectionChange={selectLasso}
						onEndReached={() => {
							if (hasNextPage) fetchNextPage();
						}}
						scrollContainerRef={scrollRef}
						renderItem={({item, projection}) => (
							<button
								type="button"
								data-selected={selectedItemIds.has(item.uuid)}
								onClick={(event) =>
									select({
										type: event.shiftKey
											? 'shiftClick'
											: event.ctrlKey || event.metaKey
												? 'ctrlClick'
												: 'plainClick',
										itemId: item.uuid,
										orderedIds
									})
								}
								onDoubleClick={() => {
									if (item.kind === 'directory') {
										setParentItemId(item.uuid);
										setScrollTop(0);
										setFocusedItemId(item.uuid);
										select({type: 'directoryChanged'});
									}
								}}
								className={`border-app-line bg-app-box/30 hover:border-accent/60 data-[selected=true]:border-accent flex w-full gap-3 rounded-lg border p-3 text-left ${organizeViewMode === 'list' ? 'flex-row items-center' : 'flex-col items-start'}`}
							>
								<OrganizeItemThumbnail
									item={item}
									list={organizeViewMode === 'list'}
								/>
								<span className="truncate text-sm font-medium">
									{item.name || taskSummary.name}
								</span>
								<span className="text-ink-faint text-xs">
									{item.kind} · {item.unit_count ?? 0} units
								</span>
								{projection?.effective_decision && (
									<span className="text-accent text-xs">
										{projection.effective_decision}
									</span>
								)}
							</button>
						)}
					/>
				</section>
				<aside className="border-app-line hidden w-[min(34vw,26rem)] shrink-0 border-l lg:block">
					<OrganizePreviewPane
						taskId={taskId}
						selectedItemId={focusedItem?.uuid ?? null}
						selectedFile={selectedFile}
						siblingFiles={[]}
					/>
				</aside>
			</div>
		</main>
	);
}

function OrganizeItemThumbnail({item, list}: {item: Model; list: boolean}) {
	const entryUuid = item.entry_uuid ?? '';
	const fileQuery = useLibraryQuery(
		{type: 'files.by_id', input: {file_id: entryUuid}},
		{enabled: entryUuid.length > 0, staleTime: 60_000}
	);
	const file = fileQuery.data as File | undefined;
	if (!file) {
		return (
			<div
				className={`${list ? 'h-12 w-12' : 'h-32 w-full'} bg-app-hover text-ink-faint flex shrink-0 items-center justify-center rounded-md text-xs`}
			>
				{item.kind}
			</div>
		);
	}
	return (
		<FileComponent.Thumb
			file={file}
			size={list ? 48 : 128}
			squareMode={!list}
			frameClassName="rounded-md border border-app-line/50 bg-app-box/30"
		/>
	);
}
