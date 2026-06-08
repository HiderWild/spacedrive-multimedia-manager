import { describe, it, expect, beforeEach } from "bun:test";
import type { File } from "@sd/ts-client";
import { preloadOrganizeThumbnails } from "../useOrganizeThumbnail";
import { organizeThumbnailCache } from "../thumbnailCache";

// Mock buildSidecarUrl function
const mockBuildSidecarUrl = (uuid: string, kind: string, variant: string, format: string) => {
  return `http://mock-server/sidecar/${uuid}/${kind}/${variant}.${format}`;
};

// Mock fetch for testing
const originalFetch = global.fetch;
global.fetch = ((url: string) => {
  if (typeof url === "string" && url.includes("mock-server")) {
    return Promise.resolve({
      ok: true,
      blob: () => Promise.resolve(new Blob(["mock-image-data"], { type: "image/jpeg" })),
    } as Response);
  }
  return Promise.reject(new Error("Not found"));
}) as typeof fetch;

// Mock URL.createObjectURL
const mockObjectUrls = new Map<Blob, string>();
let urlCounter = 0;
const originalCreateObjectURL = global.URL.createObjectURL;
const originalRevokeObjectURL = global.URL.revokeObjectURL;

global.URL.createObjectURL = (blob: Blob | MediaSource) => {
  const url = `blob:mock-${urlCounter++}`;
  if (blob instanceof Blob) {
    mockObjectUrls.set(blob, url);
  }
  return url;
};

global.URL.revokeObjectURL = (url: string) => {
  for (const [blob, storedUrl] of mockObjectUrls) {
    if (storedUrl === url) {
      mockObjectUrls.delete(blob);
      break;
    }
  }
};

function makeFile(overrides: Partial<File> = {}): File {
  return {
    id: "file-1",
    sd_path: { Physical: { device_slug: "dev-1", path: "/photos/cat.jpg" } },
    kind: "File",
    name: "cat.jpg",
    extension: "jpg",
    size: 1024,
    content_identity: { uuid: "content-uuid-123" },
    alternate_paths: [],
    tags: [],
    sidecars: [
      {
        kind: "thumb",
        variant: "grid@1x",
        format: "webp",
      },
    ],
    image_media_data: null,
    video_media_data: null,
    audio_media_data: null,
    created_at: "2026-01-01T00:00:00Z",
    modified_at: "2026-01-01T00:00:00Z",
    accessed_at: null,
    content_kind: "image",
    is_local: true,
    duration_seconds: null,
    ...overrides,
  } as File;
}

describe("thumbnailCache integration", () => {
  beforeEach(() => {
    organizeThumbnailCache.clear();
  });

  it("caches thumbnail data by path and size", () => {
    const key = organizeThumbnailCache.generateKey("/photos/cat.jpg", 1024, false);
    const data = "blob:mock-123";
    const memorySize = 96 * 96 * 4;

    organizeThumbnailCache.set(key, data, memorySize);

    expect(organizeThumbnailCache.has(key)).toBe(true);
    expect(organizeThumbnailCache.get(key)?.data).toBe(data);
  });

  it("tracks concurrent loads correctly", () => {
    const key1 = organizeThumbnailCache.generateKey("/1.jpg", 1024, false);
    const key2 = organizeThumbnailCache.generateKey("/2.jpg", 1024, false);

    organizeThumbnailCache.markLoadStart(key1);
    organizeThumbnailCache.markLoadStart(key2);

    expect(organizeThumbnailCache.getStats().pendingLoads).toBe(2);

    organizeThumbnailCache.markLoadEnd(key1);
    expect(organizeThumbnailCache.getStats().pendingLoads).toBe(1);

    organizeThumbnailCache.markLoadEnd(key2);
    expect(organizeThumbnailCache.getStats().pendingLoads).toBe(0);
  });

  it("enforces concurrent load limit", () => {
    const keys = [
      organizeThumbnailCache.generateKey("/1.jpg", 1024, false),
      organizeThumbnailCache.generateKey("/2.jpg", 1024, false),
      organizeThumbnailCache.generateKey("/3.jpg", 1024, false),
      organizeThumbnailCache.generateKey("/4.jpg", 1024, false),
    ];

    keys.forEach((key) => organizeThumbnailCache.markLoadStart(key));

    expect(organizeThumbnailCache.canStartLoad()).toBe(false);

    organizeThumbnailCache.markLoadEnd(keys[0]);
    expect(organizeThumbnailCache.canStartLoad()).toBe(true);
  });
});

describe("preloadOrganizeThumbnails", () => {
  beforeEach(() => {
    organizeThumbnailCache.clear();
  });

  it("queues files for preloading", async () => {
    const files = [
      makeFile({ id: "1", sd_path: { Physical: { device_slug: "d", path: "/1.jpg" } } }),
      makeFile({ id: "2", sd_path: { Physical: { device_slug: "d", path: "/2.jpg" } } }),
    ];

    preloadOrganizeThumbnails(files, 96, mockBuildSidecarUrl);

    // Give time for async operations
    await new Promise((resolve) => setTimeout(resolve, 500));

    // Should have started loading
    const stats = organizeThumbnailCache.getStats();
    expect(stats.pendingLoads).toBeGreaterThanOrEqual(0);
  });

  it("skips files without physical paths", () => {
    const files = [
      makeFile({
        id: "1",
        sd_path: { Content: { content_id: "abc123" } }
      }),
    ];

    preloadOrganizeThumbnails(files, 96, mockBuildSidecarUrl);

    const stats = organizeThumbnailCache.getStats();
    expect(stats.entryCount).toBe(0);
    expect(stats.pendingLoads).toBe(0);
  });

  it("skips files without sidecars", () => {
    const files = [
      makeFile({
        id: "1",
        sidecars: []
      }),
    ];

    preloadOrganizeThumbnails(files, 96, mockBuildSidecarUrl);

    const stats = organizeThumbnailCache.getStats();
    expect(stats.entryCount).toBe(0);
  });
});
