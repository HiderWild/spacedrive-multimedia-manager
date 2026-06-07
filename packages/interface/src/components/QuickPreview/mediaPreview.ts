import type {File} from '@sd/ts-client';

type Sidecar = NonNullable<File['sidecars']>[number];

function getThumbnailSizeScore(sidecar: Sidecar): number {
	const scaleMatch = sidecar.variant.match(/@(\d+)x/i);
	const scale = scaleMatch ? Number.parseInt(scaleMatch[1] ?? '1', 10) : 1;
	const baseSize = Number.parseInt(
		sidecar.variant.split('x')[0]?.replace(/\D/g, '') || '0',
		10
	);

	return baseSize * scale;
}

export function pickPosterSidecar(file: File): Sidecar | null {
	const thumbnails = file.sidecars?.filter((sidecar) => sidecar.kind === 'thumb');
	if (!thumbnails || thumbnails.length === 0) {
		return null;
	}

	return thumbnails
		.slice()
		.sort((a, b) => getThumbnailSizeScore(b) - getThumbnailSizeScore(a))[0] ?? null;
}

export function buildPosterUrl(
	file: File,
	buildSidecarUrl: (
		contentUuid: string,
		kind: string,
		variant: string,
		format: string
	) => string
): string | null {
	const contentUuid = file.content_identity?.uuid;
	const sidecar = pickPosterSidecar(file);

	if (!contentUuid || !sidecar) {
		return null;
	}

	return buildSidecarUrl(
		contentUuid,
		sidecar.kind,
		sidecar.variant,
		sidecar.format
	);
}
