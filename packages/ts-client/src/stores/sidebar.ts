import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface DraggedItem {
	type: 'file' | 'space-item' | 'space-group';
	data: any;
}

interface SidebarStore {
	// Persisted state
	currentSpaceId: string | null;
	setCurrentSpace: (id: string | null) => void;

	// Ephemeral state
	collapsedGroupOverrides: Record<string, boolean>;
	setGroupCollapsedOverride: (groupId: string, collapsed: boolean) => void;
	clearGroupCollapsedOverride: (groupId: string) => void;
	clearGroupCollapsedOverrides: () => void;
	collapseAll: (groupIds: string[]) => void;
	expandAll: () => void;

	// Drag state
	draggedItem: DraggedItem | null;
	setDraggedItem: (item: DraggedItem | null) => void;
}

export const useSidebarStore = create<SidebarStore>()(
	persist(
		(set) => ({
			// Persisted
			currentSpaceId: null,
			setCurrentSpace: (id) =>
				set((state) => ({
					currentSpaceId: id,
					collapsedGroupOverrides:
						state.currentSpaceId === id ? state.collapsedGroupOverrides : {},
				})),

			// Ephemeral
			collapsedGroupOverrides: {},
			setGroupCollapsedOverride: (groupId, collapsed) =>
				set((state) => ({
					collapsedGroupOverrides: {
						...state.collapsedGroupOverrides,
						[groupId]: collapsed,
					},
				})),
			clearGroupCollapsedOverride: (groupId) =>
				set((state) => {
					const { [groupId]: _, ...rest } = state.collapsedGroupOverrides;
					return { collapsedGroupOverrides: rest };
				}),
			clearGroupCollapsedOverrides: () => set({ collapsedGroupOverrides: {} }),
			collapseAll: (groupIds) =>
				set({
					collapsedGroupOverrides: Object.fromEntries(
						groupIds.map((groupId) => [groupId, true]),
					),
				}),
			expandAll: () => set({ collapsedGroupOverrides: {} }),

			// Drag
			draggedItem: null,
			setDraggedItem: (item) => set({ draggedItem: item }),
		}),
		{
			name: 'spacedrive-sidebar',
			partialize: (state) => ({
				currentSpaceId: state.currentSpaceId,
			}),
		}
	)
);
