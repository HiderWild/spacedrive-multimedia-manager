import {ArrowRight, FolderOpen, Plus} from '@phosphor-icons/react';
import type {
	OrganizeCreateOutcome,
	OrganizeListOutput,
	OrganizeRootAvailability,
	SdPath
} from '@sd/ts-client';
import {useState} from 'react';
import {useLocation, useNavigate} from 'react-router-dom';
import {
	useLibraryMutation,
	useLibraryQuery
} from '../../contexts/SpacedriveContext';

function physicalPath(deviceSlug: string, path: string): SdPath {
	return {Physical: {device_slug: deviceSlug.trim(), path: path.trim()}};
}

export function OrganizeTasksPage() {
	const navigate = useNavigate();
	const {search} = useLocation();
	const initialQuery = new URLSearchParams(search);
	const [deviceSlug, setDeviceSlug] = useState(
		() => initialQuery.get('device') ?? 'local'
	);
	const [rootPath, setRootPath] = useState(
		() => initialQuery.get('path') ?? ''
	);
	const tasks = useLibraryQuery({
		type: 'organize.list',
		input: {statuses: null, cursor: null, limit: 100}
	});
	const rootAvailability = useLibraryQuery(
		{
			type: 'organize.resolve_root',
			input: {root: physicalPath(deviceSlug, rootPath)}
		},
		{enabled: Boolean(deviceSlug.trim() && rootPath.trim())}
	);
	const create = useLibraryMutation('organize.create');
	const taskList =
		(tasks.data as OrganizeListOutput | undefined)?.tasks ?? [];
	const availability = rootAvailability.data as
		| OrganizeRootAvailability
		| undefined;

	const createTask = async () => {
		if (!rootPath.trim() || !deviceSlug.trim()) return;
		if (
			availability &&
			typeof availability === 'object' &&
			'OpenExisting' in availability
		) {
			navigate(`/organize/${availability.OpenExisting.task_id}`);
			return;
		}
		if (
			availability &&
			typeof availability === 'object' &&
			'Unavailable' in availability
		)
			return;
		const outcome = await create.mutateAsync({
			root: physicalPath(deviceSlug, rootPath),
			name: null
		});
		if ('Created' in outcome)
			navigate(`/organize/${outcome.Created.task_id}`);
	};

	return (
		<main className="text-ink flex h-full min-h-0 flex-col gap-5 overflow-auto p-6">
			<header>
				<p className="text-ink-faint text-xs uppercase tracking-wider">
					Organize tasks
				</p>
				<h1 className="mt-1 text-2xl font-semibold">
					Review a fixed folder snapshot
				</h1>
				<p className="text-ink-dull mt-2 max-w-2xl text-sm">
					Each task owns one recursive snapshot. Decisions stay with
					the task until you explicitly commit them.
				</p>
			</header>

			<section className="border-app-line bg-app-box/30 rounded-lg border p-4">
				<div className="mb-3 flex items-center gap-2 text-sm font-medium">
					<Plus size={16} /> New organize task
				</div>
				<div className="grid gap-3 md:grid-cols-[10rem_1fr_auto]">
					<label className="text-ink-dull text-xs">
						Device
						<input
							value={deviceSlug}
							onChange={(event) =>
								setDeviceSlug(event.target.value)
							}
							className="border-app-line bg-app-box mt-1 w-full rounded border px-2 py-1.5 text-sm"
						/>
					</label>
					<label className="text-ink-dull text-xs">
						Windows folder
						<input
							value={rootPath}
							onChange={(event) =>
								setRootPath(event.target.value)
							}
							placeholder="C:\\Photos"
							className="border-app-line bg-app-box mt-1 w-full rounded border px-2 py-1.5 text-sm"
						/>
					</label>
					<button
						type="button"
						disabled={
							create.isPending ||
							rootAvailability.isLoading ||
							!rootPath.trim() ||
							!deviceSlug.trim() ||
							(availability &&
								typeof availability === 'object' &&
								'Unavailable' in availability)
						}
						onClick={() => void createTask()}
						className="bg-accent self-end rounded px-3 py-1.5 text-sm text-white disabled:opacity-50"
					>
						{create.isPending
							? 'Starting…'
							: availability &&
								  typeof availability === 'object' &&
								  'OpenExisting' in availability
								? 'Open existing task'
								: 'Start scan'}
					</button>
				</div>
				{rootAvailability.isLoading && (
					<p className="text-ink-faint mt-2 text-sm">
						Checking this folder…
					</p>
				)}
				{availability &&
					typeof availability === 'object' &&
					'OpenExisting' in availability && (
						<p className="mt-2 text-sm text-amber-300">
							This folder already has an organize task. Open it to
							continue the existing snapshot.
						</p>
					)}
				{availability &&
					typeof availability === 'object' &&
					'Unavailable' in availability && (
						<p className="mt-2 text-sm text-red-300">
							This folder cannot be organized:{' '}
							{formatCreateRejection({
								Rejected: availability.Unavailable
							} as OrganizeCreateOutcome)}
						</p>
					)}
				{create.data && 'Overlap' in create.data && (
					<p className="mt-2 text-sm text-amber-300">
						This folder is already covered by task{' '}
						{create.data.Overlap.existing_task_id}.
					</p>
				)}
				{create.data && 'Rejected' in create.data && (
					<p className="mt-2 text-sm text-red-300">
						The folder could not be added:{' '}
						{formatCreateRejection(create.data)}
					</p>
				)}
			</section>

			<section className="min-h-0">
				<h2 className="mb-2 text-sm font-medium">Existing tasks</h2>
				{tasks.isLoading ? (
					<p className="text-ink-dull text-sm">Loading tasks…</p>
				) : taskList.length === 0 ? (
					<p className="border-app-line text-ink-dull rounded border border-dashed p-6 text-sm">
						No organize tasks yet.
					</p>
				) : (
					<div className="grid gap-2">
						{taskList.map((task) => (
							<button
								type="button"
								key={task.id}
								onClick={() => navigate(`/organize/${task.id}`)}
								className="border-app-line hover:bg-app-hover flex items-center gap-3 rounded-lg border p-3 text-left"
							>
								<FolderOpen
									size={22}
									className="text-accent shrink-0"
								/>
								<span className="min-w-0 flex-1">
									<span className="block truncate text-sm font-medium">
										{task.name}
									</span>
									<span className="text-ink-faint block truncate text-xs">
										{task.root_path} ·{' '}
										{task.progress.processed_units}/
										{task.progress.total_units} marked ·{' '}
										{task.status}
									</span>
								</span>
								<ArrowRight
									size={16}
									className="text-ink-faint"
								/>
							</button>
						))}
					</div>
				)}
			</section>
		</main>
	);
}

function formatCreateRejection(outcome: OrganizeCreateOutcome): string {
	if (!('Rejected' in outcome)) return '';
	const reason = outcome.Rejected.reason;
	if (typeof reason === 'string') return reason;
	if ('RootMissing' in reason)
		return `Folder not found: ${reason.RootMissing.path}`;
	if ('RootNotDirectory' in reason)
		return `Not a folder: ${reason.RootNotDirectory.path}`;
	if ('PermissionDenied' in reason)
		return `Access denied: ${reason.PermissionDenied.path}`;
	return 'Unsupported folder';
}
