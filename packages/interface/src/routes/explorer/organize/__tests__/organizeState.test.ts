import { describe, it, expect } from "bun:test";
import type { File } from "@sd/ts-client";
import {
  normalizeOrganizePath,
  buildOrganizeDirectoryKey,
  getPhysicalPath,
  getOrganizeItemKey,
  createEmptyOrganizeDirectoryState,
} from "../organizePersistence";
import {
  upsertOrganizeDecision,
  projectOrganizeBucket,
  buildOrganizePresentation,
  removeDeletedOrganizeEntries,
  clearOrganizeDecision,
} from "../organizeState";
import type { OrganizeDecision } from "../organizeTypes";

function makeFile(overrides: Partial<Pick<File, "id" | "sd_path" | "name" | "kind">> = {}): File {
  return {
    id: overrides.id ?? "file-1",
    sd_path: overrides.sd_path ?? { Physical: { device_slug: "dev-1", path: "/photos/cat.jpg" } },
    kind: overrides.kind ?? "File",
    name: overrides.name ?? "cat.jpg",
    extension: "jpg",
    size: 1024,
    content_identity: null,
    alternate_paths: [],
    tags: [],
    sidecars: [],
    image_media_data: null,
    video_media_data: null,
    audio_media_data: null,
    created_at: "2026-01-01T00:00:00Z",
    modified_at: "2026-01-01T00:00:00Z",
    accessed_at: null,
    content_kind: "image",
    is_local: true,
    duration_seconds: null,
  };
}

describe("normalizeOrganizePath", () => {
  it("normalizes backslashes to forward slashes", () => {
    expect(normalizeOrganizePath("C:\\Users\\me\\photos")).toBe("C:/Users/me/photos");
  });

  it("collapses repeated slashes", () => {
    expect(normalizeOrganizePath("/photos///cat.jpg")).toBe("/photos/cat.jpg");
  });

  it("strips trailing slash except root", () => {
    expect(normalizeOrganizePath("/photos/")).toBe("/photos");
    expect(normalizeOrganizePath("/")).toBe("/");
  });

  it("normalizes equivalent paths to same result", () => {
    expect(normalizeOrganizePath("/photos/cat.jpg")).toBe(normalizeOrganizePath("/photos//cat.jpg"));
    expect(normalizeOrganizePath("C:\\Users\\me")).toBe(normalizeOrganizePath("C:/Users/me"));
  });
});

describe("buildOrganizeDirectoryKey", () => {
  it("generates a stable dir-<hex> key from normalized path", () => {
    const key1 = buildOrganizeDirectoryKey("/photos");
    const key2 = buildOrganizeDirectoryKey("/photos");
    expect(key1).toBe(key2);
    expect(key1).toMatch(/^dir-[0-9a-f]+$/);
  });

  it("different paths produce different keys", () => {
    const key1 = buildOrganizeDirectoryKey("/photos");
    const key2 = buildOrganizeDirectoryKey("/videos");
    expect(key1).not.toBe(key2);
  });

  it("equivalent paths normalize to same key", () => {
    const key1 = buildOrganizeDirectoryKey("/photos//");
    const key2 = buildOrganizeDirectoryKey("/photos");
    expect(key1).toBe(key2);
  });
});

describe("getPhysicalPath", () => {
  it("extracts physical path from Physical SdPath", () => {
    const sdPath = { Physical: { device_slug: "dev-1", path: "/photos/cat.jpg" } };
    expect(getPhysicalPath(sdPath)).toBe("/photos/cat.jpg");
  });

  it("returns null for Content SdPath", () => {
    const sdPath = { Content: { content_id: "abc123" } };
    expect(getPhysicalPath(sdPath)).toBeNull();
  });

  it("returns null for null/undefined", () => {
    expect(getPhysicalPath(null)).toBeNull();
    expect(getPhysicalPath(undefined)).toBeNull();
  });
});

