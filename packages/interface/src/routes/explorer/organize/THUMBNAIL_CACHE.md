# Organize Thumbnail Cache

## Overview

In-memory thumbnail cache for the organize feature with automatic memory management and concurrent load limiting.

## Features

- **Cache Key**: `materialized_path + size_bytes + isDirectory`
- **Concurrent Loading**: 4 parallel loads (configurable)
- **Memory Limit**: 8GB (configurable)
- **Eviction Strategy**: FIFO (First In, First Out)
- **Lifecycle**: Memory-only, destroyed on process exit
- **Directory Support**: Supports composite thumbnails for directories (album view)

## Architecture

### Core Module: `thumbnailCache.ts`

The cache module provides a simple key-value store with memory tracking and FIFO eviction:

```typescript
import { organizeThumbnailCache } from './thumbnailCache';

// Generate cache key
const key = organizeThumbnailCache.generateKey(
  '/photos/vacation.jpg',
  1024, // file size in bytes
  false // isDirectory
);

// Check cache
if (organizeThumbnailCache.has(key)) {
  const entry = organizeThumbnailCache.get(key);
  console.log('Cached data:', entry.data);
}

// Store thumbnail
organizeThumbnailCache.set(key, blobUrl, memorySize);

// Get statistics
const stats = organizeThumbnailCache.getStats();
console.log('Cache usage:', stats.totalMemoryBytes, '/', stats.maxMemoryBytes);
```

### React Hook: `useOrganizeThumbnail.ts`

React hook that integrates the cache with component lifecycle:

```typescript
import { useOrganizeThumbnail } from './useOrganizeThumbnail';

function ThumbnailDisplay({ file }: { file: File }) {
  const { thumbnailData, isLoading, error } = useOrganizeThumbnail(file, 96);

  if (isLoading) return <Spinner />;
  if (error) return <ErrorIcon />;
  if (!thumbnailData) return <FallbackIcon />;

  return <img src={thumbnailData} alt={file.name} />;
}
```

### Preloading: `preloadOrganizeThumbnails`

Preload thumbnails for a list of files (respects concurrent load limit):

```typescript
import { preloadOrganizeThumbnails } from './useOrganizeThumbnail';
import { useServer } from '../../../contexts/ServerContext';

function FileList({ files }: { files: File[] }) {
  const { buildSidecarUrl } = useServer();

  useEffect(() => {
    // Preload thumbnails when files change
    preloadOrganizeThumbnails(files, 96, buildSidecarUrl);
  }, [files, buildSidecarUrl]);

  return <div>{/* render files */}</div>;
}
```

## Memory Estimation

The cache estimates decoded bitmap memory using the formula:

```
memorySize = width × height × 4 bytes (RGBA)
```

For example:
- 96×96 thumbnail = 36,864 bytes (~36 KB)
- 256×256 thumbnail = 262,144 bytes (~256 KB)
- 512×512 thumbnail = 1,048,576 bytes (~1 MB)

## FIFO Eviction

When adding a new thumbnail would exceed the memory limit:

1. Cache evicts oldest entries until there's enough space
2. Entries are evicted in insertion order (FIFO)
3. The new entry is added

Example:
```
Cache limit: 4MB
Current: [1MB, 1MB, 1MB] = 3MB
Add: 2MB thumbnail
Result: Evicts first two entries, keeps: [1MB, 2MB] = 3MB
```

## Concurrent Load Limiting

The cache enforces a maximum of 4 concurrent thumbnail loads:

```typescript
// Check if we can start a new load
if (organizeThumbnailCache.canStartLoad()) {
  const key = organizeThumbnailCache.generateKey(path, size);
  
  organizeThumbnailCache.markLoadStart(key);
  
  fetch(thumbnailUrl)
    .then(/* ... */)
    .finally(() => {
      organizeThumbnailCache.markLoadEnd(key);
    });
}
```

## Directory Thumbnails

Directory thumbnails (composite images for album view) are supported:

```typescript
const dirKey = organizeThumbnailCache.generateKey(
  '/photos/vacation',
  0, // directories don't have size
  true // isDirectory = true
);

organizeThumbnailCache.set(dirKey, compositeImageUrl, estimatedMemory);
```

## Integration with Existing Thumb Component

The cache is designed to work alongside the existing `Thumb` component without breaking its features:

- ✅ Hover-scrub behavior preserved
- ✅ Icon-first rendering preserved
- ✅ Fallback icons work as before
- ✅ Bearded icon badges preserved

To integrate with `Thumb.tsx`:

1. Import the cache module
2. Check cache before loading thumbnail
3. Store loaded thumbnails in cache
4. Keep existing fallback behavior

## Testing

Run tests:
```bash
bun test packages/interface/src/routes/explorer/organize/__tests__/thumbnailCache.test.ts
bun test packages/interface/src/routes/explorer/organize/__tests__/useOrganizeThumbnail.test.ts
```

Test coverage:
- ✅ Cache key generation
- ✅ FIFO eviction
- ✅ Memory tracking
- ✅ Concurrent load limiting
- ✅ Directory thumbnail support
- ✅ React hook lifecycle
- ✅ Preloading

## Configuration

Create a custom cache instance with different limits:

```typescript
import { createThumbnailCache } from './thumbnailCache';

const customCache = createThumbnailCache({
  maxMemoryBytes: 4 * 1024 * 1024 * 1024, // 4GB
  maxConcurrentLoads: 8, // 8 parallel loads
});
```

## Performance Considerations

### Memory Usage
- 8GB cache can hold approximately:
  - 222,000 thumbnails at 96×96 (36 KB each)
  - 31,250 thumbnails at 256×256 (256 KB each)
  - 8,000 thumbnails at 512×512 (1 MB each)

### Load Concurrency
- 4 concurrent loads balances speed with resource usage
- Increase for faster bulk loading (higher CPU/network usage)
- Decrease for lower resource usage (slower loading)

### Cache Hit Rate
- Higher hit rates = better performance
- FIFO eviction works well for sequential browsing
- Consider LRU (Least Recently Used) for random access patterns

## Future Enhancements

Potential improvements:
- [ ] LRU eviction strategy option
- [ ] Persistent cache (IndexedDB)
- [ ] Progressive loading (low-res → high-res)
- [ ] Cache warming based on scroll position
- [ ] Per-directory cache statistics
- [ ] Automatic memory pressure detection
