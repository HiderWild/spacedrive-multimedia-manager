# Scene Clustering Design

**Date:** 2026-07-12  
**Status:** Spec + core clustering + async status scaffolding

## Problem

We want **scene-style clusters** similar to face clusters: group photos that depict
the same *kind of place/situation* (beach evenings, snowy trails, kitchen tables)
without requiring the user to label them.

## Can existing models do this?

| Existing asset | Role | Enough for scene clustering? |
|----------------|------|-------------------------------|
| `face_detection:photos_v1` + face embeddings | Identity clusters | **No** — faces only |
| `scene_classification:resnet50` (Places365) | Discrete labels | **Partial** — only fixed taxonomy buckets; similar “sunset beach” shots may share a label but cannot form continuous visual clusters or near-duplicates across labels |
| Content hash | Exact/near-file dedupe | **No** — not semantic |

**Conclusion:** Places365 classification is useful for tags (`#beach`) but **not** for
embedding-space scene clustering. We need a dedicated **image embedding** model.

## Model selection (effect-first, local, GPU-capable)

### Recommended primary: **OpenCLIP ViT-B/32** (ONNX)

| Criterion | Assessment |
|-----------|------------|
| Clustering quality | Strong; industrial default for photo libraries |
| Search | Enables text queries (“mountain lake”) for free |
| Size | ~150–350MB ONNX depending on export |
| GPU | ONNX Runtime CUDA EP (WSL Docker already prepared) |
| License | OpenCLIP / model-card dependent (typically MIT/Apache + data notes) |
| Ecosystem | Hugging Face exports, ONNX, `ort` crate, many photo managers |

### Strong alternative: **DINOv2 ViT-B/14**

| Criterion | Assessment |
|-----------|------------|
| Clustering quality | Often **best pure-vision** separations (no text bottleneck) |
| Search | No native text alignment unless dual-index |
| Size | Larger than CLIP-B/32 for similar depth |
| GPU | Same ORT CUDA path |

### Optional light path: **MobileCLIP / CLIP-ViT-B/16-quant**

Use when RAM-constrained; quality trade-off.

### Not recommended as the only path

- Places365 ResNet50 alone (taxonomy, not metric space)
- Closed cloud vision APIs (breaks local-first)

## Pipeline (aligned with async derivatives)

```
Index/add photo
  → mark sidecar embeddings/scene = pending
  → background SceneEmbedJob (CLIP/DINO ONNX, GPU if available)
  → write MsgPack/JSON embedding vector + status ready
  → SceneClusterJob (cosine DBSCAN/HDBSCAN on pending library set)
  → SceneCluster models + tags (#scene_cluster:…)
```

Do **not** embed inline on the add path (same as thumbnails/faces).

### Status fields (existing sidecar table)

| Kind | Variant | Meaning |
|------|---------|---------|
| `embeddings` | `scene` | Per-content scene embedding readiness |
| (optional) extension model | `SceneCluster` | Cluster identity with centroid |

Query can extend `media.derivativeStatus` with `scene_embedding`.

## Clustering algorithm

Reuse face approach: **cosine distance + DBSCAN** (then optionally HDBSCAN).

- Normalize vectors L2
- Distance = `1 - cosine_similarity`
- Typical eps ≈ 0.25–0.40 for CLIP (tune on user library)
- min_samples ≈ 3–5 for albums

Core implements pure-Rust clustering so WASM extensions and host jobs share one kernel.

## GPU acceleration

| Stage | Backend |
|-------|---------|
| Embedding | ORT `CUDAExecutionProvider` → CPU fallback |
| Clustering | CPU (vector math); optional CUDA later for very large N |

WSL Docker GPU compose + `NVIDIA_DRIVER_CAPABILITIES=compute` already covers ORT.

## Deliverables in this change set

1. Core `ops/media/clustering` — cosine DBSCAN for any embedding list  
2. Async derivative status for `embeddings/scene`  
3. Photos extension: real face DBSCAN; scene clustering job scaffold  
4. This design doc + model download notes  

## Non-goals (this PR)

- Full ORT CLIP inference runtime (depends on packaging ONNX weights + host AI module)  
- UI scene album browser (consumes clusters once embeddings exist)
