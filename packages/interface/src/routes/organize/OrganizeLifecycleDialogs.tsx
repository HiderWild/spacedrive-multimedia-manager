import type {OrganizeFinishOutcome, OrganizeTaskSummary} from '@sd/ts-client';

export interface OrganizeTaskCapabilities {decide: boolean; scan: boolean; retrySnapshot: boolean; commit: boolean; finish: boolean; reopen: boolean; deleteRecord: boolean}

export function taskCapabilities(task: OrganizeTaskSummary): OrganizeTaskCapabilities {
	if (task.status === 'committing') return {decide: false, scan: false, retrySnapshot: false, commit: false, finish: false, reopen: false, deleteRecord: false};
	if (task.status === 'completed') return {decide: false, scan: false, retrySnapshot: false, commit: false, finish: false, reopen: true, deleteRecord: true};
	if (task.status === 'scanning') return {decide: false, scan: false, retrySnapshot: false, commit: false, finish: false, reopen: false, deleteRecord: true};
	if (task.status === 'failed') return {decide: false, scan: false, retrySnapshot: true, commit: false, finish: false, reopen: false, deleteRecord: true};
	return {decide: true, scan: true, retrySnapshot: false, commit: true, finish: true, reopen: false, deleteRecord: true};
}

export function handleOrganizeOutcome(outcome: OrganizeFinishOutcome, effects: {refetch: () => void; notify: () => void}) {
	if ('StaleRevision' in outcome) effects.refetch();
}

export function OrganizeLifecycleDialogs({task, onScan, onRetrySnapshot, onFinish, onReopen, onDelete}: {task: OrganizeTaskSummary; onScan: () => void; onRetrySnapshot: () => void; onFinish: () => void; onReopen: () => void; onDelete: () => void}) {
	const capabilities = taskCapabilities(task);
	return <div className="flex flex-wrap gap-2" aria-label="Organize lifecycle">
		{capabilities.scan && <button type="button" onClick={onScan} className="rounded border border-app-line px-2 py-1 text-xs">Scan changes</button>}
		{capabilities.retrySnapshot && <button type="button" onClick={onRetrySnapshot} className="rounded border border-app-line px-2 py-1 text-xs">Retry snapshot</button>}
		{capabilities.finish && <button type="button" onClick={onFinish} className="rounded border border-app-line px-2 py-1 text-xs">Finish</button>}
		{capabilities.reopen && <button type="button" onClick={onReopen} className="rounded border border-app-line px-2 py-1 text-xs">Reopen</button>}
		{capabilities.deleteRecord && <button type="button" onClick={onDelete} className="rounded border border-red-400/50 px-2 py-1 text-xs text-red-300">Delete task record</button>}
	</div>;
}
