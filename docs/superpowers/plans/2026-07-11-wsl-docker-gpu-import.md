# WSL Docker GPU + Bulk Import Throttling

**Date:** 2026-07-11  
**Status:** Phase 0–3 complete; NVENC path via `FFMPEG_PATH` landed; live bulk-import timing remains operator-side. Phase 4 (full AI ORT/Whisper CUDA runtime) still deferred pending real AI product work.

## Problem

Bulk image import froze hosts / OOM'd Docker because:

1. Thumbnail batches used unbounded `join_all` (ignored `max_concurrent`)
2. Defaults generated multiple large variants (`grid@1x` + `grid@2x` + `detail@1x`)
3. Watcher auto-ran `thumbstrip` (~6s/video) on import
4. Compose limited memory to 2G with no GPU path

JPEG/HEIC thumbs are still mostly **CPU-bound**; GPU helps video encode (when ffmpeg has NVENC) and future AI. Optional `turbojpeg` feature speeds JPEG decode/scale on Linux images with libturbojpeg.

## Changes landed

### Core

| File | Change |
|------|--------|
| `core/src/ops/media/thumbnail/job.rs` | `Semaphore(max_concurrent)`; env `SD_THUMB_*`; default batch 16 / concurrent 2; `generate_many` |
| `core/src/ops/media/thumbnail/generator.rs` | Single-decode multi-size; large-image budget; faster filter for big downscales; `format_image_for_thumbnail` |
| `core/src/ops/media/thumbnail/config.rs` | `import_defaults()` → only `grid@1x` |
| `core/src/ops/media/thumbnail/processor.rs` / `mod.rs` | Import-path uses `import_defaults` + multi-size emit |
| `core/src/ops/indexing/processor.rs` | Default watcher: `grid@1x` only; **thumbstrip disabled** |
| `core/src/ops/media/ffmpeg_bin.rs` | Resolves `FFMPEG_PATH` / `FFMPEG` for NVENC-capable binaries |
| `core/src/ops/media/{proxy,transcode,stream}/*` | All ffmpeg spawns/probes use `ffmpeg_bin` |
| `core/Cargo.toml` | `turbojpeg`; `whisper-cuda` feature (forwards `whisper-rs/cuda`) |

### Images crate

| File | Change |
|------|--------|
| `crates/images` | Feature `turbojpeg`; `format_image_for_thumbnail`; scaled JPEG decode via TurboJPEG |

### Server / Docker

| File | Change |
|------|--------|
| `apps/server/docker-compose.yml` | Memory 8G; env thumb knobs; `FFMPEG_PATH` comment |
| `apps/server/docker-compose.gpu.yml` | Overlay: `gpus: all`, CUDA env, `Dockerfile.gpu`, `FFMPEG_PATH` comment |
| `apps/server/Dockerfile.gpu` | CUDA 12.4 + `libturbojpeg` + features `heif,ffmpeg,turbojpeg` |
| `apps/server/src/main.rs` | Startup: thumb knobs + ffmpeg binary path + HW encoder probe |
| `apps/server/Cargo.toml` | Feature `turbojpeg` |
| `apps/server/README-GPU.md` | Operator guide (GPU, NVENC, benches) |

### Docs / scripts

- `AGENTS.md` — bulk import throttling + WSL GPU + turbojpeg + benches
- `scripts/bench-thumbnail-import.ps1` — offline decode/resize concurrency bench
- `scripts/bench-live-import.ps1` — live daemon sampling checklist + docker stats CSV
- `docs/superpowers/plans/2026-07-11-import-bench-report.md` — fill-in results table
- `apps/server/docker-compose.nvenc.example.yml` — mount host NVENC ffmpeg

## Verify on host (manual)

```bash
# WSL GPU
nvidia-smi
docker run --rm --gpus all nvidia/cuda:12.4.1-base-ubuntu22.04 nvidia-smi

# CPU path
cd apps/server && docker compose up -d --build

# GPU + turbojpeg path
docker compose -f docker-compose.yml -f docker-compose.gpu.yml up -d --build
docker compose exec spacedrive nvidia-smi
# logs: thumbnail limits, turbojpeg feature, ffmpeg binary, HW probe
```

```powershell
./scripts/bench-thumbnail-import.ps1 -Count 100 -Concurrency 2
./scripts/bench-live-import.ps1 -Label "100-jpeg" -Container spacedrive-server
```

## Phase checklist

- [x] Phase 0: throttle + lower variants + raise memory + disable thumbstrip default
- [x] Phase 1–2 scaffolding: GPU compose overlay + CUDA Dockerfile
- [x] Phase 3: single decode multi-size + large-image budget + faster filter
- [x] Phase 3 optional: turbojpeg feature path (on when feature/Docker.gpu)
- [x] Phase 3: offline import micro-bench script
- [x] Startup HW / thumb knob diagnostics
- [x] NVENC path: `FFMPEG_PATH` resolver wired through proxy/transcode/stream/detect
- [x] Live-import operator bench script (`bench-live-import.ps1`)
- [x] Whisper CUDA **feature flag** scaffolding (`whisper-cuda`)
- [x] NVENC compose example (`docker-compose.nvenc.example.yml`)
- [x] Import bench report template (`2026-07-11-import-bench-report.md`)
- [ ] Full library import timings filled by operator (100/500/1000 report rows)
- [ ] NVENC-capable ffmpeg binary installed and mounted (host-specific)
- [ ] Phase 4: end-to-end AI recognition (ORT + models + CUDA runtime product work)

## Knobs

```
SD_THUMB_MAX_CONCURRENT=2
SD_THUMB_BATCH_SIZE=16
FFMPEG_PATH=/path/to/ffmpeg-with-nvenc   # optional
```
