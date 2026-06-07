import type {DirectorySortBy, File, MediaSortBy} from '@sd/ts-client';
import type {OrganizePreviewTab} from './organizeTypes';

export interface OrganizeInspectorPreviewContext {
	sortBy: DirectorySortBy | MediaSortBy;
	foldersFirst: boolean;
}

/** Inline getContentKind to avoid runtime @sd/ts-client resolution in bun tests. */
function fileContentKind(file: File): string {
	return file.content_identity?.kind ?? file.content_kind ?? 'unknown';
}

/**
 * Coerce a DirectorySortBy | MediaSortBy into a MediaSortBy.
 * "type" is not valid for media queries — map it to "name".
 */
export function toMediaSortBy(
	sortBy: DirectorySortBy | MediaSortBy
): MediaSortBy {
	switch (sortBy) {
		case 'created':
		case 'datetaken':
		case 'modified':
		case 'name':
		case 'size':
			return sortBy;
		case 'type':
			return 'name';
	}
}

/**
 * Coerce a DirectorySortBy | MediaSortBy into a DirectorySortBy.
 * "created" and "datetaken" are media-only — map them to "modified".
 */
export function toPreviewListSortBy(
	sortBy: DirectorySortBy | MediaSortBy
): DirectorySortBy {
	switch (sortBy) {
		case 'name':
		case 'modified':
		case 'size':
		case 'type':
			return sortBy;
		case 'created':
		case 'datetaken':
			return 'modified';
	}
}

export interface DirectoryPreviewAvailability {
	renderedTabs: OrganizePreviewTab[];
	enabledTabs: OrganizePreviewTab[];
	defaultTab: OrganizePreviewTab;
	firstVideo: File | null;
	firstImage: File | null;
}

export interface OrganizeInspectorPreviewTabDescriptor {
	id: OrganizePreviewTab;
	disabled: boolean;
	disabledReason: 'missing-video' | 'missing-image' | null;
}

export interface OrganizeInspectorPreviewState {
	tabs: OrganizeInspectorPreviewTabDescriptor[];
	defaultTabId: OrganizePreviewTab | null;
}

/**
 * Given the recursive media entries of a selected directory, derive which
 * preview tabs should be rendered, which are enabled, and the default tab.
 */
export function deriveDirectoryPreviewAvailability(
	files: File[]
): DirectoryPreviewAvailability {
	const firstVideo =
		files.find((file) => fileContentKind(file) === 'video') ?? null;
	const firstImage =
		files.find((file) => fileContentKind(file) === 'image') ?? null;

	if (!firstVideo && !firstImage) {
		return {
			renderedTabs: ['list'],
			enabledTabs: ['list'],
			defaultTab: 'list',
			firstVideo: null,
			firstImage: null
		};
	}

	const renderedTabs: OrganizePreviewTab[] = ['video', 'image', 'list'];
	const enabledTabs: OrganizePreviewTab[] = [
		...(firstVideo ? (['video'] as const) : []),
		...(firstImage ? (['image'] as const) : []),
		...(['list'] as const)
	];

	return {
		renderedTabs,
		enabledTabs,
		defaultTab: firstVideo ? 'video' : 'image',
		firstVideo,
		firstImage
	};
}

export function deriveOrganizeInspectorPreview(args: {
	selectedFile: File | null;
	directoryAvailability: DirectoryPreviewAvailability | null;
}): OrganizeInspectorPreviewState {
	const {selectedFile, directoryAvailability} = args;
	if (!selectedFile) {
		return {tabs: [], defaultTabId: null};
	}

	if (selectedFile.kind === 'Directory') {
		const availability = directoryAvailability ?? {
			renderedTabs: ['list'] as OrganizePreviewTab[],
			enabledTabs: ['list'] as OrganizePreviewTab[],
			defaultTab: 'list' as OrganizePreviewTab,
			firstVideo: null,
			firstImage: null
		};

		return {
			defaultTabId: availability.defaultTab,
			tabs: availability.renderedTabs.map((id) => {
				const disabled = !availability.enabledTabs.includes(id);

				return {
					id,
					disabled,
					disabledReason: disabled
						? id === 'video'
							? 'missing-video'
							: id === 'image'
								? 'missing-image'
								: null
						: null
				};
			})
		};
	}

	const kind = fileContentKind(selectedFile);
	if (kind === 'video') {
		return {
			defaultTabId: 'video',
			tabs: [{id: 'video', disabled: false, disabledReason: null}]
		};
	}

	if (kind === 'image') {
		return {
			defaultTabId: 'image',
			tabs: [{id: 'image', disabled: false, disabledReason: null}]
		};
	}

	return {tabs: [], defaultTabId: null};
}
