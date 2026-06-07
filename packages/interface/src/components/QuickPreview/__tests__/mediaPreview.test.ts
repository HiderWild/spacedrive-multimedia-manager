import {describe, expect, test} from 'bun:test';
import type {File} from '@sd/ts-client';
import {buildPosterUrl} from '../mediaPreview';

function makeFile(overrides: Partial<File> = {}): File {
	return {
		id: 'video-1',
		name: 'clip.mp4',
		kind: 'File',
		extension: 'mp4',
		sd_path: {
			Physical: {
				device_slug: 'device-1',
				path: '/clips/clip.mp4'
			}
		},
		size: 1024,
		content_identity: {uuid: 'content-1', kind: 'video'} as File['content_identity'],
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

describe('buildPosterUrl', () => {
	test('prefers the largest thumbnail sidecar as the video poster', () => {
		const file = makeFile({
			sidecars: [
				{
					kind: 'thumb',
					variant: 'grid@1x',
					format: 'webp'
				},
				{
					kind: 'thumb',
					variant: 'detail@2x',
					format: 'webp'
				}
			] as File['sidecars']
		});

		expect(
			buildPosterUrl(
				file,
				(contentUuid, kind, variant, format) =>
					`${contentUuid}/${kind}/${variant}.${format}`
			)
		).toBe('content-1/thumb/detail@2x.webp');
	});

	test('returns null when the file has no thumbnail sidecars', () => {
		expect(
			buildPosterUrl(
				makeFile(),
				(contentUuid, kind, variant, format) =>
					`${contentUuid}/${kind}/${variant}.${format}`
			)
		).toBeNull();
	});
});
