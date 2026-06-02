# Reference Code Extraction Map

This document is a long-lived guide for the media-suite development effort. It maps
every reusable capability in the `reference/` repositories to one of three reuse
tiers, names the concrete source files, and states how the code lands inside
Spacedrive (Rust core + React/Tauri frontend).

The fork follows AGPL-3.0 and is used locally and non-commercially, so license
compatibility is **not** a blocker for copying source. The tiers below are therefore
driven purely by **engineering effort**, not legal constraints.

## How to read this map

| Tier | Meaning | Typical effort |
|------|---------|----------------|
| **T1 — Lift as-is** | Same language/runtime as a Spacedrive surface. Copy the file/module, fix imports, wire it in. Minimal logic change. | Hours |
| **T2 — Light refactor** | Same language but different framework/state model, or a self-contained npm/crate dependency. Copy and adapt API surface, types, and integration points. | Days |
| **T3 — Cross-language port** | Source is Python/Go/Vue/Svelte/Angular. Logic and data model are the blueprint; reimplement in Rust or React. Reference behavior, schema, and FFmpeg arg strings verbatim. | Days–weeks |

Spacedrive surfaces and their languages:

- **Core** — Rust (`core/`, `crates/`)
- **Frontend** — React + TypeScript (`packages/interface/`, `packages/ts-client/`)
- **FFmpeg layer** — Rust wrapper over the `ffmpeg` CLI/bindings (`crates/ffmpeg/`)

Reference repo languages: mediaChips (Vue/Electron), hydrus (Python), TagStudio
(Python/Qt), Video-Hub-App (Angular/Electron), immich (NestJS/Svelte), photoprism (Go).
Because no reference repo shares Spacedrive's exact stack, **there are no pure T1
extractions across language boundaries**. T1 is reserved for self-contained,
framework-agnostic JS/TS modules that drop into the React frontend, and for npm
packages that are already published as standalone libraries.

---

## 1. Tag inheritance, hierarchy, siblings/parents

Spacedrive already has a semantic tag DAG with a closure table
([core/src/domain/tag.rs](../../core/src/domain/tag.rs),
[core/src/ops/tags/](../../core/src/ops/tags/)). What is missing is **folder→child
recursive inheritance with override** and a **sibling/parent relation model**.

| Capability | Source files | Tier | Landing strategy |
|------------|--------------|------|------------------|
| Tag **parents** (hierarchical implication: "cat" ⇒ "animal") | `reference/hydrus/hydrus/client/db/ClientDBTagParents.py` | **T3** | Port the implication-expansion algorithm into the existing closure-table logic in [core/src/ops/tags/ancestors.rs](../../core/src/ops/tags/ancestors.rs). Reuse hydrus's "virtual" expansion (parents are computed at query time, not stored on every file). |
| Tag **siblings** (synonyms collapse to a canonical tag) | `reference/hydrus/hydrus/client/db/ClientDBTagSiblings.py`; content-type constants in `reference/hydrus/hydrus/core/HydrusConstants.py` (`CONTENT_TYPE_TAG_SIBLINGS/PARENTS`) | **T3** | Map onto the existing `TagRelationship::Synonym` in [core/src/domain/tag.rs](../../core/src/domain/tag.rs). Port hydrus's "ideal sibling" resolution that picks one canonical tag per synonym cluster. |
| Tag↔Entry **join schema** + parent join table | `reference/TagStudio/src/tagstudio/core/library/alchemy/models.py` (`Tag`, `TagParent`, `Entry.tags`); `reference/TagStudio/src/tagstudio/core/library/alchemy/library.py` | **T3** | Use as the SeaORM schema blueprint. TagStudio's `TagParent` many-to-many and `contains_eager` loading pattern maps directly to the new `tag_inheritance` design. Read schema, reimplement as Rust migrations. |
| Complex "meta cards" (rich tags with custom fields/ratings/images) | `reference/mediaChips/src/store/modules/meta.js` | **T3 (reference only)** | Blueprint for a future "rich tag" feature. Borrow the simple-vs-complex tag distinction; do not port Vuex code. |

**Net:** all tag work is T3 (Python→Rust). hydrus is the authoritative algorithm
source; TagStudio is the schema source. The folder-inheritance-with-override logic
itself has **no reference** and is fully bespoke (see Requirements EPIC-A).

---

