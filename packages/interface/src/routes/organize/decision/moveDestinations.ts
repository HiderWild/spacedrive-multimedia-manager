import type {SdPath} from '@sd/ts-client';

export interface RecentMoveDestination {
	destination: SdPath;
	updated_at: string;
}

export interface LocationMoveDestination {
	id: string;
	name: string;
	sd_path: SdPath;
}

export interface PinnedMoveDestination {
	id: string;
	name: string;
	sdPath: SdPath;
}

export type MoveDestinationRow =
	| {kind: 'recent'; key: string; name: string; destination: SdPath}
	| {kind: 'location'; key: string; name: string; destination: SdPath}
	| {kind: 'pinned'; key: string; name: string; destination: SdPath}
	| {kind: 'browse'; key: 'browse'; name: string};

function isPhysical(path: SdPath): path is Extract<SdPath, {Physical: unknown}> {
	return 'Physical' in path;
}

function pathKey(path: SdPath): string {
	return JSON.stringify(path);
}

export function buildMoveDestinationRows(input: {
	recent: RecentMoveDestination[];
	locations: LocationMoveDestination[];
	pinned: PinnedMoveDestination[];
}): MoveDestinationRow[] {
	const rows: MoveDestinationRow[] = [];
	const seen = new Set<string>();

	for (const recent of [...input.recent].sort((a, b) => b.updated_at.localeCompare(a.updated_at))) {
		const key = pathKey(recent.destination);
		if (!isPhysical(recent.destination) || seen.has(key) || rows.filter((row) => row.kind === 'recent').length >= 5) continue;
		seen.add(key);
		rows.push({kind: 'recent', key: `recent:${key}`, name: recent.destination.Physical.path, destination: recent.destination});
	}

	for (const location of input.locations) {
		if (!isPhysical(location.sd_path)) continue;
		const key = pathKey(location.sd_path);
		if (seen.has(key)) continue;
		seen.add(key);
		rows.push({kind: 'location', key: `location:${location.id}`, name: location.name, destination: location.sd_path});
	}

	for (const pinned of input.pinned) {
		if (!isPhysical(pinned.sdPath)) continue;
		const key = pathKey(pinned.sdPath);
		if (seen.has(key)) continue;
		seen.add(key);
		rows.push({kind: 'pinned', key: `pinned:${pinned.id}`, name: pinned.name, destination: pinned.sdPath});
	}

	rows.push({kind: 'browse', key: 'browse', name: 'Browse…'});
	return rows;
}

export function physicalDestination(deviceSlug: string, path: string): SdPath {
	return {Physical: {device_slug: deviceSlug, path}};
}
