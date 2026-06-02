import { useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";

/**
 * Shared hook to refetch all tag-related queries after mutations.
 * Used by FileInspector, useFileContextMenu, TagsGroup, TagSelector.
 */
export function useRefetchTagQueries() {
	const queryClient = useQueryClient();

	return useCallback(() => {
		queryClient.refetchQueries({ queryKey: ["query:files.directory_listing"], exact: false, type: 'all' });
		queryClient.refetchQueries({ queryKey: ["query:files.by_tag"], exact: false, type: 'all' });
		queryClient.refetchQueries({ queryKey: ["query:files.by_id"], exact: false, type: 'all' });
		queryClient.refetchQueries({ queryKey: ["query:tags.search"], exact: false, type: 'all' });
		// `tags.effective` is fetched via useLibraryQuery, whose key is the bare op name.
		queryClient.refetchQueries({ queryKey: ["tags.effective"], exact: false, type: 'all' });
	}, [queryClient]);
}
