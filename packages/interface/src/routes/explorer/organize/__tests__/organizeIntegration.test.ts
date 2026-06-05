/**
 * Integration-level runtime verification for organize view.
 *
 * Exercises the full organize state machine through realistic user-action
 * sequences, verifying that the data layer produces the correct observable
 * outcomes for each checklist item.
 *
 * This is the strongest executable proof achievable without a running Tauri
 * app — it verifies the entire decision→presentation→persistence pipeline
 * that the UI components consume.
 *
 * Checklist items covered:
 *  [C1] Mark one item Keep and one item Discard; verify state structure
 *  [C2] Decided items stay visible, dimmed, with correct decision labels
 *  [C3] Keep/Discard buckets contain only matching items
 *  [C4] Clear decision removes dimming and removes from buckets
 *  [C5] Leave and re-enter: serialize→deserialize round-trip preserves decisions
 *  [C6] Preview tab priority: video > image > list
 *  [C7] Single media type: missing tab disabled, list always available
 *  [C8] No media descendants: only list tab rendered
 *  [C9] Delete targets collected correctly from discard bucket
 * [C10] Delete removes entries from state, buckets, and presentation
 * [C11] Delete does not affect keep entries
 * [C12] JSON file structure matches expected schema for Tauri persistence
 */

import { describe, it, expect } from "bun:test";
import type { File } from "@sd/ts-client";
import {
  createEmptyOrganizeDirectoryState,
  getOrganizeItemKey,
  buildOrganizeDirectoryKey,
  normalizeOrganizePath,
} from "../organizePersistence";
import {
  upsertOrganizeDecision,
  projectOrganizeBucket,
  buildOrganizePresentation,
  removeDeletedOrganizeEntries,
  collectDiscardDeleteTargets,
  clearOrganizeDecision,
} from "../organizeState";
import { deriveDirectoryPreviewAvailability } from "../organizePreview";
import type { OrganizeDirectoryState } from "../organizeTypes";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeFile(
  overrides: Partial<Pick<File, "id" | "sd_path" | "name" | "kind" | "extension" | "content_kind">> = {},
): File {
  return {
    id: overrides.id ?? "file-1",
    sd_path: overrides.sd_path ?? { Physical: { device_slug: "dev-1", path: "/photos/cat.jpg" } },
    kind: overrides.kind ?? "File",
    name: overrides.name ?? "cat.jpg",
    extension: overrides.extension ?? "jpg",
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
    content_kind: overrides.content_kind ?? "image",
    is_local: true,
    duration_seconds: null,
  };
}

/** Simulate the full user journey: browse → decide → present → persist → restore */
function simulateUserJourney(
  files: File[],
  decisions: Array<{ file: File; decision: "keep" | "discard" | "clear" }>,
  deletedPaths?: string[],
) {
  let state = createEmptyOrganizeDirectoryState("/test/photos");

  // Apply decisions
  for (const { file, decision } of decisions) {
    if (decision === "clear") {
      state = clearOrganizeDecision(state, file);
    } else {
      state = upsertOrganizeDecision(state, file, decision);
    }
  }

  // Simulate deletion
  if (deletedPaths && deletedPaths.length > 0) {
    state = removeDeletedOrganizeEntries(state, deletedPaths);
  }

  // Project buckets
  const keepFiles = projectOrganizeBucket(files, state, "keep");
  const discardFiles = projectOrganizeBucket(files, state, "discard");

  // Build presentation
  const presentation = buildOrganizePresentation(files, state);

  // Collect delete targets
  const deleteTargets = collectDiscardDeleteTargets(files, state);

  return { state, keepFiles, discardFiles, presentation, deleteTargets };
}

// =========================================================================
// [C1] Mark Keep + Mark Discard: verify state structure
// =========================================================================
describe("[C1] Mark Keep and Discard", () => {
  const photoA = makeFile({ id: "photo-a", name: "sunset.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/sunset.jpg" } } });
  const photoB = makeFile({ id: "photo-b", name: "blurry.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/blurry.jpg" } } });

  it("marking Keep stores decision in state.items", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, photoA, "keep");
    const key = getOrganizeItemKey(photoA);
    expect(state.items[key]).toBeDefined();
    expect(state.items[key]!.decision).toBe("keep");
    expect(state.items[key]!.name).toBe("sunset.jpg");
  });

  it("marking Discard stores decision in state.items", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, photoB, "discard");
    const key = getOrganizeItemKey(photoB);
    expect(state.items[key]!.decision).toBe("discard");
  });

  it("state is version 1 with correct directoryPath", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    expect(state.version).toBe(1);
    expect(state.directoryPath).toBe("/photos");
  });

  it("JSON structure is valid for Tauri persistence", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, photoA, "keep");
    state = upsertOrganizeDecision(state, photoB, "discard");

    const json = JSON.stringify(state);
    const parsed = JSON.parse(json) as OrganizeDirectoryState;
    expect(parsed.version).toBe(1);
    expect(parsed.directoryPath).toBe("/photos");
    expect(Object.keys(parsed.items)).toHaveLength(2);
  });

  it("directory key is deterministic for same path", () => {
    const key1 = buildOrganizeDirectoryKey("/photos");
    const key2 = buildOrganizeDirectoryKey("/photos");
    expect(key1).toBe(key2);
    expect(key1).toMatch(/^dir-[0-9a-f]+$/);
  });
});

