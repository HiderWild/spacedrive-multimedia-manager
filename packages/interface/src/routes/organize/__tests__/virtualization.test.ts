import {describe, expect, test} from 'bun:test';
import {createThumbnailCacheKey, gridColumnCount, virtualRowCount, virtualRowRange} from '../virtualization';

describe('organize virtualization', () => {
	test('keeps a large grid bounded to rows plus overscan', () => {
		expect(gridColumnCount(1200, 180, 16)).toBe(6);
		expect(virtualRowCount(10_000, 6)).toBe(1667);
		expect(virtualRowRange(9000 * 220, 900, 220, 1667, 2).end - virtualRowRange(9000 * 220, 900, 220, 1667, 2).start).toBeLessThanOrEqual(8);
	});

	test('thumbnail identity includes normalized path, size, timestamp, and kind', () => {
		expect(createThumbnailCacheKey('C:\\Photos\\A.JPG\\', 10, '2026-08-28T00:00:00Z', false)).toBe('file:c:/photos/a.jpg@10@2026-08-28T00:00:00Z');
		expect(createThumbnailCacheKey('C:/Photos/A.JPG', 10, '2026-08-28T00:00:00Z', true)).toBe('dir:c:/photos/a.jpg@10@2026-08-28T00:00:00Z');
	});
});
