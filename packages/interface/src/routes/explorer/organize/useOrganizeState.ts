import { useCallback, useEffect, useRef, useState } from "react";
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

export function useOrganizeState(args: { currentPath: SdPath | null; files: File[] }) {
	const platform = usePlatform();
	const directoryPath = getPhysicalPath(args.currentPath);
	const [state, setState] = useState<OrganizeDirectoryState | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [hasPersistedFile, setHasPersistedFile] = useState(false);
	const stateRef = useRef<OrganizeDirectoryState | null>(null);

	// Keep ref in sync with state on every render.
	stateRef.current = state;

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
					try {
						const parsed = JSON.parse(json) as OrganizeDirectoryState;
						setState(parsed);
						setHasPersistedFile(true);
					} catch (e) {
						console.warn("Failed to parse organize state, resetting:", e);
						setState(createEmptyOrganizeDirectoryState(directoryPath));
						setHasPersistedFile(false);
					}
				} else {
					setState(createEmptyOrganizeDirectoryState(directoryPath));
				}
			})
			.catch((e) => {
				if (cancelled) return;
				console.warn("Failed to load organize state:", e);
				setState(createEmptyOrganizeDirectoryState(directoryPath));
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
		async (file: File, decision: OrganizeDecision | null) => {
			const prev = stateRef.current;
			if (!prev) return;
			const next = decision ? upsertOrganizeDecision(prev, file, decision) : clearOrganizeDecision(prev, file);
			setState(next);
			stateRef.current = next;
			try {
				await persist(next);
			} catch (e) {
				console.warn("Failed to save organize decision, reverting:", e);
				setState(prev);
				stateRef.current = prev;
			}
		},
		[persist],
	);

	const removeDeleted = useCallback(
		async (deletedPaths: string[]) => {
			const prev = stateRef.current;
			if (!prev) return;
			const next = removeDeletedOrganizeEntries(prev, deletedPaths);
			setState(next);
			stateRef.current = next;
			try {
				await persist(next);
			} catch (e) {
				console.warn("Failed to save organize state after deletion, reverting:", e);
				setState(prev);
				stateRef.current = prev;
			}
		},
		[persist],
	);

	return {
		isSupported: Boolean(directoryPath && platform.loadOrganizeState && platform.saveOrganizeState),
		isLoading,
		state,
		keepFiles: state ? projectOrganizeBucket(args.files, state, "keep") : [],
		discardFiles: state ? projectOrganizeBucket(args.files, state, "discard") : [],
		presentation: state ? buildOrganizePresentation(args.files, state) : [],
		markKeep: (file: File) => applyDecision(file, "keep"),
		markDiscard: (file: File) => applyDecision(file, "discard"),
		clearDecision: (file: File) => applyDecision(file, null),
		removeDeleted,
	};
}
