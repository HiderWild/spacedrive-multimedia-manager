import type { SdPath } from "@sd/ts-client";
import type { ExplorerMode } from "../context";

interface OrganizePlatform {
	loadOrganizeState?(directoryKey: string): Promise<string | null>;
	saveOrganizeState?(directoryKey: string, json: string): Promise<void>;
}

interface CanUseOrganizeViewArgs {
	mode: ExplorerMode;
	currentPath: SdPath | null;
	platform: OrganizePlatform;
}

function isPhysicalPath(path: SdPath): path is Extract<SdPath, { Physical: unknown }> {
	return "Physical" in path && path.Physical != null;
}

export function canUseOrganizeView(args: CanUseOrganizeViewArgs): boolean {
	const { mode, currentPath, platform } = args;
	return (
		mode.type === "browse" &&
		currentPath !== null &&
		isPhysicalPath(currentPath) &&
		typeof platform.loadOrganizeState === "function" &&
		typeof platform.saveOrganizeState === "function"
	);
}
