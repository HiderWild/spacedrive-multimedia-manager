# Scene Clustering Implementation Plan

**Date:** 2026-07-12  
**Spec:** `docs/superpowers/specs/2026-07-12-scene-clustering-design.md`

## Model decision

| Choice | Model | Why |
|--------|-------|-----|
| **Primary** | OpenCLIP ViT-B/32 → ONNX | Best balance of cluster quality, text search, GPU ORT support, ecosystem maturity |
| **Quality max** | DINOv2 ViT-B/14 → ONNX | Often better pure-vision clusters; no text query |
| **Existing Places365 ResNet** | Labels only | Keep for `#scene:label` tags — **not** sufficient alone for clustering |

## Status bits (async, non-blocking add)

| Field | Storage |
|-------|---------|
| `scene_embedding` readiness | `sidecar` kind=`embeddings` variant=`scene` status pending/ready/failed |
| UI | `media.derivativeStatus` → `scene_embedding` |

## Core delivered

- [x] `ops/media/clustering.rs` — cosine DBSCAN + unit tests  
- [x] Scene variant on derivative queue + status query  
- [x] Design spec  

## Extension delivered

- [x] Face DBSCAN implemented (was `todo!`)  
- [x] `cluster_scenes` job scaffold  
- [x] Config: `scene_clustering`, `scene_clustering_eps`  
- [x] Permissions for `image_embedding`  

## Still required for end-to-end “works on disk”

1. **Host SceneEmbedJob** — load OpenCLIP ONNX via `ort` with CUDA EP; write MsgPack vector to `embeddings/scene`  
2. Package/download model weights into `~/.spacedrive/models/image_embedding/`  
3. Wire job after index batch (same debounce pattern as thumbnails)  
4. UI: album strips per `#scene_cluster:*`  

## GPU

Use existing WSL Docker GPU path + ORT:

```text
ExecutionProviders: [CUDAExecutionProvider, CPUExecutionProvider]
```

Clustering remains CPU DBSCAN (cheap vs encode).