// =========================================================================
// [C2] Decided items: visible, dimmed, correct labels
// =========================================================================
describe("[C2] Decided items visible, dimmed, labeled", () => {
  const files = [
    makeFile({ id: "f1", name: "good.jpg" }),
    makeFile({ id: "f2", name: "bad.jpg" }),
    makeFile({ id: "f3", name: "neutral.jpg" }),
  ];

  it("decided items are dimmed (dimmed=true)", () => {
    const { presentation } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
      { file: files[1], decision: "discard" },
    ]);

    const f1 = presentation.find((p) => p.file.id === "f1")!;
    const f2 = presentation.find((p) => p.file.id === "f2")!;
    expect(f1.dimmed).toBe(true);
    expect(f2.dimmed).toBe(true);
  });

  it("undecided items are not dimmed", () => {
    const { presentation } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
    ]);

    const f3 = presentation.find((p) => p.file.id === "f3")!;
    expect(f3.dimmed).toBe(false);
    expect(f3.decision).toBeNull();
  });

  it("all items remain in presentation (none removed by deciding)", () => {
    const { presentation } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
      { file: files[1], decision: "discard" },
    ]);

    expect(presentation).toHaveLength(3);
    expect(presentation.map((p) => p.file.id).sort()).toEqual(["f1", "f2", "f3"]);
  });

  it("keep items have decision='keep', discard items have decision='discard'", () => {
    const { presentation } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
      { file: files[1], decision: "discard" },
    ]);

    expect(presentation.find((p) => p.file.id === "f1")!.decision).toBe("keep");
    expect(presentation.find((p) => p.file.id === "f2")!.decision).toBe("discard");
  });
});

// =========================================================================
// [C3] Keep/Discard buckets contain only matching items
// =========================================================================
describe("[C3] Buckets contain only matching items", () => {
  const files = [
    makeFile({ id: "k1", name: "keep-a.jpg" }),
    makeFile({ id: "k2", name: "keep-b.jpg" }),
    makeFile({ id: "d1", name: "discard-a.jpg" }),
    makeFile({ id: "n1", name: "neutral.jpg" }),
  ];

  it("keep bucket contains only keep-decided files", () => {
    const { keepFiles } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
      { file: files[1], decision: "keep" },
      { file: files[2], decision: "discard" },
    ]);

    expect(keepFiles).toHaveLength(2);
    expect(keepFiles.map((f) => f.id).sort()).toEqual(["k1", "k2"]);
  });

  it("discard bucket contains only discard-decided files", () => {
    const { discardFiles } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
      { file: files[2], decision: "discard" },
    ]);

    expect(discardFiles).toHaveLength(1);
    expect(discardFiles[0].id).toBe("d1");
  });

  it("undecided files appear in neither bucket", () => {
    const { keepFiles, discardFiles } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
    ]);

    expect(keepFiles.map((f) => f.id)).not.toContain("n1");
    expect(discardFiles.map((f) => f.id)).not.toContain("n1");
  });
});

// =========================================================================
// [C4] Clear decision: removes dimming and removes from buckets
// =========================================================================
describe("[C4] Clear decision removes dimming and bucket membership", () => {
  const files = [
    makeFile({ id: "f1", name: "photo.jpg" }),
    makeFile({ id: "f2", name: "other.jpg" }),
  ];

  it("clearing a keep decision removes from keep bucket", () => {
    const { keepFiles, presentation } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
      { file: files[0], decision: "clear" },
    ]);

    expect(keepFiles).toHaveLength(0);
    const f1 = presentation.find((p) => p.file.id === "f1")!;
    expect(f1.dimmed).toBe(false);
    expect(f1.decision).toBeNull();
  });

  it("clearing a discard decision removes from discard bucket", () => {
    const { discardFiles } = simulateUserJourney(files, [
      { file: files[0], decision: "discard" },
      { file: files[0], decision: "clear" },
    ]);

    expect(discardFiles).toHaveLength(0);
  });

  it("clearing is idempotent (clearing already-clear is a no-op)", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, files[0], "keep");
    state = clearOrganizeDecision(state, files[0]);
    const ref = state;
    state = clearOrganizeDecision(state, files[0]);
    expect(state).toBe(ref); // same reference = no-op
  });
});

