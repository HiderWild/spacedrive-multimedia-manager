import { useSidebarStore } from "../sidebar";

describe("sidebar store collapsed overrides", () => {
	beforeEach(() => {
		localStorage.clear();
		useSidebarStore.setState({
			currentSpaceId: null,
			collapsedGroupOverrides: {},
			draggedItem: null,
		});
	});

	test("stores absolute collapsed overrides by group id", () => {
		const store = useSidebarStore.getState() as any;

		store.setGroupCollapsedOverride("group-1", true);

		expect(
			(useSidebarStore.getState() as any).collapsedGroupOverrides["group-1"],
		).toBe(true);
	});

	test("clears collapsed overrides when switching spaces", () => {
		const store = useSidebarStore.getState() as any;

		store.setGroupCollapsedOverride("group-1", true);
		store.setCurrentSpace("space-2");

		expect(
			(useSidebarStore.getState() as any).collapsedGroupOverrides,
		).toEqual({});
	});
});
