import { describe, it, expect, beforeEach } from "bun:test";
import {
  ThumbnailCache,
  createThumbnailCache,
  type ThumbnailCacheKey,
  type ThumbnailCacheEntry,
} from "../thumbnailCache";

describe("ThumbnailCache", () => {
  let cache: ThumbnailCache;

  beforeEach(() => {
    cache = createThumbnailCache({
      maxMemoryBytes: 8 * 1024 * 1024 * 1024, // 8GB
      maxConcurrentLoads: 4,
    });
  });

  describe("cache key generation", () => {
    it("generates consistent keys from materialized_path + size_bytes", () => {
      const key1 = cache.generateKey("/photos/cat.jpg", 1024);
      const key2 = cache.generateKey("/photos/cat.jpg", 1024);
      expect(key1).toBe(key2);
    });

    it("generates different keys for different paths", () => {
      const key1 = cache.generateKey("/photos/cat.jpg", 1024);
      const key2 = cache.generateKey("/photos/dog.jpg", 1024);
      expect(key1).not.toBe(key2);
    });

    it("generates different keys for different sizes", () => {
      const key1 = cache.generateKey("/photos/cat.jpg", 1024);
      const key2 = cache.generateKey("/photos/cat.jpg", 2048);
      expect(key1).not.toBe(key2);
    });
  });

  describe("cache operations", () => {
    it("stores and retrieves thumbnail data", () => {
      const key = cache.generateKey("/photos/cat.jpg", 1024);
      const data = "data:image/png;base64,iVBORw0KGg...";
      const memorySize = 1024 * 1024; // 1MB

      cache.set(key, data, memorySize);
      const entry = cache.get(key);

      expect(entry).toBeDefined();
      expect(entry?.data).toBe(data);
      expect(entry?.memorySize).toBe(memorySize);
    });

    it("returns undefined for missing keys", () => {
      const key = cache.generateKey("/photos/missing.jpg", 1024);
      expect(cache.get(key)).toBeUndefined();
    });

    it("overwrites existing entries", () => {
      const key = cache.generateKey("/photos/cat.jpg", 1024);
      cache.set(key, "data1", 1024);
      cache.set(key, "data2", 2048);

      const entry = cache.get(key);
      expect(entry?.data).toBe("data2");
      expect(entry?.memorySize).toBe(2048);
    });
  });

  describe("FIFO eviction", () => {
    it("evicts oldest entry when memory limit exceeded", () => {
      const smallCache = createThumbnailCache({
        maxMemoryBytes: 3 * 1024 * 1024, // 3MB
        maxConcurrentLoads: 4,
      });

      const key1 = smallCache.generateKey("/photos/1.jpg", 1024);
      const key2 = smallCache.generateKey("/photos/2.jpg", 1024);
      const key3 = smallCache.generateKey("/photos/3.jpg", 1024);

      smallCache.set(key1, "data1", 1.5 * 1024 * 1024); // 1.5MB
      smallCache.set(key2, "data2", 1.5 * 1024 * 1024); // 1.5MB (total 3MB)
      smallCache.set(key3, "data3", 1.5 * 1024 * 1024); // 1.5MB (should evict key1)

      expect(smallCache.get(key1)).toBeUndefined(); // Evicted
      expect(smallCache.get(key2)).toBeDefined();
      expect(smallCache.get(key3)).toBeDefined();
    });

    it("evicts multiple entries if needed for large item", () => {
      const smallCache = createThumbnailCache({
        maxMemoryBytes: 4 * 1024 * 1024, // 4MB
        maxConcurrentLoads: 4,
      });

      const key1 = smallCache.generateKey("/photos/1.jpg", 1024);
      const key2 = smallCache.generateKey("/photos/2.jpg", 1024);
      const key3 = smallCache.generateKey("/photos/3.jpg", 1024);

      smallCache.set(key1, "data1", 1.5 * 1024 * 1024); // 1.5MB
      smallCache.set(key2, "data2", 1.5 * 1024 * 1024); // 1.5MB (total 3MB)
      smallCache.set(key3, "data3", 3 * 1024 * 1024); // 3MB (should evict key1 and key2)

      expect(smallCache.get(key1)).toBeUndefined();
      expect(smallCache.get(key2)).toBeUndefined();
      expect(smallCache.get(key3)).toBeDefined();
    });

    it("maintains FIFO order across multiple insertions", () => {
      const smallCache = createThumbnailCache({
        maxMemoryBytes: 4 * 1024 * 1024, // 4MB
        maxConcurrentLoads: 4,
      });

      const keys = [
        smallCache.generateKey("/photos/1.jpg", 1024),
        smallCache.generateKey("/photos/2.jpg", 1024),
        smallCache.generateKey("/photos/3.jpg", 1024),
        smallCache.generateKey("/photos/4.jpg", 1024),
        smallCache.generateKey("/photos/5.jpg", 1024),
      ];

      // Add 4 items at 1MB each
      for (let i = 0; i < 4; i++) {
        smallCache.set(keys[i], `data${i}`, 1 * 1024 * 1024);
      }

      // Add 5th item, should evict first
      smallCache.set(keys[4], "data4", 1 * 1024 * 1024);

      expect(smallCache.get(keys[0])).toBeUndefined();
      expect(smallCache.get(keys[1])).toBeDefined();
      expect(smallCache.get(keys[2])).toBeDefined();
      expect(smallCache.get(keys[3])).toBeDefined();
      expect(smallCache.get(keys[4])).toBeDefined();
    });
  });

  describe("memory tracking", () => {
    it("tracks total memory usage", () => {
      const key1 = cache.generateKey("/photos/1.jpg", 1024);
      const key2 = cache.generateKey("/photos/2.jpg", 1024);

      cache.set(key1, "data1", 1 * 1024 * 1024); // 1MB
      cache.set(key2, "data2", 2 * 1024 * 1024); // 2MB

      const stats = cache.getStats();
      expect(stats.totalMemoryBytes).toBe(3 * 1024 * 1024);
      expect(stats.entryCount).toBe(2);
    });

    it("updates memory after eviction", () => {
      const smallCache = createThumbnailCache({
        maxMemoryBytes: 2 * 1024 * 1024, // 2MB
        maxConcurrentLoads: 4,
      });

      const key1 = smallCache.generateKey("/photos/1.jpg", 1024);
      const key2 = smallCache.generateKey("/photos/2.jpg", 1024);

      smallCache.set(key1, "data1", 1.5 * 1024 * 1024); // 1.5MB
      smallCache.set(key2, "data2", 1.5 * 1024 * 1024); // 1.5MB (evicts key1)

      const stats = smallCache.getStats();
      expect(stats.totalMemoryBytes).toBe(1.5 * 1024 * 1024);
      expect(stats.entryCount).toBe(1);
    });
  });

  describe("clear cache", () => {
    it("removes all entries", () => {
      const key1 = cache.generateKey("/photos/1.jpg", 1024);
      const key2 = cache.generateKey("/photos/2.jpg", 1024);

      cache.set(key1, "data1", 1024);
      cache.set(key2, "data2", 2048);

      cache.clear();

      expect(cache.get(key1)).toBeUndefined();
      expect(cache.get(key2)).toBeUndefined();

      const stats = cache.getStats();
      expect(stats.totalMemoryBytes).toBe(0);
      expect(stats.entryCount).toBe(0);
    });
  });

  describe("has method", () => {
    it("returns true for existing keys", () => {
      const key = cache.generateKey("/photos/cat.jpg", 1024);
      cache.set(key, "data", 1024);
      expect(cache.has(key)).toBe(true);
    });

    it("returns false for missing keys", () => {
      const key = cache.generateKey("/photos/missing.jpg", 1024);
      expect(cache.has(key)).toBe(false);
    });
  });

  describe("delete method", () => {
    it("removes specific entry", () => {
      const key1 = cache.generateKey("/photos/1.jpg", 1024);
      const key2 = cache.generateKey("/photos/2.jpg", 1024);

      cache.set(key1, "data1", 1 * 1024 * 1024);
      cache.set(key2, "data2", 2 * 1024 * 1024);

      const deleted = cache.delete(key1);

      expect(deleted).toBe(true);
      expect(cache.get(key1)).toBeUndefined();
      expect(cache.get(key2)).toBeDefined();

      const stats = cache.getStats();
      expect(stats.totalMemoryBytes).toBe(2 * 1024 * 1024);
      expect(stats.entryCount).toBe(1);
    });

    it("returns false for non-existent keys", () => {
      const key = cache.generateKey("/photos/missing.jpg", 1024);
      expect(cache.delete(key)).toBe(false);
    });
  });

  describe("concurrent load limiting", () => {
    it("tracks pending loads", async () => {
      const key = cache.generateKey("/photos/cat.jpg", 1024);

      cache.markLoadStart(key);
      expect(cache.isLoading(key)).toBe(true);
      expect(cache.getStats().pendingLoads).toBe(1);

      cache.markLoadEnd(key);
      expect(cache.isLoading(key)).toBe(false);
      expect(cache.getStats().pendingLoads).toBe(0);
    });

    it("limits concurrent loads to maxConcurrentLoads", () => {
      const smallCache = createThumbnailCache({
        maxMemoryBytes: 8 * 1024 * 1024 * 1024,
        maxConcurrentLoads: 2,
      });

      const key1 = smallCache.generateKey("/photos/1.jpg", 1024);
      const key2 = smallCache.generateKey("/photos/2.jpg", 1024);
      const key3 = smallCache.generateKey("/photos/3.jpg", 1024);

      smallCache.markLoadStart(key1);
      smallCache.markLoadStart(key2);

      expect(smallCache.canStartLoad()).toBe(false);

      smallCache.markLoadEnd(key1);
      expect(smallCache.canStartLoad()).toBe(true);
    });
  });

  describe("directory thumbnail support", () => {
    it("stores directory thumbnails with special path handling", () => {
      const dirKey = cache.generateKey("/photos/vacation", 0, true);
      const data = "data:image/png;base64,composite...";

      cache.set(dirKey, data, 2 * 1024 * 1024);
      const entry = cache.get(dirKey);

      expect(entry).toBeDefined();
      expect(entry?.data).toBe(data);
    });

    it("differentiates between file and directory paths", () => {
      const fileKey = cache.generateKey("/photos/vacation.jpg", 1024, false);
      const dirKey = cache.generateKey("/photos/vacation", 1024, true);

      expect(fileKey).not.toBe(dirKey);
    });
  });
});
