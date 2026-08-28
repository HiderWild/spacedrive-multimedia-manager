import {ArrowLeft, CaretRight, FolderOpen} from '@phosphor-icons/react';
import {useMemo, useRef, useState} from 'react';
import {useNavigate, useParams} from 'react-router-dom';
import type {File, Model, SdPath} from '@sd/ts-client';
import {useLibraryQuery, useLibraryMutation} from '../../contexts/SpacedriveContext';
import {OrganizeDecisionBar} from './OrganizeDecisionBar';
import {OrganizeGrid} from './OrganizeGrid';
import {OrganizePreviewPane} from './OrganizePreviewPane';
import {OrganizeMovePicker} from './OrganizeMovePicker';
import {physicalDestination, type LocationMoveDestination, type PinnedMoveDestination, type RecentMoveDestination} from './decision/moveDestinations';
import {OrganizeProgress} from './OrganizeProgress';
import {useOrganizeTask} from './useOrganizeTask';
import {createSelectionState, reduceSelection, type OrganizeSelectionEvent} from './selection';
import type {OrganizeSelectionState} from './decision/contracts';
import {buildSetDecisionInput} from './decision/contracts';
import {OrganizeChangesPanel} from './OrganizeChangesPanel';
import {OrganizeCommitDialog, buildCommitReview} from './OrganizeCommitDialog';
import {OrganizeLifecycleDialogs} from './OrganizeLifecycleDialogs';

