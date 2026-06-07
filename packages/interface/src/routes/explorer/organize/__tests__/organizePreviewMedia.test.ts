import type {File} from '@sd/ts-client';
import {describe, expect, test} from 'bun:test';
import {
	filterPreviewCandidates,
	findAdjacentPreviewFile,
	getPreviewMediaKind
} from '../organizePreviewMedia';

function makeFile(overrides: Partial<File> = {}): File {
	return {
		id: 'file-1',
		name: 'clip.mp4',
		kind: 'File',
		extension: 'mp4',
		sd_path: {Physical: {device_slug: 'dev-1', path: '/photos/clip.mp4'}},
		size: 1024,
		content_identity: null,
		alternate_paths: [],
		tags: [],
		sidecars: [],
		image_media_data: null,
		video_media_data: null,
		audio_media_data: null,
		created_at: '2026-01-01T00:00:00Z',
		modified_at: '2026-01-01T00:00:00Z',
		accessed_at: null,
		content_kind: 'video',
		is_local: true,
		duration_seconds: null,
		...overrides
	} as File;
}

describe('organize preview media helpers', () => {
	test('maps preview tabs to media kinds', () => {
		expect(getPreviewMediaKind('image')).toBe('image');
		expect(getPreviewMediaKind('video')).toBe('video');
		expect(getPreviewMediaKind('list')).toBeNull();
	});

	test('filters preview candidates down to file entries of the requested kind', () => {
		const files = [
			makeFile({id: 'video-1', content_kind: 'video'}),
			makeFile({
				id: 'dir-1',
				kind: 'Directory',
				content_kind: 'unknown',
				extension: null,
				sd_path: {
					Physical: {device_slug: 'dev-1', path: '/photos/folder'}
				}
			}),
			makeFile({
				id: 'image-1',
				name: 'photo.jpg',
				extension: 'jpg',
				content_kind: 'image',
				sd_path: {
					Physical: {device_slug: 'dev-1', path: '/photos/photo.jpg'}
				}
			})
		];

		expect(filterPreviewCandidates(files, 'video').map((file) => file.id)).toEqual([
			'video-1'
		]);
		expect(filterPreviewCandidates(files, 'image').map((file) => file.id)).toEqual([
			'image-1'
		]);
	});

	test('finds adjacent preview candidates and returns null at the bounds', () => {
		const files = [
			makeFile({id: 'image-1', content_kind: 'image', extension: 'jpg'}),
			makeFile({id: 'image-2', content_kind: 'image', extension: 'jpg'}),
			makeFile({id: 'image-3', content_kind: 'image', extension: 'jpg'})
		];

		expect(
			findAdjacentPreviewFile({
				files,
				currentFileId: 'image-2',
				offset: -1
			})?.id
		).toBe('image-1');
		expect(
			findAdjacentPreviewFile({
				files,
				currentFileId: 'image-2',
				offset: 1
			})?.id
		).toBe('image-3');
		expect(
			findAdjacentPreviewFile({
				files,
				currentFileId: 'image-1',
				offset: -1
			})
		).toBeNull();
		expect(
			findAdjacentPreviewFile({
				files,
				currentFileId: 'image-3',
				offset: 1
			})
		).toBeNull();
	});
});
