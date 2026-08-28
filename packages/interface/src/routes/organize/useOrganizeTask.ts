import {useQueryClient} from '@tanstack/react-query';
import type {OrganizeChildrenOutput, OrganizeGetOutput, OrganizeTaskSummary} from '@sd/ts-client';
import {useLibraryMutation, useLibraryQuery} from '../../contexts/SpacedriveContext';

export function useOrganizeTask(taskId: string, parentItemId: string | null = null) {
	const queryClient = useQueryClient();
	const task = useLibraryQuery({type: 'organize.get', input: {task_id: taskId}}, {enabled: taskId.length > 0});
	const rootItemId = (task.data as OrganizeGetOutput | undefined)?.root_item_id ?? '';
	const children = useLibraryQuery(
		{
			type: 'organize.children',
			input: {
				task_id: taskId,
				parent_item_id: parentItemId ?? rootItemId,
				cursor: null,
				limit: 200,
				sort: 'Name',
				direction: 'Asc',
				filter: 'All',
			},
		},
		{enabled: rootItemId.length > 0 && (parentItemId === null || parentItemId.length > 0)},
	);

	const refetch = async () => {
		await queryClient.invalidateQueries({queryKey: ['organize.get']});
		await queryClient.invalidateQueries({queryKey: ['organize.children']});
	};

	return {
		task: task.data as OrganizeGetOutput | undefined,
		taskSummary: (task.data as OrganizeGetOutput | undefined)?.task as OrganizeTaskSummary | undefined,
		children: children.data as OrganizeChildrenOutput | undefined,
		isLoading: task.isLoading || children.isLoading,
		error: task.error ?? children.error,
		refetch,
	};
}

export function useOrganizeTaskActions() {
	return {
		create: useLibraryMutation('organize.create'),
		scanChanges: useLibraryMutation('organize.scan_changes'),
		retrySnapshot: useLibraryMutation('organize.retry_snapshot'),
		finish: useLibraryMutation('organize.finish'),
		reopen: useLibraryMutation('organize.reopen'),
		deleteTaskRecord: useLibraryMutation('organize.delete_task'),
		commit: useLibraryMutation('organize.commit'),
	};
}
