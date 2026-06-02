import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import {
	SpacedriveClientContext,
} from "../hooks/useClient";
import { SpaceGroup } from "../../../interface/src/components/SpacesSidebar/SpaceGroup";
import { useSidebarStore } from "../stores/sidebar";

jest.mock(
	"@dnd-kit/core",
	() => ({
		useDndContext: () => ({ active: null }),
		useDroppable: () => ({ setNodeRef: jest.fn(), isOver: false }),
	}),
	{ virtual: true },
);

jest.mock(
	"../../../interface/src/components/SpacesSidebar/DevicesGroup",
	() => ({
		DevicesGroup: () => null,
	}),
);

jest.mock(
	"../../../interface/src/components/SpacesSidebar/LocationsGroup",
	() => ({
		LocationsGroup: () => null,
	}),
);

jest.mock(
	"../../../interface/src/components/SpacesSidebar/VolumesGroup",
	() => ({
		VolumesGroup: ({
			isCollapsed,
			onToggle,
		}: {
			isCollapsed: boolean;
			onToggle: () => void;
		}) => (
			<div>
				<button onClick={onToggle} type="button">
					Toggle Volumes
				</button>
				{!isCollapsed && <div>Volume Content</div>}
			</div>
		),
	}),
);

jest.mock("../../../interface/src/components/SpacesSidebar/TagsGroup", () => ({
	TagsGroup: () => null,
}));

jest.mock(
	"../../../interface/src/components/SpacesSidebar/SourcesGroup",
	() => ({
		SourcesGroup: () => null,
	}),
);

jest.mock("../../../interface/src/components/SpacesSidebar/SpaceItem", () => ({
	SpaceItem: () => null,
}));

jest.mock(
	"../../../interface/src/components/SpacesSidebar/GroupHeader",
	() => ({
		GroupHeader: () => null,
	}),
);

describe("SpaceGroup collapse behavior", () => {
	beforeEach(() => {
		localStorage.clear();
		useSidebarStore.setState({
			currentSpaceId: null,
			collapsedGroupOverrides: {},
			draggedItem: null,
		});
	});

	afterEach(() => {
		cleanup();
	});

	test("collapses dynamic groups immediately on click before backend props change", async () => {
		const queryClient = new QueryClient();
		const execute = jest.fn().mockResolvedValue({
			group: {
				id: "group-1",
				is_collapsed: true,
			},
		});
		const client = {
			execute,
			getCurrentLibraryId: () => "library-1",
			on: jest.fn(),
			off: jest.fn(),
		} as any;

		const layoutKey = ["query:spaces.get_layout", "library-1", { space_id: "space-1" }];
		queryClient.setQueryData(layoutKey, {
			space_items: [],
			groups: [
				{
					group: {
						id: "group-1",
						space_id: "space-1",
						name: "Volumes",
						group_type: "Volumes",
						is_collapsed: false,
						order: 0,
						created_at: "2026-06-03T00:00:00Z",
					},
					items: [],
				},
			],
		});

		const { getByText, queryByText } = render(
			<QueryClientProvider client={queryClient}>
				<SpacedriveClientContext.Provider value={client}>
					<SpaceGroup
						group={{
							id: "group-1",
							space_id: "space-1",
							name: "Volumes",
							group_type: "Volumes",
							is_collapsed: false,
							order: 0,
							created_at: "2026-06-03T00:00:00Z",
						} as any}
						items={[]}
						spaceId="space-1"
					/>
				</SpacedriveClientContext.Provider>
			</QueryClientProvider>,
		);

		expect(getByText("Volume Content")).not.toBeNull();

		fireEvent.click(getByText("Toggle Volumes"));

		expect(queryByText("Volume Content")).toBeNull();
	});

	test("updates the space layout cache after collapse mutation succeeds", async () => {
		const queryClient = new QueryClient();
		const execute = jest.fn().mockResolvedValue({
			group: {
				id: "group-1",
				is_collapsed: true,
			},
		});
		const client = {
			execute,
			getCurrentLibraryId: () => "library-1",
			on: jest.fn(),
			off: jest.fn(),
		} as any;

		const layoutKey = ["query:spaces.get_layout", "library-1", { space_id: "space-1" }];
		queryClient.setQueryData(layoutKey, {
			space_items: [],
			groups: [
				{
					group: {
						id: "group-1",
						space_id: "space-1",
						name: "Volumes",
						group_type: "Volumes",
						is_collapsed: false,
						order: 0,
						created_at: "2026-06-03T00:00:00Z",
					},
					items: [],
				},
			],
		});

		const { getByText } = render(
			<QueryClientProvider client={queryClient}>
				<SpacedriveClientContext.Provider value={client}>
					<SpaceGroup
						group={{
							id: "group-1",
							space_id: "space-1",
							name: "Volumes",
							group_type: "Volumes",
							is_collapsed: false,
							order: 0,
							created_at: "2026-06-03T00:00:00Z",
						} as any}
						items={[]}
						spaceId="space-1"
					/>
				</SpacedriveClientContext.Provider>
			</QueryClientProvider>,
		);

		fireEvent.click(getByText("Toggle Volumes"));

		await waitFor(() => {
			expect(execute).toHaveBeenCalled();
		});

		expect((queryClient.getQueryData(layoutKey) as any).groups[0].group.is_collapsed).toBe(
			true,
		);
	});
});
