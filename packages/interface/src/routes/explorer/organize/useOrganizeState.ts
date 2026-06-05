import { useCallback, useEffect, useState } from "react";
import type { File, SdPath } from "@sd/ts-client";
import { usePlatform } from "../../../contexts/PlatformContext";
import type { OrganizeDecision, OrganizeDirectoryState } from "./organizeTypes";
import { buildOrganizeDirectoryKey, createEmptyOrganizeDirectoryState, getOrganizeItemKey, getPhysicalPath } from "./organizePersistence";
import {
	upsertOrganizeDecision,
	projectOrganizeBucket,
	buildOrganizePresentation,
	removeDeletedOrganizeEntries,
	clearOrganizeDecision,
} from "./organizeState";

export function useOrganizeState(args: { currentPath: SdPath | null; files: Pick<File, "id" | "sd_path" | "name" | "kind">[] }) {
	const platform = usePlatform();
	const directoryPath = getPhysicalPath(args.currentPath);
	const [state, setState] = useState<OrganizeDirectoryState | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [hasPersistedFile, setHasPersistedFile] = useState(false);

	useEffect(() => {
		if (!directoryPath || !platform.loadOrganizeState) {
			setState(directoryPath ? createEmptyOrganizeDirectoryState(directoryPath) : null);
			setHasPersistedFile(false);
			return;
		}
		let cancelled = false;
		setIsLoading(true);
		platform
			.loadOrganizeState(buildOrganizeDirectoryKey(directoryPath))
			.then((json) => {
				if (cancelled) return;
				if (json) {
					setState(JSON.parse(json) as OrganizeDirectoryState);
					setHasPersistedFile(true);
				} else {
					setState(createEmptyOrganizeDirectoryState(directoryPath));
				}
			})
			.finally(() => {
				if (!cancelled) setIsLoading(false);
			});
		return () => {
			cancelled = true;
		};
	}, [directoryPath, platform]);

	const persist = useCallback(
		async (next: OrganizeDirectoryState) => {
			if (!directoryPath || !platform.saveOrganizeState) return;
			if (!hasPersistedFile && Object.keys(next.items).length === 0) return;
			await platform.saveOrganizeState(buildOrganizeDirectoryKey(directoryPath), JSON.stringify(next));
			setHasPersistedFile(true);
		},
		[directoryPath, hasPersistedFile, platform],
	);

	const applyDecision = useCallback(
		async (file: Pick<File, "id" | "sd_path" | "name" | "kind">, decision: OrganizeDecision | null) => {
			if (!state) return;
			const next = decision ? upsertOrganizeDecision(state, file, decision) : clearOrganizeDecision(state, file);
			setState(next);
			await persist(next);
		},
		[persist, state],
	);

	const removeDeleted = useCallback(
		async (deletedPaths: string[]) => {
			if (!state) return;
			const next = removeDeletedOrganizeEntries(state, deletedPaths);
			setState(next);
			await persist(next);
		},
		[persist, state],
	);

	return {
		isSupported: Boolean(directoryPath && platform.loadOrganizeState && platform.saveOrganizeState),
		isLoading,
		state,
		keepFiles: state ? projectOrganizeBucket(args.files, state, "keep") : [],
		discardFiles: state ? projectOrganizeBucket(args.files, state, "discard") : [],
		presentation: state ? buildOrganizePresentation(args.files, state) : [],
		markKeep: (file: Pick<File, "id" | "sd_path" | "name" | "kind">) => applyDecision(file, "keep"),
		markDiscard: (file: Pick<File, "id" | "sd_path" | "name" | "kind">) => applyDecision(file, "discard"),
		clearDecision: (file: Pick<File, "id" | "sd_path" | "name" | "kind">) => applyDecision(file, null),
		removeDeleted,
	};
}
