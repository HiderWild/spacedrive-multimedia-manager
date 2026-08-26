# Async Media Derivatives (Add ≠ Generate)

**Date:** 2026-07-12  
**Status:** Core path unblocked — watcher + index enqueue pending + jobs.

## Goal

Decouple **adding/indexing photos** from **thumbnail / face embedding generation**:

1. Add path only creates entry + content identity (+ hash).
2. Status fields (via `sidecar.status`) track `pending | ready | failed` per derivative kind/variant.
3. Generation runs in background jobs, not inline on the watcher hot path.

## Design (existing schema)

Reuse the `sidecar` table (already has `status`):

| Kind | Variant | Meaning |
|------|---------|---------|
| `thumb` | `grid@1x` (import default) | Thumbnail readiness |
| `embeddings` | `face` | Face vector readiness |
| `embeddings` | `scene` | Scene embedding (CLIP/DINO) for visual clustering |

Query helper: `derivative_status_for_content(library, content_uuid)` →  
`ContentDerivativeStatus { thumbnail, face_embedding, scene_embedding }`.

## Code

| File | Change |
|------|--------|
| `core/src/ops/media/derivative_queue.rs` | `ensure_pending_sidecar`, enqueue helpers, status snapshot |
| `core/src/ops/indexing/change_detection/persistent.rs` | `run_processors`: hash only + enqueue derivatives (no inline thumb) |
| `core/src/ops/indexing/job.rs` | After Content/Deep index, batch-enqueue thumbs in background |

Still **blocking / synchronous by design**:

- Content hash on create (needed for `content_id`).
- Opt-in OCR/speech/proxy remain disabled by default (not on hot path).

## Face recognition

- Pending row is written for images (`embeddings`/`face`).
- Actual model inference stays in the photos extension / future job; UI can read `face_embedding == pending|ready`.

## UI query

```
query: media.derivativeStatus
```

Input:

```json
{
  "targets": [
    { "entry_uuid": "..." },
    { "content_uuid": "..." }
  ]
}
```

Output items: `thumbnail` / `face_embedding` / `scene_embedding` ∈ `missing|pending|ready|failed|not_applicable`, plus `not_found`.

## Batching

Watcher creates no longer each spawn a job immediately. `schedule_derivative_enqueue` debounces ~400ms and flushes one `ThumbnailJob` per library.

## Operator notes

- Bulk import should remain responsive; watch jobs complete via job manager / sidecar status.
- Thumb limits still apply inside `ThumbnailJob` (`SD_THUMB_MAX_CONCURRENT`, etc.).
