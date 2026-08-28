import {CheckSquare, Plus} from '@phosphor-icons/react';
import type {OrganizeListOutput} from '@sd/ts-client';
import {useNavigate} from 'react-router-dom';
import {useLibraryQuery} from '../../contexts/SpacedriveContext';

const ACTIVE_TASK_STATUSES = [
	'scanning',
	'active',
	'committing',
	'failed'
] as const;

export function OrganizeTasksGroup() {
	const navigate = useNavigate();
	const query = useLibraryQuery({
		type: 'organize.list',
		input: {statuses: [...ACTIVE_TASK_STATUSES], cursor: null, limit: 5}
	});
	const tasks = (query.data as OrganizeListOutput | undefined)?.tasks ?? [];

	return (
		<section className="space-y-1" aria-label="Organize tasks">
			<div className="text-ink-faint flex items-center justify-between px-2 text-[11px] font-semibold uppercase tracking-wide">
				<span>Organize</span>
				<button
					type="button"
					onClick={() => navigate('/organize')}
					className="hover:bg-app-hover rounded p-0.5"
					aria-label="Create organize task"
				>
					<Plus size={13} />
				</button>
			</div>
			<button
				type="button"
				onClick={() => navigate('/organize')}
				className="text-ink-dull hover:bg-app-hover hover:text-ink flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm"
			>
				<CheckSquare size={16} />{' '}
				<span className="flex-1">All tasks</span>
			</button>
			{tasks.map((task) => (
				<button
					type="button"
					key={task.id}
					onClick={() => navigate(`/organize/${task.id}`)}
					className="text-ink-faint hover:bg-app-hover hover:text-ink block w-full truncate rounded-md px-2 py-1 text-left text-xs"
					title={task.root_path}
				>
					{task.name}
				</button>
			))}
		</section>
	);
}