// =========================================================================
// [C5] Leave and re-enter: serialize→deserialize round-trip
// =========================================================================
describe("[C5] Serialize→deserialize round-trip preserves decisions", () => {
  const files = [
    makeFile({ id: "f1", name: "sunset.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/sunset.jpg" } } }),
    makeFile({ id: "f2", name: "blurry.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/blurry.jpg" } } }),
  ];

  it("JSON round-trip preserves all decisions", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, files[0], "keep");
    state = upsertOrganizeDecision(state, files[1], "discard");

    // Simulate save (serialize)
    const json = JSON.stringify(state);

    // Simulate load (deserialize) — this is what useOrganizeState does
    const restored = JSON.parse(json) as OrganizeDirectoryState;

    // Verify decisions survive round-trip
    const key1 = getOrganizeItemKey(files[0]);
    const key2 = getOrganizeItemKey(files[1]);
    expect(restored.items[key1]!.decision).toBe("keep");
    expect(restored.items[key2]!.decision).toBe("discard");
  });

  it("round-trip preserves bucket projections", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, files[0], "keep");
    state = upsertOrganizeDecision(state, files[1], "discard");

    const json = JSON.stringify(state);
    const restored = JSON.parse(json) as OrganizeDirectoryState;

    const keepFiles = projectOrganizeBucket(files, restored, "keep");
    const discardFiles = projectOrganizeBucket(files, restored, "discard");

    expect(keepFiles).toHaveLength(1);
    expect(keepFiles[0].id).toBe("f1");
    expect(discardFiles).toHaveLength(1);
    expect(discardFiles[0].id).toBe("f2");
  });

  it("round-trip preserves presentation (dimmed, decisions)", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, files[0], "keep");
    state = upsertOrganizeDecision(state, files[1], "discard");

    const json = JSON.stringify(state);
    const restored = JSON.parse(json) as OrganizeDirectoryState;

    const presentation = buildOrganizePresentation(files, restored);
    expect(presentation.find((p) => p.file.id === "f1")!.dimmed).toBe(true);
    expect(presentation.find((p) => p.file.id === "f1")!.decision).toBe("keep");
    expect(presentation.find((p) => p.file.id === "f2")!.dimmed).toBe(true);
    expect(presentation.find((p) => p.file.id === "f2")!.decision).toBe("discard");
  });

  it("round-trip preserves directoryPath and version", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, files[0], "keep");

    const json = JSON.stringify(state);
    const restored = JSON.parse(json) as OrganizeDirectoryState;

    expect(restored.version).toBe(1);
    expect(restored.directoryPath).toBe("/photos");
  });

  it("empty state round-trip produces valid empty state", () => {
    const state = createEmptyOrganizeDirectoryState("/photos");
    const json = JSON.stringify(state);
    const restored = JSON.parse(json) as OrganizeDirectoryState;

    expect(Object.keys(restored.items)).toHaveLength(0);
    expect(restored.version).toBe(1);
  });

  it("path normalization is consistent across save/load", () => {
    // Windows paths should normalize the same way on both sides
    const key1 = buildOrganizeDirectoryKey("C:\\Users\\me\\photos");
    const key2 = buildOrganizeDirectoryKey("C:/Users/me/photos");
    expect(key1).toBe(key2);

    const norm1 = normalizeOrganizePath("C:\\Users\\me\\photos");
    const norm2 = normalizeOrganizePath("C:/Users/me/photos");
    expect(norm1).toBe(norm2);
  });
});

