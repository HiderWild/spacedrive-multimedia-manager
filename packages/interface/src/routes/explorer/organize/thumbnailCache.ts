/**
 * In-memory thumbnail cache for organize feature
 *
 * Features:
 * - Cache key: materialized_path + size_bytes + isDirectory flag
 * - 4 concurrent loads (configurable)
 * - 8GB memory limit (configurable)
 * - FIFO eviction when limit exceeded
 * - Memory-only, destroyed on process exit
 * - Supports directory thumbnails (album composite images)
 */

export type ThumbnailCacheKey = string;

export interface ThumbnailCacheEntry {
  data: string; // Base64 data URL or blob URL
  memorySize: number; // Estimated decoded bitmap memory in bytes
  timestamp: number; // For FIFO ordering
}

export interface ThumbnailCacheConfig {
  maxMemoryBytes: number;
  maxConcurrentLoads: number;
}

export interface ThumbnailCacheStats {
  totalMemoryBytes: number;
  entryCount: number;
  pendingLoads: number;
  maxMemoryBytes: number;
  maxConcurrentLoads: number;
}

export interface ThumbnailCache {
  generateKey(path: string, sizeBytes: number, isDirectory?: boolean): ThumbnailCacheKey;
  get(key: ThumbnailCacheKey): ThumbnailCacheEntry | undefined;
  set(key: ThumbnailCacheKey, data: string, memorySize: number): void;
  has(key: ThumbnailCacheKey): boolean;
  delete(key: ThumbnailCacheKey): boolean;
  clear(): void;
  getStats(): ThumbnailCacheStats;

  // Concurrent load tracking
  markLoadStart(key: ThumbnailCacheKey): void;
  markLoadEnd(key: ThumbnailCacheKey): void;
  isLoading(key: ThumbnailCacheKey): boolean;
  canStartLoad(): boolean;
}

/**
 * Create a new in-memory thumbnail cache
 */
export function createThumbnailCache(config: ThumbnailCacheConfig): ThumbnailCache {
  // Cache storage: Map maintains insertion order for FIFO
  const cache = new Map<ThumbnailCacheKey, ThumbnailCacheEntry>();

  // Memory tracking
  let totalMemoryBytes = 0;

  // Concurrent load tracking
  const loadingKeys = new Set<ThumbnailCacheKey>();

  /**
   * Generate cache key from path, size, and optional directory flag
   */
  function generateKey(path: string, sizeBytes: number, isDirectory = false): ThumbnailCacheKey {
    // Normalize path to handle Windows/Unix differences
    const normalizedPath = path.replace(/\\/g, '/');
    const dirPrefix = isDirectory ? 'dir:' : 'file:';
    return `${dirPrefix}${normalizedPath}@${sizeBytes}`;
  }

  /**
   * Evict entries until there's enough space (FIFO order)
   */
  function evictToFit(requiredBytes: number): void {
    // If the new item is larger than max capacity, don't try to evict
    if (requiredBytes > config.maxMemoryBytes) {
      return;
    }

    // Keep evicting oldest entries until we have enough space for the new item
    while (totalMemoryBytes + requiredBytes > config.maxMemoryBytes && cache.size > 0) {
      // Get first (oldest) entry
      const firstKey = cache.keys().next().value;
      if (!firstKey) break;

      const entry = cache.get(firstKey);
      if (entry) {
        cache.delete(firstKey);
        totalMemoryBytes -= entry.memorySize;
      }
    }
  }

  /**
   * Get cached thumbnail
   */
  function get(key: ThumbnailCacheKey): ThumbnailCacheEntry | undefined {
    return cache.get(key);
  }

  /**
   * Store thumbnail in cache with FIFO eviction
   */
  function set(key: ThumbnailCacheKey, data: string, memorySize: number): void {
    // If updating existing entry, remove its memory footprint first
    const existing = cache.get(key);
    if (existing) {
      totalMemoryBytes -= existing.memorySize;
      cache.delete(key);
    }

    // Evict old entries if needed
    if (totalMemoryBytes + memorySize > config.maxMemoryBytes) {
      evictToFit(memorySize);
    }

    // Add new entry
    const entry: ThumbnailCacheEntry = {
      data,
      memorySize,
      timestamp: Date.now(),
    };

    cache.set(key, entry);
    totalMemoryBytes += memorySize;
  }

  /**
   * Check if key exists in cache
   */
  function has(key: ThumbnailCacheKey): boolean {
    return cache.has(key);
  }

  /**
   * Delete specific entry from cache
   */
  function deleteEntry(key: ThumbnailCacheKey): boolean {
    const entry = cache.get(key);
    if (!entry) {
      return false;
    }

    cache.delete(key);
    totalMemoryBytes -= entry.memorySize;
    return true;
  }

  /**
   * Clear all entries from cache
   */
  function clear(): void {
    cache.clear();
    totalMemoryBytes = 0;
    loadingKeys.clear();
  }

  /**
   * Get cache statistics
   */
  function getStats(): ThumbnailCacheStats {
    return {
      totalMemoryBytes,
      entryCount: cache.size,
      pendingLoads: loadingKeys.size,
      maxMemoryBytes: config.maxMemoryBytes,
      maxConcurrentLoads: config.maxConcurrentLoads,
    };
  }

  /**
   * Mark a thumbnail load as started
   */
  function markLoadStart(key: ThumbnailCacheKey): void {
    loadingKeys.add(key);
  }

  /**
   * Mark a thumbnail load as completed
   */
  function markLoadEnd(key: ThumbnailCacheKey): void {
    loadingKeys.delete(key);
  }

  /**
   * Check if a thumbnail is currently loading
   */
  function isLoading(key: ThumbnailCacheKey): boolean {
    return loadingKeys.has(key);
  }

  /**
   * Check if we can start a new load (respects maxConcurrentLoads)
   */
  function canStartLoad(): boolean {
    return loadingKeys.size < config.maxConcurrentLoads;
  }

  return {
    generateKey,
    get,
    set,
    has,
    delete: deleteEntry,
    clear,
    getStats,
    markLoadStart,
    markLoadEnd,
    isLoading,
    canStartLoad,
  };
}

/**
 * Default singleton cache instance for organize feature
 */
export const organizeThumbnailCache = createThumbnailCache({
  maxMemoryBytes: 8 * 1024 * 1024 * 1024, // 8GB
  maxConcurrentLoads: 4,
});

/**
 * Estimate decoded bitmap memory size from image dimensions
 * Assumes RGBA (4 bytes per pixel)
 */
export function estimateBitmapMemory(width: number, height: number): number {
  return width * height * 4; // RGBA
}

/**
 * Estimate memory size from thumbnail size hint
 * Uses common thumbnail dimensions as heuristic
 */
export function estimateMemoryFromSize(sizeHint: number): number {
  // Common sizes: 32, 48, 96, 128, 256, 512
  // Assume square thumbnails for estimation
  return sizeHint * sizeHint * 4; // RGBA
}