describe("getOrganizeItemKey", () => {
  it("uses id: prefix when file.id exists", () => {
    const file = makeFile({ id: "file-42" });
    expect(getOrganizeItemKey(file)).toBe("id:file-42");
  });

  it("uses path: prefix when file.id is empty but physical path exists", () => {
    const file = makeFile({ id: "", sd_path: { Physical: { device_slug: "d", path: "/a/b.txt" } } });
    expect(getOrganizeItemKey(file)).toBe("path:/a/b.txt");
  });

  it("normalizes equivalent paths to same key via path: prefix", () => {
    const file1 = makeFile({ id: "", sd_path: { Physical: { device_slug: "d", path: "/a//b.txt" } } });
    const file2 = makeFile({ id: "", sd_path: { Physical: { device_slug: "d", path: "/a/b.txt" } } });
    expect(getOrganizeItemKey(file1)).toBe(getOrganizeItemKey(file2));
  });

  it("uses fallback: prefix when no id and no physical path", () => {
    const file = makeFile({ id: "", sd_path: { Content: { content_id: "abc" } } });
    expect(getOrganizeItemKey(file)).toBe("fallback:File:cat.jpg");
  });
});

describe("createEmptyOrganizeDirectoryState", () => {
  it("creates state with version 1 and empty items", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    expect(state).toEqual({
      version: 1,
      directoryPath: "/photos",
      updatedAt: expect.any(String),
      items: {},
    });
  });
});

describe("upsertOrganizeDecision", () => {
  it("stores a full item record in state.items", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file = makeFile({ id: "file-1", sd_path: { Physical: { device_slug: "d", path: "/photos/a.jpg" } } });
    const updated = upsertOrganizeDecision(state, file, "keep");

    const key = getOrganizeItemKey(file);
    expect(updated.items[key]).toEqual({
      itemId: "file-1",
      path: "/photos/a.jpg",
      name: "cat.jpg",
      kind: "File",
      decision: "keep",
      updatedAt: expect.any(String),
    });
  });

  it("overwrites an existing decision", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file = makeFile({ id: "file-1" });
    let updated = upsertOrganizeDecision(state, file, "keep");
    updated = upsertOrganizeDecision(updated, file, "discard");

    const key = getOrganizeItemKey(file);
    expect(updated.items[key]!.decision).toBe("discard");
  });

  it("returns a new state object (immutability)", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file = makeFile({ id: "file-1" });
    const updated = upsertOrganizeDecision(state, file, "keep");
    expect(updated).not.toBe(state);
  });
});

describe("projectOrganizeBucket", () => {
  it("returns files matching the given decision via persisted state", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file1 = makeFile({ id: "file-1" });
    const file2 = makeFile({ id: "file-2" });
    const file3 = makeFile({ id: "file-3" });
    let updated = upsertOrganizeDecision(state, file1, "keep");
    updated = upsertOrganizeDecision(updated, file2, "discard");
    updated = upsertOrganizeDecision(updated, file3, "keep");

    const kept = projectOrganizeBucket([file1, file2, file3], updated, "keep");
    expect(kept).toHaveLength(2);
    expect(kept.map((f) => f.id)).toEqual(["file-1", "file-3"]);
  });

  it("returns files matching discard decision", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file1 = makeFile({ id: "file-1" });
    const file2 = makeFile({ id: "file-2" });
    let updated = upsertOrganizeDecision(state, file1, "keep");
    updated = upsertOrganizeDecision(updated, file2, "discard");

    const discarded = projectOrganizeBucket([file1, file2], updated, "discard");
    expect(discarded).toHaveLength(1);
    expect(discarded[0].id).toBe("file-2");
  });

  it("returns empty array when no files match", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file1 = makeFile({ id: "file-1" });
    const updated = upsertOrganizeDecision(state, file1, "keep");

    const discarded = projectOrganizeBucket([file1], updated, "discard");
    expect(discarded).toHaveLength(0);
  });
});

