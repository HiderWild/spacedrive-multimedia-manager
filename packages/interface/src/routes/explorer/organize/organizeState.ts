import type { File } from "@sd/ts-client";
import type { OrganizeDecision, OrganizeDirectoryState, OrganizeItemRecord, OrganizePendingItems } from "./organizeTypes";
import { getOrganizeItemKey, getPhysicalPath, normalizeOrganizePath } from "./organizePersistence";

function getEffectiveOrganizeRecord(
  state: OrganizeDirectoryState,
  key: string,
  pending?: OrganizePendingItems,
): OrganizeItemRecord | null {
  if (pending && key in pending) {
    return pending[key];
  }
  return state.items[key] ?? null;
}

export function upsertOrganizeDecision(
  state: OrganizeDirectoryState,
  file: File,
  decision: OrganizeDecision,
): OrganizeDirectoryState {
  const key = getOrganizeItemKey(file);
  const physicalPath = getPhysicalPath(file.sd_path);
  const kind: OrganizeItemRecord["kind"] = file.kind === "Directory" ? "Directory" : "File";
  const record = {
    itemId: file.id || null,
    path: physicalPath ? normalizeOrganizePath(physicalPath) : null,
    name: file.name,
    kind,
    decision,
    updatedAt: new Date().toISOString(),
  };
  return {
    ...state,
    updatedAt: record.updatedAt,
    items: { ...state.items, [key]: record },
  };
}

export function projectOrganizeBucket(
  files: File[],
  state: OrganizeDirectoryState,
  decision: OrganizeDecision,
  pending?: OrganizePendingItems,
): File[] {
  return files.filter((f) => {
    const key = getOrganizeItemKey(f);
    return getEffectiveOrganizeRecord(state, key, pending)?.decision === decision;
  });
}

export interface OrganizePresentationEntry {
  file: File;
  decision: OrganizeDecision | null;
  dimmed: boolean;
}

export function buildOrganizePresentation(
  files: File[],
  state: OrganizeDirectoryState,
  pending?: OrganizePendingItems,
): OrganizePresentationEntry[] {
  return files.map((file) => {
    const key = getOrganizeItemKey(file);
    const record = getEffectiveOrganizeRecord(state, key, pending);
    return {
      file,
      decision: record?.decision ?? null,
      dimmed: Boolean(record),
    };
  });
}

export function removeDeletedOrganizeEntries(
  state: OrganizeDirectoryState,
  deletedPaths: string[],
): OrganizeDirectoryState {
  if (deletedPaths.length === 0) return state;
  const normalizedDeletes = new Set(deletedPaths.map(normalizeOrganizePath));
  const items = { ...state.items };
  let changed = false;
  for (const [key, record] of Object.entries(items)) {
    if (record.path && normalizedDeletes.has(record.path)) {
      delete items[key];
      changed = true;
    }
  }
  return changed ? { ...state, items, updatedAt: new Date().toISOString() } : state;
}

export function collectDiscardDeleteTargets(
  files: File[],
  state: OrganizeDirectoryState,
): File[] {
  return files.filter(
    (file) =>
      state.items[getOrganizeItemKey(file)]?.decision === "discard" &&
      Boolean(file.sd_path),
  );
}

export function clearOrganizeDecision(
  state: OrganizeDirectoryState,
  file: File,
): OrganizeDirectoryState {
  const key = getOrganizeItemKey(file);
  if (!(key in state.items)) return state;
  const items = { ...state.items };
  delete items[key];
  return { ...state, items, updatedAt: new Date().toISOString() };
}