export function OrganizeTaskPage() {
	const {taskId = ''} = useParams();
	const navigate = useNavigate();
	const [parentItemId, setParentItemId] = useState<string | null>(null);
	const {task, taskSummary, children, isLoading, error, refetch} = useOrganizeTask(taskId, parentItemId);
	const [selection, setSelection] = useState<OrganizeSelectionState>(createSelectionState());
	const [scrollTop, setScrollTop] = useState(0);
	const [focusedItemId, setFocusedItemId] = useState<string | null>(null);
	const [movePickerOpen, setMovePickerOpen] = useState(false);
	const [movePath, setMovePath] = useState('');
	const [commitDialogOpen, setCommitDialogOpen] = useState(false);
	const scrollRef = useRef<HTMLElement | null>(null);
	const setDecision = useLibraryMutation('organize.set_decision');
	const commit = useLibraryMutation('organize.commit');
	const scanChanges = useLibraryMutation('organize.scan_changes');
	const retrySnapshot = useLibraryMutation('organize.retry_snapshot');
	const finish = useLibraryMutation('organize.finish');
	const reopen = useLibraryMutation('organize.reopen');
	const deleteTaskRecord = useLibraryMutation('organize.delete_task');
	const commitPlan = useLibraryQuery({type: 'organize.commit_plan', input: {task_id: taskId, expected_revision: taskSummary?.revision ?? 0}}, {enabled: Boolean(taskSummary)});
	const items = children?.items ?? [];
	const projections = useMemo(() => new Map((children?.decision_projections ?? []).map((projection) => [projection.item_id, projection])), [children]);
	const focusedItem = items.find((item) => item.uuid === focusedItemId) ?? null;
	const focusedEntryUuid = focusedItem?.entry_uuid ?? '';
	const focusedFileQuery = useLibraryQuery(
		{type: 'files.by_id', input: {file_id: focusedEntryUuid}},
		{enabled: focusedEntryUuid.length > 0},
	);

	if (isLoading) return <div className="flex h-full items-center justify-center text-sm text-ink-dull">Loading organize task…</div>;
	if (error || !taskSummary || !task) return <div className="flex h-full flex-col items-center justify-center gap-3 text-sm text-ink-dull"><p>Could not load this organize task.</p><button type="button" onClick={() => navigate('/organize')} className="rounded bg-app-box px-3 py-1.5 text-ink">Back to tasks</button></div>;

	const orderedIds = items.map((item) => item.uuid);
	const selectedItemIds = selection.kind === 'items'
		? selection.itemIds
		: new Set(items.filter((item) => !selection.excludedItemIds.has(item.uuid)).map((item) => item.uuid));
	const select = (event: OrganizeSelectionEvent) => {
		setSelection((current) => reduceSelection(current, event));
		if ('itemId' in event && event.itemId) setFocusedItemId(event.itemId);
	};
	const selectLasso = (itemIds: Set<string>) => {
		setSelection((current) => ({
			kind: 'items',
			itemIds,
			focusId: current.focusId,
			anchorId: current.anchorId,
		}));
	};
	const selectedFile = (focusedFileQuery.data as File | undefined) ?? null;
	const moveDeviceSlug = 'Physical' in taskSummary.root_sd_path ? taskSummary.root_sd_path.Physical.device_slug : 'local';
	const recentDestinations: RecentMoveDestination[] = [];
	const locationDestinations: LocationMoveDestination[] = [];
	const pinnedDestinations: PinnedMoveDestination[] = [];
	const applyMove = async (destination: SdPath) => {
		const result = await setDecision.mutateAsync(buildSetDecisionInput(taskId, taskSummary.revision, selection, {Move: {destination}}));
		if ('StaleRevision' in result) {
			setSelection(createSelectionState());
			await refetch();
			return;
		}
		setMovePickerOpen(false);
		await refetch();
	};
	const refreshAfter = async (run: () => Promise<unknown>) => { await run(); await refetch(); };

	return (
		<main className="flex h-full min-h-0 flex-col text-ink">
			<header className="flex shrink-0 items-center gap-3 border-b border-app-line px-4 py-3">
				<button type="button" onClick={() => navigate('/organize')} className="rounded p-1 text-ink-dull hover:bg-app-hover" aria-label="Back to organize tasks"><ArrowLeft size={18} /></button>
				<div className="min-w-0 flex-1"><h1 className="truncate text-lg font-semibold">{taskSummary.name}</h1><p className="truncate text-xs text-ink-faint">{taskSummary.root_path}</p></div>
				<div className="hidden min-w-[12rem] md:block"><OrganizeProgress progress={taskSummary.progress} /></div>
			</header>
			<OrganizeChangesPanel plan={commitPlan.data} />
			<div className="flex flex-wrap items-center justify-between gap-2 border-b border-app-line px-4 py-2"><OrganizeLifecycleDialogs task={taskSummary} onScan={() => void refreshAfter(() => scanChanges.mutateAsync({task_id: taskId, expected_revision: taskSummary.revision}))} onRetrySnapshot={() => void refreshAfter(() => retrySnapshot.mutateAsync({task_id: taskId, expected_revision: taskSummary.revision}))} onFinish={() => { if (taskSummary.progress.unmarked_units > 0 && !window.confirm(`Finish with ${taskSummary.progress.unmarked_units} unmarked units?`)) return; void refreshAfter(() => finish.mutateAsync({task_id: taskId, expected_revision: taskSummary.revision, confirm_unmarked: taskSummary.progress.unmarked_units > 0})); }} onReopen={() => void refreshAfter(() => reopen.mutateAsync({task_id: taskId, expected_revision: taskSummary.revision}))} onDelete={() => { if (window.confirm('Delete this task record? Files will not be deleted.')) void refreshAfter(() => deleteTaskRecord.mutateAsync({task_id: taskId, expected_revision: taskSummary.revision})); }} /><button type="button" disabled={!commitPlan.data} onClick={() => setCommitDialogOpen(true)} className="rounded bg-accent px-3 py-1.5 text-xs text-white disabled:opacity-50">Review commit</button></div>
			<OrganizeCommitDialog plan={commitPlan.data} open={commitDialogOpen} taskId={taskId} onCancel={() => setCommitDialogOpen(false)} onConfirm={(input) => { setCommitDialogOpen(false); void commit.mutateAsync(input).then(() => refetch()); }} />
			<OrganizeDecisionBar task={taskSummary} selection={selection} progress={taskSummary.progress} onStale={() => { setSelection(createSelectionState()); void refetch(); }} onApplied={() => void refetch()} onChooseMove={() => setMovePickerOpen(true)} />
			{movePickerOpen && <div className="border-b border-app-line bg-app-box/50 p-3"><div className="mb-2 flex items-center justify-between text-sm font-medium"><span>Move selected items to…</span><button type="button" onClick={() => setMovePickerOpen(false)} className="text-xs text-ink-faint hover:text-ink">Cancel</button></div><OrganizeMovePicker recent={recentDestinations} locations={locationDestinations} pinned={pinnedDestinations} task={taskSummary} selection={selection} onStale={() => { setSelection(createSelectionState()); void refetch(); }} onApplied={() => { setMovePickerOpen(false); void refetch(); }} onBrowse={() => undefined} /><div className="mt-2 flex gap-2"><input value={movePath} onChange={(event) => setMovePath(event.target.value)} placeholder="C:\\Sorted\\Keep" className="min-w-0 flex-1 rounded border border-app-line bg-app-box px-2 py-1.5 text-sm" /><button type="button" disabled={!movePath.trim() || setDecision.isPending} onClick={() => void applyMove(physicalDestination(moveDeviceSlug, movePath))} className="rounded bg-accent px-3 py-1.5 text-sm text-white disabled:opacity-50">Set destination</button></div></div>}
			<div className="flex min-h-0 flex-1">
				<section ref={scrollRef} className="min-w-0 flex-1 overflow-auto p-4" tabIndex={0} onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)} onKeyDown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a') { event.preventDefault(); select({type: 'selectAll', parentItemId: parentItemId ?? task.root_item_id, filter: 'All'}); } }}>
					<div className="mb-3 flex items-center gap-2 text-xs text-ink-faint"><FolderOpen size={15} /><span>{items.length} direct children</span>{parentItemId && <><CaretRight size={13} /><button type="button" onClick={() => { setParentItemId(null); select({type: 'directoryChanged'}); }} className="hover:text-ink">Back to task root</button></>}<span className="ml-auto">Recursive progress is included above</span></div>
					<OrganizeGrid items={items.map((item) => ({item, projection: projections.get(item.uuid)}))} width={900} viewportHeight={700} scrollTop={scrollTop} selectedItemIds={selectedItemIds} onLassoSelectionChange={selectLasso} scrollContainerRef={scrollRef} renderItem={({item, projection}) => <button type="button" data-selected={selection.kind === 'items' && selection.itemIds.has(item.uuid)} onClick={(event) => select({type: event.shiftKey ? 'shiftClick' : event.ctrlKey || event.metaKey ? 'ctrlClick' : 'plainClick', itemId: item.uuid, orderedIds})} onDoubleClick={() => { if (item.kind === 'directory') { setParentItemId(item.uuid); setFocusedItemId(item.uuid); select({type: 'directoryChanged'}); } }} className="flex w-full flex-col items-start gap-1 rounded-lg border border-app-line bg-app-box/30 p-3 text-left hover:border-accent/60 data-[selected=true]:border-accent"><span className="truncate text-sm font-medium">{item.name || taskSummary.name}</span><span className="text-xs text-ink-faint">{item.kind} · {item.unit_count ?? 0} units</span>{projection?.effective_decision && <span className="text-xs text-accent">{projection.effective_decision}</span>}</button>} />
				</section>
				<aside className="hidden w-[min(34vw,26rem)] shrink-0 border-l border-app-line lg:block"><OrganizePreviewPane taskId={taskId} selectedFile={selectedFile} siblingFiles={[]} /></aside>
			</div>
		</main>
	);
}
