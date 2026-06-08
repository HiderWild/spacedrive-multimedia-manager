import { useEffect, useState, useRef } from "react";
import type { File } from "@sd/ts-client";
import {
  organizeThumbnailCache,
  estimateMemoryFromSize,
  type ThumbnailCacheKey,
} from "./thumbnailCache";
import { useServer } from "../../../contexts/ServerContext";

/**
 * Hook to load and cache thumbnails for organize feature
 *
 * Features:
 * - Automatic concurrent load limiting (4 max)
 * - Memory-based caching with FIFO eviction
 * - Returns cached data immediately if available
 * - Handles loading states
 */
export function useOrganizeThumbnail(file: File, size: number) {
  const { buildSidecarUrl } = useServer();
  const [thumbnailData, setThumbnailData] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const loadingRef = useRef(false);

  // Generate cache key based on file path and size
  const cacheKey = useRef<ThumbnailCacheKey | null>(null);

  useEffect(() => {
    // Extract physical path from file
    const getPhysicalPath = (file: File): string | null => {
      if (file.sd_path && "Physical" in file.sd_path) {
        return file.sd_path.Physical.path;
      }
      return null;
    };

    const physicalPath = getPhysicalPath(file);
    if (!physicalPath) {
      setThumbnailData(null);
      return;
    }

    const isDirectory = file.kind === "Directory";
    const key = organizeThumbnailCache.generateKey(
      physicalPath,
      file.size ?? 0,
      isDirectory
    );
    cacheKey.current = key;

    // Check cache first
    const cached = organizeThumbnailCache.get(key);
    if (cached) {
      setThumbnailData(cached.data);
      setIsLoading(false);
      setError(null);
      return;
    }

    // Check if already loading
    if (organizeThumbnailCache.isLoading(key) || loadingRef.current) {
      return;
    }

    // Check if we can start a new load
    if (!organizeThumbnailCache.canStartLoad()) {
      // Wait until a slot opens up
      const checkInterval = setInterval(() => {
        if (organizeThumbnailCache.canStartLoad() && !loadingRef.current) {
          clearInterval(checkInterval);
          startLoad();
        }
      }, 100);

      return () => clearInterval(checkInterval);
    }

    startLoad();

    function startLoad() {
      if (!file.content_identity?.uuid || loadingRef.current) return;

      // Find appropriate thumbnail sidecar
      const thumbnails = file.sidecars.filter((s) => s.kind === "thumb");
      if (thumbnails.length === 0) {
        return;
      }

      // Select best thumbnail variant (prefer lower resolution for organize)
      const thumbnail = thumbnails.sort((a, b) => {
        const aSize = parseInt(a.variant.split("x")[0]?.replace(/\D/g, "") || "0");
        const bSize = parseInt(b.variant.split("x")[0]?.replace(/\D/g, "") || "0");
        return Math.abs(aSize - size) - Math.abs(bSize - size);
      })[0];

      if (!thumbnail) return;

      const thumbnailUrl = buildSidecarUrl(
        file.content_identity.uuid,
        thumbnail.kind,
        thumbnail.variant,
        thumbnail.format
      );

      loadingRef.current = true;
      organizeThumbnailCache.markLoadStart(key);
      setIsLoading(true);
      setError(null);

      // Load thumbnail as blob
      fetch(thumbnailUrl)
        .then((response) => {
          if (!response.ok) {
            throw new Error(`Failed to load thumbnail: ${response.statusText}`);
          }
          return response.blob();
        })
        .then((blob) => {
          // Convert to object URL for display
          const objectUrl = URL.createObjectURL(blob);

          // Estimate memory size (assume decoded bitmap)
          const memorySize = estimateMemoryFromSize(size);

          // Store in cache
          organizeThumbnailCache.set(key, objectUrl, memorySize);
          setThumbnailData(objectUrl);
          setIsLoading(false);
        })
        .catch((err) => {
          setError(err instanceof Error ? err : new Error(String(err)));
          setIsLoading(false);
        })
        .finally(() => {
          organizeThumbnailCache.markLoadEnd(key);
          loadingRef.current = false;
        });
    }
  }, [file, size, buildSidecarUrl]);

  // Cleanup: revoke object URLs when unmounting
  useEffect(() => {
    return () => {
      if (thumbnailData && thumbnailData.startsWith("blob:")) {
        URL.revokeObjectURL(thumbnailData);
      }
    };
  }, [thumbnailData]);

  return {
    thumbnailData,
    isLoading,
    error,
    cacheKey: cacheKey.current,
  };
}

/**
 * Preload thumbnails for a list of files (respects concurrent load limit)
 */
export function preloadOrganizeThumbnails(
  files: File[],
  size: number,
  buildSidecarUrl: (uuid: string, kind: string, variant: string, format: string) => string
): void {
  const queue = [...files];

  function processNext() {
    if (queue.length === 0) return;

    if (!organizeThumbnailCache.canStartLoad()) {
      // Wait and retry
      setTimeout(processNext, 100);
      return;
    }

    const file = queue.shift();
    if (!file) return;

    const getPhysicalPath = (file: File): string | null => {
      if (file.sd_path && "Physical" in file.sd_path) {
        return file.sd_path.Physical.path;
      }
      return null;
    };

    const physicalPath = getPhysicalPath(file);
    if (!physicalPath) {
      processNext();
      return;
    }

    const isDirectory = file.kind === "Directory";
    const key = organizeThumbnailCache.generateKey(
      physicalPath,
      file.size ?? 0,
      isDirectory
    );

    // Skip if already cached or loading
    if (organizeThumbnailCache.has(key) || organizeThumbnailCache.isLoading(key)) {
      processNext();
      return;
    }

    if (!file.content_identity?.uuid) {
      processNext();
      return;
    }

    const thumbnails = file.sidecars.filter((s) => s.kind === "thumb");
    if (thumbnails.length === 0) {
      processNext();
      return;
    }

    const thumbnail = thumbnails[0];
    const thumbnailUrl = buildSidecarUrl(
      file.content_identity.uuid,
      thumbnail.kind,
      thumbnail.variant,
      thumbnail.format
    );

    organizeThumbnailCache.markLoadStart(key);

    fetch(thumbnailUrl)
      .then((response) => response.blob())
      .then((blob) => {
        const objectUrl = URL.createObjectURL(blob);
        const memorySize = estimateMemoryFromSize(size);
        organizeThumbnailCache.set(key, objectUrl, memorySize);
      })
      .catch(() => {
        // Silent fail for preload
      })
      .finally(() => {
        organizeThumbnailCache.markLoadEnd(key);
        processNext();
      });

    // Continue with next
    processNext();
  }

  // Start processing
  processNext();
}
