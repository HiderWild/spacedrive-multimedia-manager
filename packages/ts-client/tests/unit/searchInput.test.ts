import {describe, expect, test} from 'bun:test';
import {buildSearchFilters} from '../../src/hooks/useSearchFiles';

describe('buildSearchFilters', () => {
	test('fills every backend filter field with explicit null defaults', () => {
		expect(buildSearchFilters()).toEqual({
			file_types: null,
			tags: null,
			date_range: null,
			size_range: null,
			locations: null,
			content_types: null,
			include_hidden: null,
			include_archived: null,
			at_risk: null,
			on_volumes: null,
			not_on_volumes: null,
			min_volume_count: null,
			max_volume_count: null
		});
	});

	test('preserves structured filters, including the new volume fields', () => {
		expect(
			buildSearchFilters({
				file_types: ['mp4'],
				tags: {
					include: ['tag-a'],
					exclude: ['tag-b'],
					include_inherited: true
				},
				date_range: {
					field: 'ModifiedAt',
					start: '2026-06-01T00:00:00.000Z',
					end: '2026-06-02T00:00:00.000Z'
				},
				size_range: {min: 10, max: 20},
				locations: ['location-a'],
				content_types: ['Video'],
				include_hidden: false,
				include_archived: true,
				at_risk: true,
				on_volumes: ['volume-a'],
				not_on_volumes: ['volume-b'],
				min_volume_count: 1,
				max_volume_count: 3
			})
		).toEqual({
			file_types: ['mp4'],
			tags: {
				include: ['tag-a'],
				exclude: ['tag-b'],
				include_inherited: true
			},
			date_range: {
				field: 'ModifiedAt',
				start: '2026-06-01T00:00:00.000Z',
				end: '2026-06-02T00:00:00.000Z'
			},
			size_range: {min: 10, max: 20},
			locations: ['location-a'],
			content_types: ['Video'],
			include_hidden: false,
			include_archived: true,
			at_risk: true,
			on_volumes: ['volume-a'],
			not_on_volumes: ['volume-b'],
			min_volume_count: 1,
			max_volume_count: 3
		});
	});
});
