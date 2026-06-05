import { describe, expect, test } from "bun:test";
import type { File } from "@sd/ts-client";
import { deriveDirectoryPreviewAvailability, toMediaSortBy, toPreviewListSortBy } from "../organizePreview";

function makeFile(overrides: Partial<File> = {}): File {
	return {
		id: "file-1",
		name: "clip.mp4",
		kind: "File",
		extension: "mp4",
		sd_path: { Physical: { device_slug: "dev-1", path: "/photos/clip.mp4" } },
		size: 1024,
		content_identity: null,
		alternate_paths: [],
		tags: [],
		sidecars: [],
		image_media_data: null,
		video_media_data: null,
		audio_media_data: null,
		created_at: "2026-01-01T00:00:00Z",
		modified_at: "2026-01-01T00:00:00Z",
		accessed_at: null,
		content_kind: "video",
		is_local: true,
		duration_seconds: null,
		...overrides,
	} as File;
}

describe("organize preview helpers", () => {
	test("toMediaSortBy coerces unsupported sorts safely", () => {
		expect(toMediaSortBy("type")).toBe("name");
		expect(toMediaSortBy("modified")).toBe("modified");
		expect(toMediaSortBy("name")).toBe("name");
		expect(toMediaSortBy("size")).toBe("size");
		expect(toMediaSortBy("created")).toBe("created");
		expect(toMediaSortBy("datetaken")).toBe("datetaken");
	});

	test("toPreviewListSortBy coerces unsupported directory preview sorts safely", () => {
		expect(toPreviewListSortBy("datetaken")).toBe("modified");
		expect(toPreviewListSortBy("created")).toBe("modified");
		expect(toPreviewListSortBy("name")).toBe("name");
		expect(toPreviewListSortBy("modified")).toBe("modified");
		expect(toPreviewListSortBy("size")).toBe("size");
		expect(toPreviewListSortBy("type")).toBe("type");
	});

	test("deriveDirectoryPreviewAvailability disables missing media tabs and falls back to list when nothing exists", () => {
		expect(deriveDirectoryPreviewAvailability([])).toEqual({
			renderedTabs: ["list"],
			enabledTabs: ["list"],
			defaultTab: "list",
			firstVideo: null,
			firstImage: null,
		});
	});

	test("deriveDirectoryPreviewAvailability enables video tab when video files exist", () => {
		const videoFile = makeFile({ id: "vid-1", content_kind: "video", name: "clip.mp4" });
		const result = deriveDirectoryPreviewAvailability([videoFile]);
		expect(result.renderedTabs).toEqual(["video", "image", "list"]);
		expect(result.enabledTabs).toEqual(["video", "list"]);
		expect(result.defaultTab).toBe("video");
		expect(result.firstVideo).toBe(videoFile);
		expect(result.firstImage).toBeNull();
	});

	test("deriveDirectoryPreviewAvailability enables image tab when image files exist", () => {
		const imageFile = makeFile({ id: "img-1", content_kind: "image", name: "photo.jpg", extension: "jpg" });
		const result = deriveDirectoryPreviewAvailability([imageFile]);
		expect(result.renderedTabs).toEqual(["video", "image", "list"]);
		expect(result.enabledTabs).toEqual(["image", "list"]);
		expect(result.defaultTab).toBe("image");
		expect(result.firstVideo).toBeNull();
		expect(result.firstImage).toBe(imageFile);
	});

	test("deriveDirectoryPreviewAvailability enables both media tabs and defaults to video", () => {
		const imageFile = makeFile({ id: "img-1", content_kind: "image", name: "photo.jpg", extension: "jpg" });
		const videoFile = makeFile({ id: "vid-1", content_kind: "video", name: "clip.mp4" });
		const result = deriveDirectoryPreviewAvailability([imageFile, videoFile]);
		expect(result.renderedTabs).toEqual(["video", "image", "list"]);
		expect(result.enabledTabs).toEqual(["video", "image", "list"]);
		expect(result.defaultTab).toBe("video");
		expect(result.firstVideo).toBe(videoFile);
		expect(result.firstImage).toBe(imageFile);
	});
});
