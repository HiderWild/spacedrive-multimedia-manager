import type { File } from "@sd/ts-client";
import type { OrganizeDecision, OrganizeDirectoryState, OrganizeItemRecord } from "./organizeTypes";
import { getOrganizeItemKey, getPhysicalPath, normalizeOrganizePath } from "./organizePersistence";

export function upsertOrganizeDecision(
  state: OrganizeDirectoryState,
  file: Pick<File, "id" | "sd_path" | "name" | "kind">,
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
  files: Pick<File, "id" | "sd_path" | "name" | "kind">[],
  state: OrganizeDirectoryState,
  decision: OrganizeDecision,
): Pick<File, "id" | "sd_path" | "name" | "kind">[] {
  return files.filter((f) => {
    const key = getOrganizeItemKey(f);
    return state.items[key]?.decision === decision;
  });
}

export interface OrganizePresentationEntry {
  file: Pick<File, "id" | "sd_path" | "name" | "kind">;
  decision: OrganizeDecision | null;
  dimmed: boolean;
}

export function buildOrganizePresentation(
  files: Pick<File, "id" | "sd_path" | "name" | "kind">[],
  state: OrganizeDirectoryState,
): OrganizePresentationEntry[] {
  return files.map((file) => {
    const key = getOrganizeItemKey(file);
    const record = state.items[key] ?? null;
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
