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

interface LoadedOrganizeState {
	state: OrganizeDirectoryState;
	hasPersistedFile: boolean;
}

interface LoadPersistedOrganizeStateArgs {
	directoryPath: string;
	loadOrganizeState?: (directoryKey: string) => Promise<string | null>;
}

interface PersistOrganizeStateChangeArgs {
	directoryPath: string | null;
	next: OrganizeDirectoryState;
	hasPersistedFile: boolean;
	saveOrganizeState?: (directoryKey: string, json: string) => Promise<void>;
	deleteOrganizeState?: (directoryKey: string) => Promise<void>;
}

function createFreshOrganizeLoad(directoryPath: string): LoadedOrganizeState {
	return {
		state: createEmptyOrganizeDirectoryState(directoryPath),
		hasPersistedFile: false,
	};
}

export async function loadPersistedOrganizeState(
	args: LoadPersistedOrganizeStateArgs,
): Promise<LoadedOrganizeState> {
	const { directoryPath, loadOrganizeState } = args;
	const fresh = createFreshOrganizeLoad(directoryPath);
	if (!loadOrganizeState) return fresh;
	try {
		const json = await loadOrganizeState(buildOrganizeDirectoryKey(directoryPath));
		if (!json) return fresh;
		try {
			return {
				state: JSON.parse(json) as OrganizeDirectoryState,
				hasPersistedFile: true,
			};
		} catch (e) {
			console.warn("Failed to parse organize state, resetting:", e);
			return fresh;
		}
	} catch (e) {
		console.warn("Failed to load organize state:", e);
		return fresh;
	}
}

export async function persistOrganizeStateChange(
	args: PersistOrganizeStateChangeArgs,
): Promise<boolean> {
	const {
		directoryPath,
		next,
		hasPersistedFile,
		saveOrganizeState,
		deleteOrganizeState,
	} = args;
	if (!directoryPath || !saveOrganizeState) return hasPersistedFile;
	const directoryKey = buildOrganizeDirectoryKey(directoryPath);
	if (Object.keys(next.items).length === 0) {
		if (!hasPersistedFile) return false;
		if (deleteOrganizeState) {
			await deleteOrganizeState(directoryKey);
			return false;
		}
	}
	await saveOrganizeState(directoryKey, JSON.stringify(next));
	return true;
}

export function useOrganizeState(args: { currentPath: SdPath | null; files: File[] }) {
	const platform = usePlatform();
	const directoryPath = getPhysicalPath(args.currentPath);
	const [state, setState] = useState<OrganizeDirectoryState | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [hasPersistedFile, setHasPersistedFile] = useState(false);
	const stateRef = useRef<OrganizeDirectoryState | null>(null);
	const hasPersistedFileRef = useRef(false);

	// Keep ref in sync with state on every render.
	stateRef.current = state;
	hasPersistedFileRef.current = hasPersistedFile;

	useEffect(() => {
		if (!directoryPath || !platform.loadOrganizeState) {
			setState(directoryPath ? createEmptyOrganizeDirectoryState(directoryPath) : null);
			setHasPersistedFile(false);
			hasPersistedFileRef.current = false;
			return;
		}
		let cancelled = false;
		setIsLoading(true);
		setHasPersistedFile(false);
		hasPersistedFileRef.current = false;
		loadPersistedOrganizeState({
			directoryPath,
			loadOrganizeState: platform.loadOrganizeState,
		})
			.then((loaded) => {
				if (cancelled) return;
				setState(loaded.state);
				setHasPersistedFile(loaded.hasPersistedFile);
				hasPersistedFileRef.current = loaded.hasPersistedFile;
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
			const nextHasPersistedFile = await persistOrganizeStateChange({
				directoryPath,
				next,
				hasPersistedFile: hasPersistedFileRef.current,
				saveOrganizeState: platform.saveOrganizeState,
				deleteOrganizeState: platform.deleteOrganizeState,
			});
			setHasPersistedFile(nextHasPersistedFile);
			hasPersistedFileRef.current = nextHasPersistedFile;
		},
		[directoryPath, platform],
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
