import type { File } from "@sd/ts-client";
import type { OrganizeDirectoryState } from "./organizeTypes";

function fnv1a64(input: string): string {
  let hash = BigInt("14695981039346656037"); // FNV offset basis
  const prime = BigInt("1099511628211"); // FNV prime
  for (let i = 0; i < input.length; i++) {
    hash ^= BigInt(input.charCodeAt(i));
    hash = (hash * prime) & BigInt("0xFFFFFFFFFFFFFFFF");
  }
  return hash.toString(16).padStart(16, "0");
}

export function normalizeOrganizePath(physicalPath: string): string {
  let normalized = physicalPath.replace(/\\/g, "/");
  normalized = normalized.replace(/\/+/g, "/");
  if (normalized.length > 1 && normalized.endsWith("/")) {
    normalized = normalized.slice(0, -1);
  }
  return normalized;
}

export function buildOrganizeDirectoryKey(physicalPath: string): string {
  const normalized = normalizeOrganizePath(physicalPath);
  return `dir-${fnv1a64(normalized)}`;
}

export function getPhysicalPath(
  sdPath: File["sd_path"] | null | undefined,
): string | null {
  if (!sdPath || !("Physical" in sdPath)) return null;
  return sdPath.Physical?.path ?? null;
}

export function getOrganizeItemKey(
  file: Pick<File, "id" | "sd_path" | "name" | "kind">,
): string {
  if (file.id) {
    return `id:${file.id}`;
  }
  const physicalPath = getPhysicalPath(file.sd_path);
  if (physicalPath) {
    return `path:${normalizeOrganizePath(physicalPath)}`;
  }
  return `fallback:${file.kind}:${file.name}`;
}

export function createEmptyOrganizeDirectoryState(
  directoryPath: string,
): OrganizeDirectoryState {
  return {
    version: 1,
    directoryPath,
    updatedAt: new Date().toISOString(),
    items: {},
  };
}
