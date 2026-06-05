import { describe, expect, test } from "bun:test";
import type { File } from "@sd/ts-client";
import { createEmptyOrganizeDirectoryState } from "../organizePersistence";
import { collectDiscardDeleteTargets, upsertOrganizeDecision } from "../organizeState";

const makeFile = (overrides: Partial<File>): File =>
	({
		id: "file-1",
		name: "clip",
		kind: "File",
		extension: "mp4",
		sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/clip.mp4" } },
		...overrides,
	}) as File;

describe("collectDiscardDeleteTargets", () => {
	test("returns only discard-marked direct children with valid sd_path", () => {
		let state = createEmptyOrganizeDirectoryState("C:/Photos");
		const keepFile = makeFile({
			id: "keep-1",
			sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/keep.mp4" } },
		});
		const discardFile = makeFile({
			id: "discard-1",
			sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/discard.mp4" } },
		});
		state = upsertOrganizeDecision(state, keepFile, "keep");
		state = upsertOrganizeDecision(state, discardFile, "discard");
		expect(collectDiscardDeleteTargets([keepFile, discardFile], state).map((f) => f.id)).toEqual([
			"discard-1",
		]);
	});

	test("excludes files without sd_path", () => {
		let state = createEmptyOrganizeDirectoryState("C:/Photos");
		const noPathFile = makeFile({ id: "no-path", sd_path: null as unknown as File["sd_path"] });
		state = upsertOrganizeDecision(state, noPathFile, "discard");
		expect(collectDiscardDeleteTargets([noPathFile], state)).toEqual([]);
	});

	test("returns empty array when no files are discard-marked", () => {
		const state = createEmptyOrganizeDirectoryState("C:/Photos");
		const file = makeFile({ id: "fresh-1" });
		expect(collectDiscardDeleteTargets([file], state)).toEqual([]);
	});
});
