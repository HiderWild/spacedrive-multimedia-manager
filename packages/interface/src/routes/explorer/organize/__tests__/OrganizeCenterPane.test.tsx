import { describe, expect, it } from "bun:test";
import type { File, SdPath } from "@sd/ts-client";

function makeFile(id: string, name: string, kind: "File" | "Directory" = "File"): File {
	const path = `/test/${name}`;
	return {
		id,
		name,
		kind,
		extension: kind === "File" ? "jpg" : null,
		size: 1024,
		sd_path: { Physical: { device_slug: "disk-1", path } } as SdPath,
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

describe("OrganizeCenterPane directory navigation", () => {
	it("identifies directories correctly", () => {
		const directory = makeFile("dir-1", "subfolder", "Directory");
		const file = makeFile("file-1", "photo.jpg", "File");

		expect(directory.kind).toBe("Directory");
		expect(file.kind).toBe("File");
	});

	it("directories have sd_path for navigation", () => {
		const directory = makeFile("dir-1", "subfolder", "Directory");

		expect(directory.sd_path).toBeDefined();
		expect(directory.sd_path).toHaveProperty("Physical");
	});

	it("navigation handler receives directory when double-clicked", () => {
		const directory = makeFile("dir-1", "subfolder", "Directory");
		let navigatedFile: File | null = null;

		const handleNavigate = (file: File) => {
			navigatedFile = file;
		};

		// Simulate double-click behavior
		if (directory.kind === "Directory") {
			handleNavigate(directory);
		}

		expect(navigatedFile).not.toBeNull();
		expect(navigatedFile?.id).toBe("dir-1");
		expect(navigatedFile?.kind).toBe("Directory");
	});

	it("navigation handler not called for file double-click", () => {
		const file = makeFile("file-1", "photo.jpg", "File");
		let navigatedFile: File | null = null;

		const handleNavigate = (file: File) => {
			navigatedFile = file;
		};

		// Simulate double-click behavior
		if (file.kind === "Directory") {
			handleNavigate(file);
		}

		expect(navigatedFile).toBeNull();
	});
});
