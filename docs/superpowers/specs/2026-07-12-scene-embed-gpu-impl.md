# Scene Embedding GPU Implementation

**Date:** 2026-07-12  
**Spec:** `docs/superpowers/specs/2026-07-12-scene-clustering-design.md`  
**Status:** Core pipeline + GPU scaffolding complete; ONNX weights + CLI wiring pending

## What was built

### Core (`sd-core`)

| Module | Purpose |
|--------|---------|
| `ops/media/clustering.rs` | Cosine DBSCAN (face + scene), L2 normalize, unit tests |
| `ops/media/scene_embed/mod.rs` | Module hub |
| `ops/media/scene_embed/backend.rs` | Multi-backend embed: OpenCLIP / DINOv2 ONNX (GPU via ORT CUDA EP), histogram baseline |
| `ops/media/scene_embed/preprocess.rs` | Image → NCHW f32 (ImageNet mean/std), RGB histogram fingerprint |
| `ops/media/scene_embed/eval.rs` | Horizontal eval: latency p95, cluster count, NN label accuracy, device detection |
| `ops/media/scene_embed/job.rs` | `SceneEmbedJob`: drains pending `embeddings/scene` sidecars, writes vectors to disk, flips status |
| `ops/models/image_embedding.rs` | Backend catalog: OpenCLIP ViT-B/32, DINOv2 ViT-B/14, histogram; env override `SD_SCENE_EMBED_BACKEND` |
| `ops/models/types.rs` | `ModelType::ImageEmbedding` variant |
| `ops/media/derivative_queue.rs` | `SCENE_EMBEDDING_VARIANT`, `enqueue_derivatives_for_entry_ext(want_scene)` |
| `ops/media/derivative_status_query.rs` | `scene_embedding` field in `DerivativeStatusItem` |
| `infra/job/output.rs` | `JobOutput::SceneEmbedding` variant |

### Cargo features

```toml
scene-embed = ["dep:ort"]           # ONNX Runtime, CPU
scene-embed-cuda = ["scene-embed", "ort/cuda"]  # ONNX Runtime + CUDA EP
```

### GPU acceleration path

```
SceneEmbedJob
  → load image (tokio)
  → spawn_blocking → ORT session (CUDA EP registered first, CPU fallback)
  → NCHW preprocess → session.run() → pool → L2 normalize
  → write MsgPack to sidecar file → update DB status=ready
```

CUDA EP registration (from `backend.rs`):
```rust
#[cfg(feature = "scene-embed-cuda")]
{
    use ort::ep::CUDA;
    match builder.with_execution_providers([CUDA::default().build()]) {
        Ok(b) => { builder = b; device = Cuda; }
        Err(e) => { warn!(...); device = Cpu; }  // graceful fallback
    }
}
```

### Horizontal evaluation

`eval.rs` runs all three backends on the same image set and reports:

| Metric | Description |
|--------|-------------|
| `mean_latency_ms` | Average embed time |
| `p95_latency_ms` | 95th percentile |
| `device` | CUDA / CPU / baseline (actual runtime) |
| `cluster_count` | DBSCAN cluster count at default eps |
| `nn_label_accuracy` | Fraction of nearest-neighbors sharing ground-truth label |

Script: `scripts/bench-scene-embed.ps1`

## How to use

### 1. Build with GPU support

```bash
# CPU-only (ORT works everywhere)
cargo build --bin sd-cli --features scene-embed

# GPU (NVIDIA + CUDA toolkit required)
cargo build --bin sd-cli --features scene-embed-cuda
```

### 2. Place ONNX model weights

```
~/.spacedrive/models/image_embedding/
  ├── openclip-vit-b-32.onnx    # ~150-350MB
  └── dinov2-vit-b-14.onnx      # ~330MB
```

Export from HuggingFace:
- OpenCLIP: `openai/clip-vit-base-patch32` → export vision tower to ONNX
- DINOv2: `facebook/dinov2-base` → export to ONNX

### 3. Run evaluation

```powershell
./scripts/bench-scene-embed.ps1 -Images ./test-photos -DataDir ~/.spacedrive
```

### 4. Run SceneEmbedJob (production)

The job drains all `embeddings/scene` sidecars with status `pending`:
```
SD_SCENE_EMBED_BACKEND=openclip-vit-b-32   # or dinov2 / histogram
SD_SCENE_EMBED_MAX_CONCURRENT=2            # GPU memory limit
```

After embeddings are `ready`, run `cluster_scenes` (photos extension) to
generate `#scene_cluster:*` tags.

## What's still needed

1. **CLI subcommand** `scene-embed-eval` to invoke `evaluate_backends()` from the bench script
2. **Model download automation** (extend `ops/models/download.rs` for image_embedding)
3. **Watcher integration** to auto-dispatch `SceneEmbedJob` after import (like thumbnails)
4. **UI** to show scene cluster albums
