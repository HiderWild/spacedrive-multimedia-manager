import type { File, SdPath } from "@sd/ts-client";
import { describe, expect, test } from "bun:test";
import {
	findAdjacentPreviewFile,
	getPreviewSequenceKeyTarget,
	getFullPreviewFileId,
	previewSequenceInput,
	previewSequenceLabel,
} from "../OrganizePreviewPane";

function makeFile(id: string, name = id): File {
	return {
		id,
		name,
		kind: "File",
		extension: "jpg",
		sd_path: {
			Physical: { device_slug: "device", path: `C:/photos/${name}` },
		},
		size: 1,
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
		content_kind: "image",
		is_local: true,
		duration_seconds: null,
	};
}

describe("organize preview sequence", () => {
	test("moves through bounded candidates and stops at both edges", () => {
		const files = [makeFile("a"), makeFile("b"), makeFile("c")];
		expect(findAdjacentPreviewFile(files, "b", -1)?.id).toBe("a");
		expect(findAdjacentPreviewFile(files, "b", 1)?.id).toBe("c");
		expect(findAdjacentPreviewFile(files, "a", -1)).toBeNull();
		expect(findAdjacentPreviewFile(files, "c", 1)).toBeNull();
	});

	test("uses focused arrow keys to move only within the bounded sample set", () => {
		const files = [makeFile("a"), makeFile("b"), makeFile("c")];

		expect(getPreviewSequenceKeyTarget(files, "b", "ArrowLeft")).toBe("a");
		expect(getPreviewSequenceKeyTarget(files, "b", "ArrowRight")).toBe("c");
		expect(getPreviewSequenceKeyTarget(files, "a", "ArrowLeft")).toBe("a");
		expect(getPreviewSequenceKeyTarget(files, "c", "ArrowRight")).toBe("c");
		expect(getPreviewSequenceKeyTarget(files, "b", "Enter")).toBeNull();
	});

	test("opens the currently selected sample through the full-preview file contract", () => {
		const sample = makeFile("sample");

		expect(getFullPreviewFileId(sample)).toBe("sample");
		expect(getFullPreviewFileId(null)).toBeNull();
	});

	test("builds the task manifest query input without live recursive options", () => {
		const directory = {
			Physical: { device_slug: "device", path: "C:/photos" },
		} satisfies SdPath;
		expect(previewSequenceInput(directory, "task-1", "item-1")).toEqual({
			directory,
			organize: { task_id: "task-1", item_id: "item-1" },
			limit: 12,
		});
	});

	test("labels sampled results and preserves the selected position", () => {
		const files = [makeFile("a"), makeFile("b")];
		expect(previewSequenceLabel(files, "b", false)).toBe("2 / 2");
		expect(previewSequenceLabel(files, "b", true)).toBe("2 / 2 · sampled");
		expect(previewSequenceLabel([], null, false)).toBe("No media");
	});
});
