export type {
	SpaceGroup,
	SpaceItem,
	LibraryAction,
	CoreAction,
	Location,
	LocationsListOutput,
	LibraryInfo,
} from "../../generated/types";
export type { SpacedriveClient } from "../../client";
export { useSidebarStore } from "../../stores/sidebar";
export { useLibraryMutation } from "../../hooks/useMutation";
export const getDeviceIcon = () => "";
export const getVolumeIcon = () => "";
