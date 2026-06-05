import type { DirectorySortBy, MediaSortBy, File } from "@sd/ts-client";
import type { OrganizePreviewTab } from "./organizeTypes";

/** Inline getContentKind to avoid runtime @sd/ts-client resolution in bun tests. */
function fileContentKind(file: File): string {
	return file.content_identity?.kind ?? file.content_kind ?? "unknown";
}

/**
 * Coerce a DirectorySortBy | MediaSortBy into a MediaSortBy.
 * "type" is not valid for media queries — map it to "name".
 */
export function toMediaSortBy(sortBy: DirectorySortBy | MediaSortBy): MediaSortBy {
	switch (sortBy) {
		case "created":
		case "datetaken":
		case "modified":
		case "name":
		case "size":
			return sortBy;
		case "type":
			return "name";
	}
}

/**
 * Coerce a DirectorySortBy | MediaSortBy into a DirectorySortBy.
 * "created" and "datetaken" are media-only — map them to "modified".
 */
export function toPreviewListSortBy(sortBy: DirectorySortBy | MediaSortBy): DirectorySortBy {
	switch (sortBy) {
		case "name":
		case "modified":
		case "size":
		case "type":
			return sortBy;
		case "created":
		case "datetaken":
			return "modified";
	}
}

export interface DirectoryPreviewAvailability {
	renderedTabs: OrganizePreviewTab[];
	enabledTabs: OrganizePreviewTab[];
	defaultTab: OrganizePreviewTab;
	firstVideo: File | null;
	firstImage: File | null;
}

/**
 * Given the recursive media entries of a selected directory, derive which
 * preview tabs should be rendered, which are enabled, and the default tab.
 */
export function deriveDirectoryPreviewAvailability(files: File[]): DirectoryPreviewAvailability {
	const firstVideo = files.find((file) => fileContentKind(file) === "video") ?? null;
	const firstImage = files.find((file) => fileContentKind(file) === "image") ?? null;

	if (!firstVideo && !firstImage) {
		return {
			renderedTabs: ["list"],
			enabledTabs: ["list"],
			defaultTab: "list",
			firstVideo: null,
			firstImage: null,
		};
	}

	const renderedTabs: OrganizePreviewTab[] = ["video", "image", "list"];
	const enabledTabs: OrganizePreviewTab[] = [
		...(firstVideo ? (["video"] as const) : []),
		...(firstImage ? (["image"] as const) : []),
		...(["list"] as const),
	];

	return {
		renderedTabs,
		enabledTabs,
		defaultTab: firstVideo ? "video" : "image",
		firstVideo,
		firstImage,
	};
}