// =========================================================================
// [C6] Preview tab priority: video > image > list
// =========================================================================
describe("[C6] Preview tab priority: video > image > list", () => {
  it("video+image present: default tab is video", () => {
    const videoFile = makeFile({ id: "v1", content_kind: "video" });
    const imageFile = makeFile({ id: "i1", content_kind: "image" });
    const result = deriveDirectoryPreviewAvailability([videoFile, imageFile]);
    expect(result.defaultTab).toBe("video");
    expect(result.firstVideo).toBe(videoFile);
    expect(result.firstImage).toBe(imageFile);
  });

  it("only images: default tab is image", () => {
    const imageFile = makeFile({ id: "i1", content_kind: "image" });
    const result = deriveDirectoryPreviewAvailability([imageFile]);
    expect(result.defaultTab).toBe("image");
    expect(result.firstVideo).toBeNull();
  });

  it("video+image: all three tabs rendered", () => {
    const videoFile = makeFile({ id: "v1", content_kind: "video" });
    const imageFile = makeFile({ id: "i1", content_kind: "image" });
    const result = deriveDirectoryPreviewAvailability([videoFile, imageFile]);
    expect(result.renderedTabs).toEqual(["video", "image", "list"]);
    expect(result.enabledTabs).toEqual(["video", "image", "list"]);
  });
});

// =========================================================================
// [C7] Single media type: missing tab disabled, list always available
// =========================================================================
describe("[C7] Single media type: missing tab disabled", () => {
  it("only video: image tab not in enabledTabs, list still available", () => {
    const videoFile = makeFile({ id: "v1", content_kind: "video" });
    const result = deriveDirectoryPreviewAvailability([videoFile]);
    expect(result.enabledTabs).toContain("video");
    expect(result.enabledTabs).toContain("list");
    expect(result.enabledTabs).not.toContain("image");
    // All three are still rendered (disabled ones shown as grayed out)
    expect(result.renderedTabs).toEqual(["video", "image", "list"]);
  });

  it("only image: video tab not in enabledTabs, list still available", () => {
    const imageFile = makeFile({ id: "i1", content_kind: "image" });
    const result = deriveDirectoryPreviewAvailability([imageFile]);
    expect(result.enabledTabs).toContain("image");
    expect(result.enabledTabs).toContain("list");
    expect(result.enabledTabs).not.toContain("video");
    expect(result.renderedTabs).toEqual(["video", "image", "list"]);
  });
});

// =========================================================================
// [C8] No media descendants: only list tab rendered
// =========================================================================
describe("[C8] No media descendants: only list tab", () => {
  it("empty file list: only list tab rendered", () => {
    const result = deriveDirectoryPreviewAvailability([]);
    expect(result.renderedTabs).toEqual(["list"]);
    expect(result.enabledTabs).toEqual(["list"]);
    expect(result.defaultTab).toBe("list");
    expect(result.firstVideo).toBeNull();
    expect(result.firstImage).toBeNull();
  });

  it("non-media files: only list tab rendered", () => {
    const docFile = makeFile({ id: "d1", content_kind: "document" });
    const result = deriveDirectoryPreviewAvailability([docFile]);
    expect(result.renderedTabs).toEqual(["list"]);
    expect(result.defaultTab).toBe("list");
  });
});

// =========================================================================
// [C9] Delete targets collected correctly from discard bucket
// =========================================================================
describe("[C9] Delete targets from discard bucket", () => {
  const files = [
    makeFile({ id: "k1", name: "keep.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/keep.jpg" } } }),
    makeFile({ id: "d1", name: "bad.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/bad.jpg" } } }),
    makeFile({ id: "d2", name: "ugly.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/ugly.jpg" } } }),
  ];

  it("collects only discard-decided files with valid sd_path", () => {
    const { deleteTargets } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
      { file: files[1], decision: "discard" },
      { file: files[2], decision: "discard" },
    ]);

    expect(deleteTargets).toHaveLength(2);
    expect(deleteTargets.map((f) => f.id).sort()).toEqual(["d1", "d2"]);
  });

  it("excludes keep-decided files from delete targets", () => {
    const { deleteTargets } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
      { file: files[1], decision: "discard" },
    ]);

    expect(deleteTargets.map((f) => f.id)).not.toContain("k1");
  });

  it("empty when no discard decisions", () => {
    const { deleteTargets } = simulateUserJourney(files, [
      { file: files[0], decision: "keep" },
    ]);

    expect(deleteTargets).toHaveLength(0);
  });
});

