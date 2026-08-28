import {describe, expect, test} from 'bun:test';
import {buildMoveDestinationRows, mapLocationsToMoveDestinations, physicalDestination} from '../decision';

describe('organize move destinations', () => {
	test('orders recent, locations, physical pinned paths, then browse', () => {
		const rows = buildMoveDestinationRows({
			recent: [{destination: physicalDestination('dev', 'C:/Recent'), updated_at: '2026-06-01'}],
			locations: [{id: 'loc', name: 'Photos', sd_path: physicalDestination('dev', 'C:/Photos')}],
			pinned: [{id: 'pin', name: 'Archive', sdPath: physicalDestination('dev', 'D:/Archive')}, {id: 'cloud', name: 'Ignored', sdPath: {Cloud: {service: 's3', identifier: 'bucket', path: 'recent'}}}],
		});
		expect(rows.map((row) => row.kind)).toEqual(['recent', 'location', 'pinned', 'browse']);
	});

	test('limits recent destinations to five distinct physical paths', () => {
		const rows = buildMoveDestinationRows({
			recent: Array.from({length: 7}, (_, index) => ({destination: physicalDestination('dev', `C:/Recent/${index}`), updated_at: `2026-06-0${index + 1}`})),
			locations: [],
			pinned: [],
		});
		expect(rows.filter((row) => row.kind === 'recent')).toHaveLength(5);
	});

	test('maps current-library location records without inventing destinations', () => {
		expect(mapLocationsToMoveDestinations([
			{id: 'loc', name: 'Photos', sd_path: physicalDestination('dev', 'C:/Photos')},
		])).toEqual([
			{id: 'loc', name: 'Photos', sd_path: physicalDestination('dev', 'C:/Photos')},
		]);
	});
});
