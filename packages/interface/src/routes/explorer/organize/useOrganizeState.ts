import { useCallback, useEffect, useRef, useState } from "react";
import type { File, SdPath } from "@sd/ts-client";
import { usePlatform } from "../../../contexts/PlatformContext";
import type { OrganizeDecision, OrganizeDirectoryState, OrganizePendingItems } from "./organizeTypes";
import { buildOrganizeDirectoryKey, createEmptyOrganizeDirectoryState, getOrganizeItemKey, getPhysicalPath } from "./organizePersistence";
import {
	upsertOrganizeDecision,
	projectOrganizeBucket,
	buildOrganizePresentation,
	removeDeletedOrganizeEntries,
} from "./organizeState";

const ORGANIZE_FLUSH_DECISION_INTERVAL = 5;

interface LoadedOrganizeState {
	state: OrganizeDirectoryState;
	hasPersistedFile: boolean;
}

interface LoadPersistedOrganizeStateArgs {
	directoryPath: string;
	loadOrganizeState?: (directoryKey: string) => Promise<string | null>;
}

interface LegacyPersistOrganizeStateChangeArgs {
	directoryPath: string | null;
	next: OrganizeDirectoryState;
	hasPersistedFile: boolean;
	saveOrganizeState?: (directoryKey: string, json: string) => Promise<void>;
	deleteOrganizeState?: (directoryKey: string) => Promise<void>;
}

interface PendingPersistOrganizeStateChangeArgs {
	directoryPath: string | null;
	baseline: OrganizeDirectoryState;
	pending: OrganizePendingItems;
	hasPersistedFile: boolean;
	effectiveDecisionCount: number;
	flush?: boolean;
	saveOrganizeState?: (directoryKey: string, json: string) => Promise<void>;
	deleteOrganizeState?: (directoryKey: string) => Promise<void>;
}

interface PersistedPendingOrganizeStateChange {
	baseline: OrganizeDirectoryState;
	pending: OrganizePendingItems;
	hasPersistedFile: boolean;
	flushed: boolean;
}

function createFreshOrganizeLoad(directoryPath: string): LoadedOrganizeState {
	return {
		state: createEmptyOrganizeDirectoryState(directoryPath),
		hasPersistedFile: false,
	};
}

function hasPendingOrganizeChanges(pending: OrganizePendingItems): boolean {
	return Object.keys(pending).length > 0;
}

function hasOrganizeItems(state: OrganizeDirectoryState): boolean {
	return Object.keys(state.items).length > 0;
}

function sameEffectiveDecision(
	baseline: OrganizeDirectoryState,
	key: string,
	nextDecision: OrganizeDecision | null,
): boolean {
	return (baseline.items[key]?.decision ?? null) === nextDecision;
}

function buildPendingDecisionRecord(
	baseline: OrganizeDirectoryState,
	file: File,
	decision: OrganizeDecision,
) {
	return upsertOrganizeDecision(createEmptyOrganizeDirectoryState(baseline.directoryPath), file, decision).items[
		getOrganizeItemKey(file)
	]!;
}

function updatePendingDecision(
	baseline: OrganizeDirectoryState,
	pending: OrganizePendingItems,
	file: File,
	decision: OrganizeDecision | null,
): OrganizePendingItems {
	const key = getOrganizeItemKey(file);
	const next = { ...pending };
	if (decision === null) {
		if (baseline.items[key]) {
			next[key] = null;
		} else {
			delete next[key];
		}
		return next;
	}
	if (sameEffectiveDecision(baseline, key, decision)) {
		delete next[key];
		return next;
	}
	next[key] = buildPendingDecisionRecord(baseline, file, decision);
	return next;
}

function pendingChanged(a: OrganizePendingItems, b: OrganizePendingItems): boolean {
	const aKeys = Object.keys(a);
	const bKeys = Object.keys(b);
	if (aKeys.length !== bKeys.length) return true;
	return bKeys.some((key) => a[key]?.decision !== b[key]?.decision);
}

async function persistMaterializedOrganizeState(args: {
	directoryPath: string | null;
	next: OrganizeDirectoryState;
	hasPersistedFile: boolean;
	saveOrganizeState?: (directoryKey: string, json: string) => Promise<void>;
	deleteOrganizeState?: (directoryKey: string) => Promise<void>;
}): Promise<boolean> {
	const {
		directoryPath,
		next,
		hasPersistedFile,
		saveOrganizeState,
		deleteOrganizeState,
	} = args;
	if (!directoryPath || !saveOrganizeState) return hasPersistedFile;
	const directoryKey = buildOrganizeDirectoryKey(directoryPath);
	if (!hasOrganizeItems(next)) {
		if (!hasPersistedFile) return false;
		if (deleteOrganizeState) {
			await deleteOrganizeState(directoryKey);
			return false;
		}
		return hasPersistedFile;
	}
	await saveOrganizeState(directoryKey, JSON.stringify(next));
	return true;
}