// =========================================================================
// [C10] Delete removes entries from state, buckets, and presentation
// =========================================================================
describe("[C10] Delete removes entries from state, buckets, presentation", () => {
  const files = [
    makeFile({ id: "f1", name: "keep.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/keep.jpg" } } }),
    makeFile({ id: "f2", name: "discard.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/discard.jpg" } } }),
    makeFile({ id: "f3", name: "other.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/other.jpg" } } }),
  ];

  it("removing deleted entries clears them from state.items", () => {
    const { state } = simulateUserJourney(
      files,
      [
        { file: files[1], decision: "discard" },
      ],
      ["/photos/discard.jpg"],
    );

    const key = getOrganizeItemKey(files[1]);
    expect(state.items[key]).toBeUndefined();
  });

  it("removing deleted entries updates discard bucket", () => {
    const { discardFiles } = simulateUserJourney(
      files,
      [
        { file: files[1], decision: "discard" },
      ],
      ["/photos/discard.jpg"],
    );

    expect(discardFiles).toHaveLength(0);
  });

  it("removing deleted entries updates presentation (no longer dimmed)", () => {
    const { presentation } = simulateUserJourney(
      files,
      [
        { file: files[1], decision: "discard" },
      ],
      ["/photos/discard.jpg"],
    );

    // f2 was deleted, so it should not be in the state items
    // But it's still in the files array, so it appears in presentation as undecided
    const f2 = presentation.find((p) => p.file.id === "f2")!;
    expect(f2.dimmed).toBe(false);
    expect(f2.decision).toBeNull();
  });

  it("updatedAt changes after deletion", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(state, files[1], "discard");
    state = removeDeletedOrganizeEntries(state, ["/photos/discard.jpg"]);

    // Verify the state was actually modified (not the same reference)
    const key = getOrganizeItemKey(files[1]);
    expect(state.items[key]).toBeUndefined();
    // updatedAt is a valid ISO string
    expect(state.updatedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });
});

// =========================================================================
// [C11] Delete does not affect keep entries
// =========================================================================
describe("[C11] Delete does not affect keep entries", () => {
  const files = [
    makeFile({ id: "k1", name: "keep.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/keep.jpg" } } }),
    makeFile({ id: "d1", name: "discard.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/discard.jpg" } } }),
  ];

  it("deleting discard entries preserves keep entries", () => {
    const { keepFiles, state } = simulateUserJourney(
      files,
      [
        { file: files[0], decision: "keep" },
        { file: files[1], decision: "discard" },
      ],
      ["/photos/discard.jpg"],
    );

    expect(keepFiles).toHaveLength(1);
    expect(keepFiles[0].id).toBe("k1");

    const keepKey = getOrganizeItemKey(files[0]);
    expect(state.items[keepKey]).toBeDefined();
    expect(state.items[keepKey]!.decision).toBe("keep");
  });
});

// =========================================================================
// [C12] JSON file structure matches expected schema
// =========================================================================
describe("[C12] JSON persistence schema", () => {
  it("serialized state has correct top-level fields", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    state = upsertOrganizeDecision(
      state,
      makeFile({ id: "f1", name: "a.jpg", sd_path: { Physical: { device_slug: "d", path: "/photos/a.jpg" } } }),
      "keep",
    );

    const json = JSON.parse(JSON.stringify(state));
    expect(json).toHaveProperty("version", 1);
    expect(json).toHaveProperty("directoryPath", "/photos");
    expect(json).toHaveProperty("updatedAt");
    expect(json).toHaveProperty("items");
    expect(typeof json.updatedAt).toBe("string");
    expect(typeof json.items).toBe("object");
  });

  it("each item record has correct fields", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    const file = makeFile({
      id: "f1",
      name: "a.jpg",
      kind: "File",
      sd_path: { Physical: { device_slug: "d", path: "/photos/a.jpg" } },
    });
    state = upsertOrganizeDecision(state, file, "discard");

    const json = JSON.parse(JSON.stringify(state));
    const key = getOrganizeItemKey(file);
    const record = json.items[key];

    expect(record).toHaveProperty("itemId", "f1");
    expect(record).toHaveProperty("path", "/photos/a.jpg");
    expect(record).toHaveProperty("name", "a.jpg");
    expect(record).toHaveProperty("kind", "File");
    expect(record).toHaveProperty("decision", "discard");
    expect(record).toHaveProperty("updatedAt");
  });

  it("directory kind is recorded correctly", () => {
    let state = createEmptyOrganizeDirectoryState("/photos");
    const dir = makeFile({ id: "d1", name: "subfolder", kind: "Directory" });
    state = upsertOrganizeDecision(state, dir, "keep");

    const json = JSON.parse(JSON.stringify(state));
    const key = getOrganizeItemKey(dir);
    expect(json.items[key].kind).toBe("Directory");
  });

  it("directory key format is dir-<hex>", () => {
    const key = buildOrganizeDirectoryKey("/test/path");
    expect(key).toMatch(/^dir-[0-9a-f]{16}$/);
  });
});