## 2. Video thumbnails, filmstrips, hover scrubbing

Spacedrive already extracts a single video frame
([crates/ffmpeg/src/thumbnailer.rs](../../crates/ffmpeg/src/thumbnailer.rs)) and
generates WebP storyboard grids
([core/src/ops/media/thumbstrip/](../../core/src/ops/media/thumbstrip/)). The gap is
the **frontend hover-scrub interaction** over those sprite sheets.

| Capability | Source files | Tier | Landing strategy |
|------------|--------------|------|------------------|
| Filmstrip sprite **hover-scrub** offset math | `reference/Video-Hub-App/src/app/components/views/filmstrip/` (offset formula `imgWidth * floor(cursorX / (containerWidth / screens))`) | **T2** | MIT, Angular. Logic is trivial and framework-agnostic. Reimplement as a small React hook driving CSS `background-position` over the existing thumbstrip sidecar. ~30 lines. |
| Sprite-sheet **dimension config** (rows/cols/screens count) | `reference/Video-Hub-App/src/app/common/app-state.ts` (`thumbnailSheet`) | **T2** | Align the React component's expectations with the grid layout already emitted by [core/src/ops/media/thumbstrip/](../../core/src/ops/media/thumbstrip/). Tune `screens` to match. |
| Multi-size frame extraction + caching strategy | `reference/photoprism/internal/thumb/video.go` | **T3 (reference)** | Confirm our FFmpeg seek/extract args against photoprism's proven flags. Our extraction already exists; use only to validate quality/perf choices. |

**Net:** the only new extraction here is the **hover-scrub hook (T2)**; backend
already produces the sprites.

---

## 3. Justified / masonry gallery for large libraries

Spacedrive virtualizes with `@tanstack/react-virtual` but has only fixed-grid and
date-grouped views. immich's gallery is the gold standard for 20TB-scale justified
layout.

| Capability | Source files | Tier | Landing strategy |
|------------|--------------|------|------------------|
| **Justified layout** row-packing algorithm | npm `justified-layout` (MIT, used by immich); WASM variant `@immich/justified-layout-wasm` | **T1** | Pure standalone npm package. `pnpm add justified-layout`, feed it aspect ratios from media metadata, render into a new `MasonryView`. No porting. |
| Timeline / virtual-scroll **manager pattern** (date buckets, segment heights, scroll restoration) | `reference/immich/web/src/lib/managers/timeline-manager/` | **T3 (reference)** | Svelte → React. Use as the architectural blueprint for the `MasonryView` data manager: how to bucket by date, precompute segment heights, and drive the virtualizer. Reimplement in TS against our [useExplorerFiles.ts](../../packages/interface/src/routes/explorer/hooks/useExplorerFiles.ts). |

**Net:** `justified-layout` is a clean **T1 npm drop-in**; the surrounding
virtual-scroll manager is a **T3** architectural reference.

---

## 4. Transcoding, streaming formats, GPU acceleration

Spacedrive has a proxy transcoder (H.264/MP4 only) with HW-accel detection
([core/src/ops/media/proxy/](../../core/src/ops/media/proxy/),
[core/src/ops/media/proxy/hardware.rs](../../core/src/ops/media/proxy/hardware.rs)).
Gaps: **generic codecs, HLS/DASH streaming output, vendor-neutral GPU**.

| Capability | Source files | Tier | Landing strategy |
|------------|--------------|------|------------------|
| **Vendor-neutral GPU H.264** (`h264_vulkan`, single `TranscodeToAvcCmd()` entry) | `reference/photoprism/internal/ffmpeg/vulkan/vulkan.go` + `README.md`; `reference/photoprism/internal/ffmpeg/` builders | **T3** | Go → Rust. Copy the exact FFmpeg argument strings (filters, `-c:v h264_vulkan`, device init). Extend [core/src/ops/media/proxy/generator.rs](../../core/src/ops/media/proxy/generator.rs) and `hardware.rs` with a Vulkan path. The arg strings transfer verbatim; only the command-builder code is rewritten. |
| Modular **transcode builder** (codec/container/resolution matrix) | `reference/photoprism/internal/ffmpeg/` | **T3** | Blueprint for a generic `TranscodeJob` config enum. Mirror photoprism's builder decomposition in Rust. |
| **HLS/DASH** segmenting (`.m3u8` + `.ts`, per-resolution ladders) | `reference/immich/server/src/services/` media/transcode services (FFmpeg `-f hls` params) | **T3** | NestJS → Rust. Lift the FFmpeg HLS flag set (segment time, playlist type, bitrate ladder) verbatim into a new streaming job. Reimplement orchestration in the job system. |
| Image **format conversion** | already in [crates/images/src/handler.rs](../../crates/images/src/handler.rs) | n/a | Native — extend, no extraction. |

