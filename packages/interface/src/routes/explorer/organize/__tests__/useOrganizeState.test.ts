import { describe, expect, it } from "bun:test";
import type { File } from "@sd/ts-client";
import { buildOrganizeDirectoryKey, createEmptyOrganizeDirectoryState } from "../organizePersistence";
import { clearOrganizeDecision, upsertOrganizeDecision } from "../organizeState";
import { loadPersistedOrganizeState, persistOrganizeStateChange } from "../useOrganizeState";

function makeFile(id: string, path: string): File {
	return {
		id,
		name: path.split("/").at(-1) ?? id,
		kind: "File",
		extension: "jpg",
		size: 1024,
		sd_path: { Physical: { device_slug: "disk-1", path } },
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

describe("loadPersistedOrganizeState", () => {
	it("clears the persisted-file guard when a directory has no saved state", async () => {
		const result = await loadPersistedOrganizeState({
			directoryPath: "/fresh",
			loadOrganizeState: async () => null,
		});

		expect(result.hasPersistedFile).toBe(false);
		expect(result.state.directoryPath).toBe("/fresh");
		expect(result.state.items).toEqual({});
	});

	it("clears the persisted-file guard when loading organize state fails", async () => {
		const result = await loadPersistedOrganizeState({
			directoryPath: "/fresh",
			loadOrganizeState: async () => {
				throw new Error("disk offline");
			},
		});

		expect(result.hasPersistedFile).toBe(false);
		expect(result.state.directoryPath).toBe("/fresh");
		expect(result.state.items).toEqual({});
	});
});

describe("persistOrganizeStateChange", () => {
	it("deletes persisted organize JSON instead of saving an empty state", async () => {
		const file = makeFile("keep-1", "/photos/keep.jpg");
		let persisted = createEmptyOrganizeDirectoryState("/photos");
		persisted = upsertOrganizeDecision(persisted, file, "keep");
		const cleared = clearOrganizeDecision(persisted, file);
		const saveCalls: Array<{ directoryKey: string; json: string }> = [];
		const deleteCalls: string[] = [];

		const hasPersistedFile = await persistOrganizeStateChange({
			directoryPath: "/photos",
			next: cleared,
			hasPersistedFile: true,
			saveOrganizeState: async (directoryKey, json) => {
				saveCalls.push({ directoryKey, json });
			},
			deleteOrganizeState: async (directoryKey) => {
				deleteCalls.push(directoryKey);
			},
		});

		expect(hasPersistedFile).toBe(false);
		expect(saveCalls).toHaveLength(0);
		expect(deleteCalls).toEqual([buildOrganizeDirectoryKey("/photos")]);
	});

	it("does not create organize JSON before the first decision in a fresh directory", async () => {
		const saveCalls: Array<{ directoryKey: string; json: string }> = [];
		const deleteCalls: string[] = [];

		const hasPersistedFile = await persistOrganizeStateChange({
			directoryPath: "/fresh",
			next: createEmptyOrganizeDirectoryState("/fresh"),
			hasPersistedFile: false,
			saveOrganizeState: async (directoryKey, json) => {
				saveCalls.push({ directoryKey, json });
			},
			deleteOrganizeState: async (directoryKey) => {
				deleteCalls.push(directoryKey);
			},
		});

		expect(hasPersistedFile).toBe(false);
		expect(saveCalls).toHaveLength(0);
		expect(deleteCalls).toHaveLength(0);
	});

	it("propagates delete failures so callers can roll back optimistic state", async () => {
		const file = makeFile("discard-1", "/photos/discard.jpg");
		let persisted = createEmptyOrganizeDirectoryState("/photos");
		persisted = upsertOrganizeDecision(persisted, file, "discard");
		const cleared = clearOrganizeDecision(persisted, file);

		await expect(
			persistOrganizeStateChange({
				directoryPath: "/photos",
				next: cleared,
				hasPersistedFile: true,
				saveOrganizeState: async () => {},
				deleteOrganizeState: async () => {
					throw new Error("delete failed");
				},
			}),
		).rejects.toThrow("delete failed");
	});

	it("propagates save failures so callers can roll back optimistic state", async () => {
		const file = makeFile("keep-1", "/photos/keep.jpg");
		const next = upsertOrganizeDecision(createEmptyOrganizeDirectoryState("/photos"), file, "keep");

		await expect(
			persistOrganizeStateChange({
				directoryPath: "/photos",
				next,
				hasPersistedFile: false,
				saveOrganizeState: async () => {
					throw new Error("save failed");
				},
			}),
		).rejects.toThrow("save failed");
	});
});