describe("buildOrganizePresentation", () => {
  it("returns array of { file, decision, dimmed } entries", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file1 = makeFile({ id: "file-1" });
    const file2 = makeFile({ id: "file-2" });
    let updated = upsertOrganizeDecision(state, file1, "keep");
    updated = upsertOrganizeDecision(updated, file2, "discard");

    const presentation = buildOrganizePresentation([file1, file2], updated);
    expect(presentation).toEqual([
      { file: file1, decision: "keep", dimmed: true },
      { file: file2, decision: "discard", dimmed: true },
    ]);
  });

  it("files without decisions have decision null and dimmed false", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file1 = makeFile({ id: "file-1" });

    const presentation = buildOrganizePresentation([file1], state);
    expect(presentation).toEqual([
      { file: file1, decision: null, dimmed: false },
    ]);
  });

  it("keeps decided items in the center list while only matching decisions appear in left buckets", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(
      state,
      makeFile({ id: "keep-1", sd_path: { Physical: { device_slug: "disk", path: "/photos/keep.mp4" } } }),
      "keep",
    );
    const files = [
      makeFile({ id: "keep-1", sd_path: { Physical: { device_slug: "disk", path: "/photos/keep.mp4" } } }),
      makeFile({ id: "fresh-1", sd_path: { Physical: { device_slug: "disk", path: "/photos/fresh.mp4" } } }),
    ];
    const presentation = buildOrganizePresentation(files, state);
    expect(presentation.find((item) => item.file.id === "keep-1")).toMatchObject({ decision: "keep", dimmed: true });
    expect(presentation.find((item) => item.file.id === "fresh-1")).toMatchObject({ decision: null, dimmed: false });
  });
});

describe("removeDeletedOrganizeEntries", () => {
  it("removes records whose persisted path matches deleted paths", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file1 = makeFile({ id: "file-1", sd_path: { Physical: { device_slug: "d", path: "/photos/a.jpg" } } });
    const file2 = makeFile({ id: "file-2", sd_path: { Physical: { device_slug: "d", path: "/photos/b.jpg" } } });
    let updated = upsertOrganizeDecision(state, file1, "keep");
    updated = upsertOrganizeDecision(updated, file2, "discard");

    const cleaned = removeDeletedOrganizeEntries(updated, ["/photos/a.jpg"]);
    const key1 = getOrganizeItemKey(file1);
    const key2 = getOrganizeItemKey(file2);
    expect(cleaned.items[key1]).toBeUndefined();
    expect(cleaned.items[key2]).toBeDefined();
  });

  it("returns same object when nothing deleted", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file1 = makeFile({ id: "file-1" });
    const updated = upsertOrganizeDecision(state, file1, "keep");

    const cleaned = removeDeletedOrganizeEntries(updated, []);
    expect(cleaned).toBe(updated);
  });

  it("returns same object when no matching deletions", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file1 = makeFile({ id: "file-1", sd_path: { Physical: { device_slug: "d", path: "/photos/a.jpg" } } });
    const updated = upsertOrganizeDecision(state, file1, "keep");

    const cleaned = removeDeletedOrganizeEntries(updated, ["/nonexistent/path"]);
    expect(cleaned).toBe(updated);
  });
});

describe("clearOrganizeDecision", () => {
  it("removes a decided item from state", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file = makeFile({ id: "file-1" });
    const decided = upsertOrganizeDecision(state, file, "keep");

    const cleared = clearOrganizeDecision(decided, file);
    const key = getOrganizeItemKey(file);
    expect(cleared.items[key]).toBeUndefined();
  });

  it("returns same reference when key not in state", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file = makeFile({ id: "file-1" });

    const result = clearOrganizeDecision(state, file);
    expect(result).toBe(state);
  });

  it("produces a new state object on clear", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const file = makeFile({ id: "file-1" });
    const decided = upsertOrganizeDecision(state, file, "discard");

    const cleared = clearOrganizeDecision(decided, file);
    expect(cleared).not.toBe(decided);
    expect(Object.keys(cleared.items)).toHaveLength(0);
  });
});
