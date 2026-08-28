import {FolderOpen, Plus, ArrowRight} from '@phosphor-icons/react';
import {useState} from 'react';
import {useNavigate} from 'react-router-dom';
import type {OrganizeCreateOutcome, OrganizeListOutput, SdPath} from '@sd/ts-client';
import {useLibraryMutation, useLibraryQuery} from '../../contexts/SpacedriveContext';

function physicalPath(deviceSlug: string, path: string): SdPath {
	return {Physical: {device_slug: deviceSlug.trim(), path: path.trim()}};
}

export function OrganizeTasksPage() {
	const navigate = useNavigate();
	const [deviceSlug, setDeviceSlug] = useState('local');
	const [rootPath, setRootPath] = useState('');
	const tasks = useLibraryQuery({type: 'organize.list', input: {statuses: null, cursor: null, limit: 100}});
	const create = useLibraryMutation('organize.create');
	const taskList = (tasks.data as OrganizeListOutput | undefined)?.tasks ?? [];

	const createTask = async () => {
		if (!rootPath.trim() || !deviceSlug.trim()) return;
		const outcome = await create.mutateAsync({root: physicalPath(deviceSlug, rootPath), name: null});
		if ('Created' in outcome) navigate(`/organize/${outcome.Created.task_id}`);
	};

	return (
		<main className="flex h-full min-h-0 flex-col gap-5 overflow-auto p-6 text-ink">
			<header>
				<p className="text-xs uppercase tracking-wider text-ink-faint">Organize tasks</p>
				<h1 className="mt-1 text-2xl font-semibold">Review a fixed folder snapshot</h1>
				<p className="mt-2 max-w-2xl text-sm text-ink-dull">Each task owns one recursive snapshot. Decisions stay with the task until you explicitly commit them.</p>
			</header>

			<section className="rounded-lg border border-app-line bg-app-box/30 p-4">
				<div className="mb-3 flex items-center gap-2 text-sm font-medium"><Plus size={16} /> New organize task</div>
				<div className="grid gap-3 md:grid-cols-[10rem_1fr_auto]">
					<label className="text-xs text-ink-dull">Device<input value={deviceSlug} onChange={(event) => setDeviceSlug(event.target.value)} className="mt-1 w-full rounded border border-app-line bg-app-box px-2 py-1.5 text-sm" /></label>
					<label className="text-xs text-ink-dull">Windows folder<input value={rootPath} onChange={(event) => setRootPath(event.target.value)} placeholder="C:\\Photos" className="mt-1 w-full rounded border border-app-line bg-app-box px-2 py-1.5 text-sm" /></label>
					<button type="button" disabled={create.isPending || !rootPath.trim()} onClick={() => void createTask()} className="self-end rounded bg-accent px-3 py-1.5 text-sm text-white disabled:opacity-50">{create.isPending ? 'Starting…' : 'Start scan'}</button>
				</div>
				{create.data && 'Overlap' in create.data && <p className="mt-2 text-sm text-amber-300">This folder is already covered by task {create.data.Overlap.existing_task_id}.</p>}
				{create.data && 'Rejected' in create.data && <p className="mt-2 text-sm text-red-300">The folder could not be added: {formatCreateRejection(create.data)}</p>}
			</section>

			<section className="min-h-0">
				<h2 className="mb-2 text-sm font-medium">Existing tasks</h2>
				{tasks.isLoading ? <p className="text-sm text-ink-dull">Loading tasks…</p> : taskList.length === 0 ? <p className="rounded border border-dashed border-app-line p-6 text-sm text-ink-dull">No organize tasks yet.</p> : <div className="grid gap-2">{taskList.map((task) => <button type="button" key={task.id} onClick={() => navigate(`/organize/${task.id}`)} className="flex items-center gap-3 rounded-lg border border-app-line p-3 text-left hover:bg-app-hover"><FolderOpen size={22} className="shrink-0 text-accent" /><span className="min-w-0 flex-1"><span className="block truncate text-sm font-medium">{task.name}</span><span className="block truncate text-xs text-ink-faint">{task.root_path} · {task.progress.processed_units}/{task.progress.total_units} marked · {task.status}</span></span><ArrowRight size={16} className="text-ink-faint" /></button>)}</div>}
			</section>
		</main>
	);
}

function formatCreateRejection(outcome: OrganizeCreateOutcome): string {
	if (!('Rejected' in outcome)) return '';
	const reason = outcome.Rejected.reason;
	if (typeof reason === 'string') return reason;
	if ('RootMissing' in reason) return `Folder not found: ${reason.RootMissing.path}`;
	if ('RootNotDirectory' in reason) return `Not a folder: ${reason.RootNotDirectory.path}`;
	if ('PermissionDenied' in reason) return `Access denied: ${reason.PermissionDenied.path}`;
	return 'Unsupported folder';
}
