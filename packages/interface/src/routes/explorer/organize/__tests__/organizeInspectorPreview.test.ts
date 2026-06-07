import type {File} from '@sd/ts-client';
import {describe, expect, test} from 'bun:test';
import {
	deriveDirectoryPreviewAvailability,
	deriveOrganizeInspectorPreview
} from '../organizePreview';

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

describe('deriveOrganizeInspectorPreview', () => {
	test('keeps organize directory preview tabs first with disabled states and default tab', () => {
		const files = [
			makeFile({id: 'video-1', content_kind: 'video'}),
			makeFile({
				id: 'dir-1',
				name: 'Album',
				kind: 'Directory',
				extension: '',
				content_kind: 'unknown',
				sd_path: {
					Physical: {device_slug: 'dev-1', path: '/photos/Album'}
				}
			})
		];
		const directory = files[1]!;
		const availability = deriveDirectoryPreviewAvailability([files[0]!]);
		const preview = deriveOrganizeInspectorPreview({
			selectedFile: directory,
			directoryAvailability: availability
		});

		expect(preview.defaultTabId).toBe('video');
		expect(preview.tabs.map((tab) => tab.id)).toEqual([
			'video',
			'image',
			'list'
		]);
		expect(preview.tabs.map((tab) => tab.disabled)).toEqual([
			false,
			true,
			false
		]);
		expect(preview.tabs.map((tab) => tab.disabledReason)).toEqual([
			null,
			'missing-image',
			null
		]);
	});

	test('shows only image preview tab for a single image file', () => {
		const preview = deriveOrganizeInspectorPreview({
			selectedFile: makeFile({
				id: 'image-1',
				name: 'photo.jpg',
				extension: 'jpg',
				content_kind: 'image',
				sd_path: {
					Physical: {device_slug: 'dev-1', path: '/photos/photo.jpg'}
				}
			}),
			directoryAvailability: null
		});

		expect(preview.defaultTabId).toBe('image');
		expect(preview.tabs.map((tab) => tab.id)).toEqual(['image']);
	});

	test('shows only video preview tab for a single video file', () => {
		const preview = deriveOrganizeInspectorPreview({
			selectedFile: makeFile({id: 'video-1', content_kind: 'video'}),
			directoryAvailability: null
		});

		expect(preview.defaultTabId).toBe('video');
		expect(preview.tabs.map((tab) => tab.id)).toEqual(['video']);
	});

	test('returns no organize preview tabs for non-media files', () => {
		const preview = deriveOrganizeInspectorPreview({
			selectedFile: makeFile({
				id: 'text-1',
				name: 'notes.txt',
				extension: 'txt',
				content_kind: 'document',
				sd_path: {
					Physical: {device_slug: 'dev-1', path: '/photos/notes.txt'}
				}
			}),
			directoryAvailability: null
		});

		expect(preview.defaultTabId).toBeNull();
		expect(preview.tabs).toEqual([]);
	});
});
