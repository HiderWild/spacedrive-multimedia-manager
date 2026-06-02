import { useCallback, useEffect, useMemo, useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import type { File, FileSearchInput, FileSearchOutput } from "@sd/ts-client";
import { useSpacedriveClient } from "../../../contexts/SpacedriveContext";

/**
 * Default number of files requested per page.
 *
 * Picked to comfortably overfill a virtualized viewport so the first page
 * renders a full screen while keeping each request small relative to the
 * old fixed ~1000-item cap.
 */
export const DEFAULT_PAGE_SIZE = 100;

/**
 * Shared shape returned by the infinite-loading helpers.
 *
 * `files` is the flattened list of every page fetched so far, ready to feed
 * straight into the existing virtualizer. The pagination controls let a view
 * request the next page as the user nears the end of the list.
 */
export interface InfiniteFilesResult {
	files: File[];
	isLoading: boolean;
	isFetchingNextPage: boolean;
	hasNextPage: boolean;
	fetchNextPage: () => void;
	/** Total matching files reported by the backend, when known. */
	total: number;
}

export interface UseInfiniteSearchFilesOptions {
	/** Base search input. Pagination is overridden per page by this hook. */
	input: FileSearchInput | null;
	enabled?: boolean;
	pageSize?: number;
}

const WIRE_SEARCH_FILES = "query:search.files";

/**
 * Infinite-loading wrapper around the `search.files` op.
 *
 * The search op is the only file-listing op that exposes real offset/limit
 * pagination (directory listing supports limit only), so it drives true
 * windowed loading here. Each page requests `pageSize` results at a growing
 * offset; pages are flattened into a single `File[]` so consuming views keep
 * their existing data shape and virtualization untouched.
 *
 * Live WebSocket cache updates are intentionally not wired here: search
 * results are recomputed on demand and the normalized-cache subscription used
 * by directory browsing assumes a single (non-paginated) query key.
 */
export function useInfiniteSearchFiles({
	input,
	enabled = true,
	pageSize = DEFAULT_PAGE_SIZE,
}: UseInfiniteSearchFilesOptions): InfiniteFilesResult {
	const client = useSpacedriveClient();
	const [libraryId, setLibraryId] = useState<string | null>(
		client.getCurrentLibraryId(),
	);

	// Refetch from scratch when the active library changes.
	useEffect(() => {
		const handleLibraryChange = (newLibraryId: string) => {
			setLibraryId(newLibraryId);
		};
		client.on("library-changed", handleLibraryChange);
		return () => {
			client.off("library-changed", handleLibraryChange);
		};
	}, [client]);

	const queryKey = useMemo(
		() => [WIRE_SEARCH_FILES, libraryId, input, pageSize],
		// Stringify input so structurally equal inputs reuse the cache entry.
		// eslint-disable-next-line react-hooks/exhaustive-deps
		[WIRE_SEARCH_FILES, libraryId, JSON.stringify(input), pageSize],
	);

	const query = useInfiniteQuery<FileSearchOutput, Error>({
		queryKey,
		enabled: enabled && !!input && !!libraryId,
		initialPageParam: 0,
		queryFn: async ({ pageParam }) => {
			if (!input) {
				throw new Error("useInfiniteSearchFiles requires a search input");
			}
			const offset = pageParam as number;
			const pageInput: FileSearchInput = {
				...input,
				pagination: { limit: pageSize, offset },
			};
			return client.execute<FileSearchInput, FileSearchOutput>(
				WIRE_SEARCH_FILES,
				pageInput,
			);
		},
		getNextPageParam: (lastPage, _allPages, lastPageParam) => {
			const received = lastPage.files.length;
			if (received === 0) return undefined;

			const nextOffset = (lastPageParam as number) + received;

			// total_found is authoritative when the backend reports it.
			if (lastPage.total_found > 0 && nextOffset >= lastPage.total_found) {
				return undefined;
			}

			// Otherwise a short page means we've reached the end.
			if (received < pageSize) return undefined;

			return nextOffset;
		},
	});

	const files = useMemo(
		() => query.data?.pages.flatMap((page) => page.files) ?? [],
		[query.data],
	);

	const total = query.data?.pages[0]?.total_found ?? files.length;

	const fetchNextPage = useCallback(() => {
		if (query.hasNextPage && !query.isFetchingNextPage) {
			void query.fetchNextPage();
		}
	}, [query]);

	return {
		files,
		isLoading: query.isLoading,
		isFetchingNextPage: query.isFetchingNextPage,
		hasNextPage: query.hasNextPage,
		fetchNextPage,
		total,
	};
}
