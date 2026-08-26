# Spacedrive Server: WSL2 Docker + GPU + Bulk Import

## Why imports froze

Batch photo import was CPU-bound and could OOM:

- Thumbnail batches ignored `max_concurrent` (unbounded `join_all`)
- Multiple large thumbnail variants by default
- Video `thumbstrip` ran automatically
- Compose memory limit was 2G

JPEG/HEIC thumbs are still primarily CPU work. GPU helps video HW encode (if ffmpeg has NVENC) and future AI.

## Quick start (CPU, safer first)

From `apps/server/`:

```bash
docker compose up -d --build
docker compose logs -f --tail=100 spacedrive
```

Look for startup lines:

- `Thumbnail import limits: SD_THUMB_MAX_CONCURRENT=...`
- `JPEG turbo decode feature: ...`
- `ffmpeg H.264 HW encoders: ...`

## GPU path (WSL2 + NVIDIA)

Prerequisites:

1. Windows NVIDIA driver with WSL support
2. In WSL: `nvidia-smi`
3. NVIDIA Container Toolkit; verify:
   `docker run --rm --gpus all nvidia/cuda:12.4.1-base-ubuntu22.04 nvidia-smi`

Run:

```bash
docker compose -f docker-compose.yml -f docker-compose.gpu.yml up -d --build
docker compose exec spacedrive nvidia-smi
```

`Dockerfile.gpu` builds with `heif,ffmpeg,turbojpeg` and installs `libturbojpeg`.

### NVENC overlay example

```bash
# 1) Install/build an NVENC ffmpeg on the host, e.g. /opt/ffmpeg-nvenc/bin/ffmpeg
# 2) Copy and edit paths:
cp docker-compose.nvenc.example.yml docker-compose.nvenc.yml
# 3) Stack all three files:
docker compose \
  -f docker-compose.yml \
  -f docker-compose.gpu.yml \
  -f docker-compose.nvenc.yml \
  up -d
# 4) Confirm logs show FFMPEG_PATH and nvenc=true
docker compose logs --tail=50 spacedrive
```

## Thumb knobs

```bash
SD_THUMB_MAX_CONCURRENT=2   # in-flight thumbnail generations
SD_THUMB_BATCH_SIZE=16      # discovery batch size
```

Defaults keep import stable on constrained hosts. Raise only after a successful bulk import.

## Offline micro-bench (Windows host)

```powershell
./scripts/bench-thumbnail-import.ps1 -Count 100 -Concurrency 2
./scripts/bench-thumbnail-import.ps1 -Count 100 -Concurrency 8
```

This only measures decode/resize concurrency; it does not run the daemon.

## NVENC / custom ffmpeg (`FFMPEG_PATH`)

Stock Ubuntu/Debian `ffmpeg` packages usually **do not** include `h264_nvenc`.  
If logs show `nvenc=false`, video proxy/thumbstrip stay on CPU even with GPU passthrough.

You can point Spacedrive at a different binary without replacing the distro package:

```bash
# Host / compose
export FFMPEG_PATH=/usr/local/bin/ffmpeg-nvenc
# or
export FFMPEG=/opt/ffmpeg/bin/ffmpeg
```

Proxy, transcode, stream encode, and HW detection all use this resolver  
(`core/src/ops/media/ffmpeg_bin.rs`). Startup logs the resolved binary and encoder probe.

Example (bind-mount an NVENC build into the GPU container):

```yaml
# snippet for docker-compose.gpu.yml override
services:
  spacedrive:
    volumes:
      - /opt/ffmpeg-nvenc:/opt/ffmpeg-nvenc:ro
    environment:
      - FFMPEG_PATH=/opt/ffmpeg-nvenc/bin/ffmpeg
```

## Optional host features

```bash
# Fast JPEG (libturbojpeg)
cargo build -p sd-server --features heif,ffmpeg,turbojpeg

# Whisper CUDA (Linux + CUDA toolkit; large/slow build — prefer GPU CI images)
cargo build -p sd-core --features speech-to-text,whisper-cuda
```

## Live import bench (operator)

```powershell
# Start sampling, run your import in another terminal, press Enter when done
./scripts/bench-live-import.ps1 -Label "100-jpeg" -Container spacedrive-server
```

Offline decode-only concurrency bench (no daemon):

```powershell
./scripts/bench-thumbnail-import.ps1 -Count 100 -Concurrency 2
```