**Net:** all T3. The high-value, low-risk extraction is the **literal FFmpeg argument
strings** from photoprism (GPU) and immich (HLS); the surrounding code is rewritten.

---

## 5. Batch image rotation & EXIF correctness

Spacedrive reads EXIF orientation
([crates/media-metadata/src/exif/orientation.rs](../../crates/media-metadata/src/exif/orientation.rs))
but never writes rotated pixels back.

| Capability | Source files | Tier | Landing strategy |
|------------|--------------|------|------------------|
| Orientation enum (8 states) → pixel transform | native [crates/media-metadata/src/exif/orientation.rs](../../crates/media-metadata/src/exif/orientation.rs) + `image` crate rotate ops | n/a | Native. Build a `RotateJob` on top of [crates/images/src/handler.rs](../../crates/images/src/handler.rs). No reference needed. |
| ICC color-profile preservation across transforms | `reference/photoprism/internal/thumb/vips_icc.go` | **T3 (reference)** | Validate that rotation/transcode preserves ICC. Read for correctness rules only. |

**Net:** native work; photoprism is a correctness reference only.

---

## 6. Media preview, players, lightbox

Spacedrive already has a strong preview suite
([packages/interface/src/components/QuickPreview/](../../packages/interface/src/components/QuickPreview/):
`VideoPlayer.tsx`, image zoom/pan, audio, subtitles, 3D). **Nothing to extract** —
the wander/slideshow features build on these native components. mediaChips and
Video-Hub-App player UIs are inferior and not worth porting.

---

## 7. Duplicate detection

Spacedrive has [core/src/ops/files/duplicate_detection/](../../core/src/ops/files/) .

| Capability | Source files | Tier | Landing strategy |
|------------|--------------|------|------------------|
| Perceptual/pixel similarity & dedup workflow | `reference/hydrus/hydrus/client/db/` (similar-file search, hash comparison) | **T3 (reference)** | Read for the perceptual-hash + threshold workflow if we extend beyond exact-hash dedup. Algorithm reference only. |

---

## Extraction summary table

| # | Capability | Best source | Tier | New code location |
|---|-----------|-------------|------|-------------------|
| 1a | Tag parents (implication) | hydrus | T3 | `core/src/ops/tags/` |
| 1b | Tag siblings (synonyms) | hydrus | T3 | `core/src/ops/tags/` |
| 1c | Tag/Entry join schema | TagStudio | T3 | SeaORM migrations |
| 1d | Folder inheritance + override | — (bespoke) | new | `core/src/ops/tags/inheritance/` |
| 2a | Filmstrip hover-scrub | Video-Hub-App | **T2** | `packages/interface/.../explorer/File/` |
| 3a | Justified layout | npm `justified-layout` | **T1** | new `MasonryView` |
| 3b | Virtual timeline manager | immich | T3 | `packages/interface/.../explorer/views/MasonryView/` |
| 4a | GPU (Vulkan) FFmpeg args | photoprism | T3 | `core/src/ops/media/proxy/` |
| 4b | Generic transcode builder | photoprism | T3 | `core/src/ops/media/transcode/` |
| 4c | HLS/DASH segmenting | immich | T3 | `core/src/ops/media/streaming/` |
| 5 | Batch rotation | native + photoprism (ICC ref) | native/T3 | `core/src/ops/media/rotate/` |
| 7 | Perceptual dedup | hydrus | T3 (ref) | `core/src/ops/files/duplicate_detection/` |

**Bottom line:** Exactly **one true drop-in (T1)** — the `justified-layout` npm
package. **One light refactor (T2)** — the filmstrip hover-scrub hook. **Everything
else is a cross-language port (T3)** where the reference supplies the algorithm,
database schema, or literal FFmpeg argument strings, and we reimplement in
Rust/React. The single most valuable cross-language asset is the **verbatim FFmpeg
command strings** from photoprism (GPU) and immich (HLS): copy the args, rewrite the
wrapper.
