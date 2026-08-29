import type {
	OrganizeChildrenOutput,
	OrganizeGetOutput,
	OrganizeItemFilter,
	OrganizeItemSort,
	OrganizeSortDirection,
	OrganizeTaskSummary
} from '@sd/ts-client';
import {useSpacedriveClient} from '@sd/ts-client/hooks';
import {useInfiniteQuery, useQueryClient} from '@tanstack/react-query';
import {
	useLibraryMutation,
	useLibraryQuery
} from '../../contexts/SpacedriveContext';

export interface OrganizeTaskViewOptions {
	filter: OrganizeItemFilter;
	sort: OrganizeItemSort;
	direction: OrganizeSortDirection;
}

const DEFAULT_VIEW_OPTIONS: OrganizeTaskViewOptions = {
	filter: 'All',
	sort: 'Name',
	direction: 'Asc'
};

export function useOrganizeTask(
	taskId: string,
	parentItemId: string | null = null,
	viewOptions: OrganizeTaskViewOptions = DEFAULT_VIEW_OPTIONS
) {
	const queryClient = useQueryClient();
	const client = useSpacedriveClient();
	const libraryId = client.getCurrentLibraryId();
	const task = useLibraryQuery(
		{type: 'organize.get', input: {task_id: taskId}},
		{enabled: taskId.length > 0}
	);
	const rootItemId =
		(task.data as OrganizeGetOutput | undefined)?.root_item_id ?? '';
	const children = useInfiniteQuery<OrganizeChildrenOutput, Error>({
		queryKey: [
			'organize.children',
			libraryId,
			taskId,
			parentItemId ?? rootItemId,
			viewOptions
		],
		enabled:
			Boolean(libraryId) &&
			rootItemId.length > 0 &&
			(parentItemId === null || parentItemId.length > 0),
		initialPageParam: null as string | null,
		queryFn: ({pageParam}) =>
			client.execute('query:organize.children', {
				task_id: taskId,
				parent_item_id: parentItemId ?? rootItemId,
				cursor: pageParam,
				limit: 200,
				sort: viewOptions.sort,
				direction: viewOptions.direction,
				filter: viewOptions.filter
			}) as Promise<OrganizeChildrenOutput>,
		getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined
	});
	const flattenedChildren: OrganizeChildrenOutput | undefined = children.data
		?.pages.length
		? {
				...children.data.pages[0],
				items: children.data.pages.flatMap((page) => page.items),
				decision_projections: children.data.pages.flatMap(
					(page) => page.decision_projections
				),
				breadcrumb: children.data.pages[0].breadcrumb,
				next_cursor:
					children.data.pages[children.data.pages.length - 1]
						.next_cursor
			}
		: undefined;

	const refetch = async () => {
		await queryClient.invalidateQueries({queryKey: ['organize.get']});
		await queryClient.invalidateQueries({queryKey: ['organize.children']});
	};

	return {
		task: task.data as OrganizeGetOutput | undefined,
		taskSummary: (task.data as OrganizeGetOutput | undefined)?.task as
			| OrganizeTaskSummary
			| undefined,
		children: flattenedChildren,
		hasNextPage: children.hasNextPage,
		isFetchingNextPage: children.isFetchingNextPage,
		fetchNextPage: () => {
			if (children.hasNextPage && !children.isFetchingNextPage)
				void children.fetchNextPage();
		},
		isLoading: task.isLoading || children.isLoading,
		error: task.error ?? children.error,
		refetch
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
		commit: useLibraryMutation('organize.commit')
	};
}