async function flushPendingOrganizeState(
	args: Omit<PendingPersistOrganizeStateChangeArgs, "effectiveDecisionCount" | "flush">,
): Promise<PersistedPendingOrganizeStateChange> {
	const materialized = materializeOrganizeState(args.baseline, args.pending);
	const nextHasPersistedFile = await persistMaterializedOrganizeState({
		directoryPath: args.directoryPath,
		next: materialized,
		hasPersistedFile: args.hasPersistedFile,
		saveOrganizeState: args.saveOrganizeState,
		deleteOrganizeState: args.deleteOrganizeState,
	});
	return {
		baseline: materialized,
		pending: {},
		hasPersistedFile: nextHasPersistedFile,
		flushed: true,
	};
}

export function materializeOrganizeState(
	baseline: OrganizeDirectoryState,
	pending: OrganizePendingItems,
): OrganizeDirectoryState {
	if (!hasPendingOrganizeChanges(pending)) return baseline;
	const items = { ...baseline.items };
	for (const [key, record] of Object.entries(pending)) {
		if (record) {
			items[key] = record;
		} else {
			delete items[key];
		}
	}
	return {
		...baseline,
		updatedAt: new Date().toISOString(),
		items,
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
	args: LegacyPersistOrganizeStateChangeArgs,
): Promise<boolean>;
export async function persistOrganizeStateChange(
	args: PendingPersistOrganizeStateChangeArgs,
): Promise<PersistedPendingOrganizeStateChange>;
export async function persistOrganizeStateChange(
	args: LegacyPersistOrganizeStateChangeArgs | PendingPersistOrganizeStateChangeArgs,
): Promise<boolean | PersistedPendingOrganizeStateChange> {
	if ("next" in args) {
		return persistMaterializedOrganizeState(args);
	}
	const materialized = materializeOrganizeState(args.baseline, args.pending);
	const hasPendingChanges = hasPendingOrganizeChanges(args.pending);
	const shouldFlush =
		args.flush === true ||
		(hasPendingChanges && !args.hasPersistedFile && hasOrganizeItems(materialized)) ||
		(hasPendingChanges && args.hasPersistedFile && !hasOrganizeItems(materialized)) ||
		(hasPendingChanges &&
			args.effectiveDecisionCount > 0 &&
			args.effectiveDecisionCount % ORGANIZE_FLUSH_DECISION_INTERVAL === 0);
	if (!shouldFlush) {
		return {
			baseline: args.baseline,
			pending: args.pending,
			hasPersistedFile: args.hasPersistedFile,
			flushed: false,
		};
	}
	return flushPendingOrganizeState(args);
}

export function useOrganizeState(args: { currentPath: SdPath | null; files: File[] }) {
	const platform = usePlatform();
	const directoryPath = getPhysicalPath(args.currentPath);
	const [state, setState] = useState<OrganizeDirectoryState | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [hasPersistedFile, setHasPersistedFile] = useState(false);
	const baselineRef = useRef<OrganizeDirectoryState | null>(null);
	const pendingRef = useRef<OrganizePendingItems>({});
	const hasPersistedFileRef = useRef(false);
	const effectiveDecisionCountRef = useRef(0);

	useEffect(() => {
		const flushCurrentDirectory = async () => {
			const baseline = baselineRef.current;
			const pending = pendingRef.current;
			if (!directoryPath || !baseline || !hasPendingOrganizeChanges(pending)) return;
			try {
				const result = await persistOrganizeStateChange({
					directoryPath,
					baseline,
					pending,
					hasPersistedFile: hasPersistedFileRef.current,
					effectiveDecisionCount: effectiveDecisionCountRef.current,
					flush: true,
					saveOrganizeState: platform.saveOrganizeState,
					deleteOrganizeState: platform.deleteOrganizeState,
				});
				baselineRef.current = result.baseline;
				pendingRef.current = result.pending;
				hasPersistedFileRef.current = result.hasPersistedFile;
				effectiveDecisionCountRef.current = 0;
			} catch (e) {
				console.warn("Failed to flush organize state before navigation:", e);
			}
		};

		if (!directoryPath || !platform.loadOrganizeState) {
			const fresh = directoryPath ? createEmptyOrganizeDirectoryState(directoryPath) : null;
			baselineRef.current = fresh;
			pendingRef.current = {};
			effectiveDecisionCountRef.current = 0;
			setState(fresh);
			setHasPersistedFile(false);
			hasPersistedFileRef.current = false;
			return;
		}
		let cancelled = false;
		setIsLoading(true);
		setHasPersistedFile(false);
		hasPersistedFileRef.current = false;
		pendingRef.current = {};
		effectiveDecisionCountRef.current = 0;
		loadPersistedOrganizeState({
			directoryPath,
			loadOrganizeState: platform.loadOrganizeState,
		})
			.then((loaded) => {
				if (cancelled) return;
				baselineRef.current = loaded.state;
				pendingRef.current = {};
				setState(loaded.state);
				setHasPersistedFile(loaded.hasPersistedFile);
				hasPersistedFileRef.current = loaded.hasPersistedFile;
			})
			.finally(() => {
				if (!cancelled) setIsLoading(false);
			});
		return () => {
			cancelled = true;
			void flushCurrentDirectory();
		};
	}, [directoryPath, platform]);

	const persist = useCallback(
		async (baseline: OrganizeDirectoryState, pending: OrganizePendingItems) => {
			const result = await persistOrganizeStateChange({
				directoryPath,
				baseline,
				pending,
				hasPersistedFile: hasPersistedFileRef.current,
				effectiveDecisionCount: effectiveDecisionCountRef.current,
				saveOrganizeState: platform.saveOrganizeState,
				deleteOrganizeState: platform.deleteOrganizeState,
			});
			if (result.flushed) {
				effectiveDecisionCountRef.current = 0;
			}
			baselineRef.current = result.baseline;
			pendingRef.current = result.pending;
			setHasPersistedFile(result.hasPersistedFile);
			hasPersistedFileRef.current = result.hasPersistedFile;
			setState(materializeOrganizeState(result.baseline, result.pending));
		},
		[directoryPath, platform],
	);

	const applyDecision = useCallback(
		async (file: File, decision: OrganizeDecision | null) => {
			const baseline = baselineRef.current;
			if (!baseline) return;
			const prevPending = pendingRef.current;
			const prevCount = effectiveDecisionCountRef.current;
			const prevState = materializeOrganizeState(baseline, prevPending);
			const nextPending = updatePendingDecision(baseline, prevPending, file, decision);
			if (!pendingChanged(prevPending, nextPending)) return;
			effectiveDecisionCountRef.current += 1;
			const nextState = materializeOrganizeState(baseline, nextPending);
			pendingRef.current = nextPending;
			setState(nextState);
			try {
				await persist(baseline, nextPending);
			} catch (e) {
				console.warn("Failed to save organize decision, reverting:", e);
				pendingRef.current = prevPending;
				effectiveDecisionCountRef.current = prevCount;
				setState(prevState);
			}
		},
		[persist],
	);

	const removeDeleted = useCallback(
		async (deletedPaths: string[]) => {
			const baseline = baselineRef.current;
			if (!baseline) return;
			const prevPending = pendingRef.current;
			const prevCount = effectiveDecisionCountRef.current;
			const prevState = materializeOrganizeState(baseline, prevPending);
			const cleaned = removeDeletedOrganizeEntries(prevState, deletedPaths);
			if (cleaned === prevState) return;
			baselineRef.current = cleaned;
			pendingRef.current = {};
			effectiveDecisionCountRef.current = 0;
			setState(cleaned);
			try {
				const result = await persistOrganizeStateChange({
					directoryPath,
					baseline: cleaned,
					pending: {},
					hasPersistedFile: hasPersistedFileRef.current,
					effectiveDecisionCount: 0,
					flush: true,
					saveOrganizeState: platform.saveOrganizeState,
					deleteOrganizeState: platform.deleteOrganizeState,
				});
				baselineRef.current = result.baseline;
				pendingRef.current = result.pending;
				setHasPersistedFile(result.hasPersistedFile);
				hasPersistedFileRef.current = result.hasPersistedFile;
				setState(result.baseline);
			} catch (e) {
				console.warn("Failed to save organize state after deletion, reverting:", e);
				baselineRef.current = baseline;
				pendingRef.current = prevPending;
				effectiveDecisionCountRef.current = prevCount;
				setState(prevState);
			}
		},
		[directoryPath, platform],
	);

	const flushPending = useCallback(async () => {
		const baseline = baselineRef.current;
		const pending = pendingRef.current;
		if (!directoryPath || !baseline || !hasPendingOrganizeChanges(pending)) return;
		try {
			const result = await persistOrganizeStateChange({
				directoryPath,
				baseline,
				pending,
				hasPersistedFile: hasPersistedFileRef.current,
				effectiveDecisionCount: effectiveDecisionCountRef.current,
				flush: true,
				saveOrganizeState: platform.saveOrganizeState,
				deleteOrganizeState: platform.deleteOrganizeState,
			});
			baselineRef.current = result.baseline;
			pendingRef.current = result.pending;
			hasPersistedFileRef.current = result.hasPersistedFile;
			effectiveDecisionCountRef.current = 0;
			setHasPersistedFile(result.hasPersistedFile);
			setState(result.baseline);
		} catch (e) {
			console.warn("Failed to flush organize state:", e);
		}
	}, [directoryPath, platform]);

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
		flushPending,
	};
}
