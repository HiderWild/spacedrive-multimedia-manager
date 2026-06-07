import type {File} from '@sd/ts-client';
import type {OrganizePreviewTab} from './organizeTypes';

export type OrganizePreviewMediaKind = 'image' | 'video';

function fileContentKind(file: File): string {
	return file.content_identity?.kind ?? file.content_kind ?? 'unknown';
}

export function getPreviewMediaKind(
	tab: OrganizePreviewTab
): OrganizePreviewMediaKind | null {
	if (tab === 'image' || tab === 'video') {
		return tab;
	}

	return null;
}

export function filterPreviewCandidates(
	files: File[],
	kind: OrganizePreviewMediaKind
): File[] {
	return files.filter(
		(file) => file.kind === 'File' && fileContentKind(file) === kind
	);
}

export function findAdjacentPreviewFile(args: {
	files: File[];
	currentFileId: string;
	offset: -1 | 1;
}): File | null {
	const index = args.files.findIndex((file) => file.id === args.currentFileId);
	if (index === -1) {
		return null;
	}

	return args.files[index + args.offset] ?? null;
}
