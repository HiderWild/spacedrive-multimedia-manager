# Bulk Import Bench Report Template

Fill this after each live import run (see `scripts/bench-live-import.ps1`).  
One table row per scenario; check the plan item when 100 / 500 / 1000 are done **and** the host did not freeze.

## Environment

| Field | Value |
|-------|--------|
| Date (UTC) | |
| Host (CPU/RAM/GPU) | |
| Runtime | Docker CPU / Docker GPU / native |
| Image / commit | |
| `SD_THUMB_MAX_CONCURRENT` | |
| `SD_THUMB_BATCH_SIZE` | |
| `FFMPEG_PATH` | |
| turbojpeg feature | on / off |
| Media mount path | |

## Results

| Images | Content | Duration (s) | Peak mem | Froze? | Notes / CSV path |
|-------:|---------|-------------:|---------:|:------:|------------------|
| 100 | JPEG only | | | | |
| 500 | JPEG only | | | | |
| 1000 | JPEG only | | | | |
| 100 | mixed + video | | | | optional |

## Acceptance

- [ ] 100 images complete without host freeze
- [ ] 500 images complete without host freeze
- [ ] 1000 images complete without host freeze
- [ ] Peak container memory stays under compose limit (default 8G)
- [ ] UI/HTTP remains responsive during import (spot-check `/health`)

## Commands used

```powershell
./scripts/bench-thumbnail-import.ps1 -Count 100 -Concurrency 2
./scripts/bench-live-import.ps1 -Label "100-jpeg" -Container spacedrive-server
```

```bash
cd apps/server
docker compose up -d --build
# GPU:
docker compose -f docker-compose.yml -f docker-compose.gpu.yml up -d --build
# NVENC (after copying example and editing paths):
# docker compose -f docker-compose.yml -f docker-compose.gpu.yml -f docker-compose.nvenc.yml up -d
```
