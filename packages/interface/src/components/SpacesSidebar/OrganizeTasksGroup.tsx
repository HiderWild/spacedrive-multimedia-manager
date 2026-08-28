import {CheckSquare, Plus} from '@phosphor-icons/react';
import {useNavigate} from 'react-router-dom';
import type {OrganizeListOutput} from '@sd/ts-client';
import {useLibraryQuery} from '../../contexts/SpacedriveContext';

export function OrganizeTasksGroup() {
	const navigate = useNavigate();
	const query = useLibraryQuery({type: 'organize.list', input: {statuses: null, cursor: null, limit: 5}});
	const tasks = (query.data as OrganizeListOutput | undefined)?.tasks ?? [];

	return (
		<section className="space-y-1" aria-label="Organize tasks">
			<div className="flex items-center justify-between px-2 text-[11px] font-semibold uppercase tracking-wide text-ink-faint">
				<span>Organize</span>
				<button type="button" onClick={() => navigate('/organize')} className="rounded p-0.5 hover:bg-app-hover" aria-label="Create organize task"><Plus size={13} /></button>
			</div>
			<button type="button" onClick={() => navigate('/organize')} className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-ink-dull hover:bg-app-hover hover:text-ink">
				<CheckSquare size={16} /> <span className="flex-1">Tasks</span><span className="text-xs text-ink-faint">{tasks.length}</span>
			</button>
			{tasks.slice(0, 3).map((task) => <button type="button" key={task.id} onClick={() => navigate(`/organize/${task.id}`)} className="block w-full truncate rounded-md px-2 py-1 text-left text-xs text-ink-faint hover:bg-app-hover hover:text-ink" title={task.root_path}>{task.name}</button>)}
		</section>
	);
}
