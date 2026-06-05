import { describe, it, expect } from "bun:test";
import { canUseOrganizeView } from "../organizeAvailability";
import type { ExplorerMode } from "../../context";
import type { SdPath, SearchFilters } from "@sd/ts-client";

describe("canUseOrganizeView", () => {
	const browseMode: ExplorerMode = { type: "browse" };
	const searchMode: ExplorerMode = { type: "search", query: "test", scope: "folder" };
	const physicalPath: SdPath = { Physical: { device_slug: "dev-1", path: "/photos" } };
	const contentPath: SdPath = { Content: { content_id: "abc123" } };

	const platformWithPersistence = {
		loadOrganizeState: async () => null,
		saveOrganizeState: async () => {},
	};

	const platformWithoutPersistence = {};

	it("returns true when all conditions are met", () => {
		expect(
			canUseOrganizeView({
				mode: browseMode,
				currentPath: physicalPath,
				platform: platformWithPersistence,
			})
		).toBe(true);
	});

	it("returns false when not in browse mode", () => {
		expect(
			canUseOrganizeView({
				mode: searchMode,
				currentPath: physicalPath,
				platform: platformWithPersistence,
			})
		).toBe(false);
	});

	it("returns false when path is null", () => {
		expect(
			canUseOrganizeView({
				mode: browseMode,
				currentPath: null,
				platform: platformWithPersistence,
			})
		).toBe(false);
	});

	it("returns false when path is content-based (non-physical)", () => {
		expect(
			canUseOrganizeView({
				mode: browseMode,
				currentPath: contentPath,
				platform: platformWithPersistence,
			})
		).toBe(false);
	});

	it("returns false when platform lacks loadOrganizeState", () => {
		expect(
			canUseOrganizeView({
				mode: browseMode,
				currentPath: physicalPath,
				platform: platformWithoutPersistence,
			})
		).toBe(false);
	});

	it("returns false when platform lacks saveOrganizeState", () => {
		expect(
			canUseOrganizeView({
				mode: browseMode,
				currentPath: physicalPath,
				platform: { loadOrganizeState: async () => null },
			})
		).toBe(false);
	});

	it("returns false when platform has neither persistence method", () => {
		expect(
			canUseOrganizeView({
				mode: browseMode,
				currentPath: physicalPath,
				platform: {},
			})
		).toBe(false);
	});

	it("returns false for recents mode", () => {
		expect(
			canUseOrganizeView({
				mode: { type: "recents" },
				currentPath: physicalPath,
				platform: platformWithPersistence,
			})
		).toBe(false);
	});

	it("returns false for tag mode", () => {
		expect(
			canUseOrganizeView({
				mode: { type: "tag", tagId: "123" },
				currentPath: physicalPath,
				platform: platformWithPersistence,
			})
		).toBe(false);
	});

	it("returns false for filtered mode", () => {
		const emptyFilters: SearchFilters = {
			file_types: null,
			tags: null,
			date_range: null,
			size_range: null,
			locations: null,
			content_types: null,
			include_hidden: null,
			include_archived: null,
			at_risk: null,
		};
		expect(
			canUseOrganizeView({
				mode: { type: "filtered", filters: emptyFilters, label: "test" },
				currentPath: physicalPath,
				platform: platformWithPersistence,
			})
		).toBe(false);
	});
});
