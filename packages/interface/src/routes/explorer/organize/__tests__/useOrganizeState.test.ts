import { describe, expect, it } from "bun:test";
import type { File } from "@sd/ts-client";
import { buildOrganizeDirectoryKey, createEmptyOrganizeDirectoryState } from "../organizePersistence";
import { clearOrganizeDecision, removeDeletedOrganizeEntries, upsertOrganizeDecision } from "../organizeState";
import { loadPersistedOrganizeState, persistOrganizeStateChange } from "../useOrganizeState";
import type { OrganizePendingItems } from "../organizeTypes";

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

describe("organize baseline and pending persistence", () => {
	it("saves the first effective decision in a fresh directory immediately", async () => {
		const file = makeFile("keep-1", "/photos/keep.jpg");
		const baseline = createEmptyOrganizeDirectoryState("/photos");
		const pendingRecord = upsertOrganizeDecision(createEmptyOrganizeDirectoryState("/photos"), file, "keep").items[
			`id:${file.id}`
		]!;
		const saveCalls: Array<{ directoryKey: string; json: string }> = [];

		const result = await persistOrganizeStateChange({
			directoryPath: "/photos",
			baseline,
			pending: { [`id:${file.id}`]: pendingRecord },
			hasPersistedFile: false,
			effectiveDecisionCount: 1,
			saveOrganizeState: async (directoryKey, json) => {
				saveCalls.push({ directoryKey, json });
			},
		});

		expect(result.hasPersistedFile).toBe(true);
		expect(result.baseline.items[`id:${file.id}`]?.decision).toBe("keep");
		expect(result.pending).toEqual({});
		expect(saveCalls).toHaveLength(1);
	});

	it("defers flushing until every fifth effective decision after persistence exists", async () => {
		const file = makeFile("keep-1", "/photos/keep.jpg");
		const baseline = createEmptyOrganizeDirectoryState("/photos");
		const pendingRecord = upsertOrganizeDecision(createEmptyOrganizeDirectoryState("/photos"), file, "keep").items[
			`id:${file.id}`
		]!;
		const saveCalls: Array<{ directoryKey: string; json: string }> = [];

		const deferred = await persistOrganizeStateChange({
			directoryPath: "/photos",
			baseline,
			pending: { [`id:${file.id}`]: pendingRecord },
			hasPersistedFile: true,
			effectiveDecisionCount: 4,
			saveOrganizeState: async (directoryKey, json) => {
				saveCalls.push({ directoryKey, json });
			},
		});

		expect(deferred.hasPersistedFile).toBe(true);
		expect(deferred.baseline).toBe(baseline);
		expect(deferred.pending[`id:${file.id}`]).toBeDefined();
		expect(saveCalls).toHaveLength(0);

		const flushed = await persistOrganizeStateChange({
			directoryPath: "/photos",
			baseline,
			pending: { [`id:${file.id}`]: pendingRecord },
			hasPersistedFile: true,
			effectiveDecisionCount: 5,
			saveOrganizeState: async (directoryKey, json) => {
				saveCalls.push({ directoryKey, json });
			},
		});

		expect(flushed.baseline.items[`id:${file.id}`]?.decision).toBe("keep");
		expect(flushed.pending).toEqual({});
		expect(saveCalls).toHaveLength(1);
	});

	it("flushes pending state immediately when requested for navigation", async () => {
		const file = makeFile("keep-1", "/photos/keep.jpg");
		const baseline = createEmptyOrganizeDirectoryState("/photos");
		const pendingRecord = upsertOrganizeDecision(createEmptyOrganizeDirectoryState("/photos"), file, "keep").items[
			`id:${file.id}`
		]!;
		const saveCalls: string[] = [];

		const flushed = await persistOrganizeStateChange({
			directoryPath: "/photos",
			baseline,
			pending: { [`id:${file.id}`]: pendingRecord },
			hasPersistedFile: true,
			effectiveDecisionCount: 1,
			flush: true,
			saveOrganizeState: async (_directoryKey, json) => {
				saveCalls.push(json);
			},
		});

		expect(flushed.baseline.items[`id:${file.id}`]?.decision).toBe("keep");
		expect(flushed.pending).toEqual({});
		expect(saveCalls).toHaveLength(1);
	});

	it("deletes persistence when final materialized state is empty", async () => {
		const file = makeFile("keep-1", "/photos/keep.jpg");
		let baseline = createEmptyOrganizeDirectoryState("/photos");
		baseline = upsertOrganizeDecision(baseline, file, "keep");
		const deleteCalls: string[] = [];

		const result = await persistOrganizeStateChange({
			directoryPath: "/photos",
			baseline,
			pending: { [`id:${file.id}`]: null },
			hasPersistedFile: true,
			effectiveDecisionCount: 1,
			flush: true,
			saveOrganizeState: async () => {
				throw new Error("should not save empty state");
			},
			deleteOrganizeState: async (directoryKey) => {
				deleteCalls.push(directoryKey);
			},
		});

		expect(result.hasPersistedFile).toBe(false);
		expect(result.baseline.items).toEqual({});
		expect(result.pending).toEqual({});
		expect(deleteCalls).toEqual([buildOrganizeDirectoryKey("/photos")]);
	});

	it("browsing without pending decisions never creates persistence", async () => {
		const saveCalls: string[] = [];

		const result = await persistOrganizeStateChange({
			directoryPath: "/fresh",
			baseline: createEmptyOrganizeDirectoryState("/fresh"),
			pending: {},
			hasPersistedFile: false,
			effectiveDecisionCount: 0,
			flush: true,
			saveOrganizeState: async (_directoryKey, json) => {
				saveCalls.push(json);
			},
		});

		expect(result.hasPersistedFile).toBe(false);
		expect(result.pending).toEqual({});
		expect(saveCalls).toHaveLength(0);
	});

	it("does not flush every fifth interaction when pending changes reconcile to the baseline", async () => {
		const file = makeFile("keep-1", "/photos/keep.jpg");
		const baseline = upsertOrganizeDecision(createEmptyOrganizeDirectoryState("/photos"), file, "keep");
		const saveCalls: string[] = [];

		const result = await persistOrganizeStateChange({
			directoryPath: "/photos",
			baseline,
			pending: {},
			hasPersistedFile: true,
			effectiveDecisionCount: 5,
			saveOrganizeState: async (_directoryKey, json) => {
				saveCalls.push(json);
			},
		});

		expect(result.flushed).toBe(false);
		expect(result.baseline).toBe(baseline);
		expect(result.pending).toEqual({});
		expect(saveCalls).toHaveLength(0);
	});

	it("delete settlement removes persistence after deleted entries clear all remaining decisions", async () => {
		const discard = makeFile("discard-1", "/photos/discard.jpg");
		const baseline = upsertOrganizeDecision(createEmptyOrganizeDirectoryState("/photos"), discard, "discard");
		const cleaned = removeDeletedOrganizeEntries(baseline, ["/photos/discard.jpg"]);
		const saveCalls: string[] = [];
		const deleteCalls: string[] = [];

		const result = await persistOrganizeStateChange({
			directoryPath: "/photos",
			baseline: cleaned,
			pending: {},
			hasPersistedFile: true,
			effectiveDecisionCount: 0,
			flush: true,
			saveOrganizeState: async (_directoryKey, json) => {
				saveCalls.push(json);
			},
			deleteOrganizeState: async (directoryKey) => {
				deleteCalls.push(directoryKey);
			},
		});

		expect(result.hasPersistedFile).toBe(false);
		expect(result.baseline.items).toEqual({});
		expect(result.pending).toEqual({});
		expect(saveCalls).toHaveLength(0);
		expect(deleteCalls).toEqual([buildOrganizeDirectoryKey("/photos")]);
	});
});
