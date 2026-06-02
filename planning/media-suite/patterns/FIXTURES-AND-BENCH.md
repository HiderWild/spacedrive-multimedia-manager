# Media test fixtures & benchmark harness (G-03)

Shared fixtures and a benchmark harness for the media suite. Used by transcode
(B-01), batch rotation (B-04), and effective-tag resolution (A-05).

Nothing here downloads from the network, and no binary blobs are committed.
Image fixtures are generated in memory at test time; video fixtures are
synthesized with ffmpeg when it is available and skipped otherwise.

## Where it lives

| Path | Purpose |
|------|---------|
| `core/src/testing/media_fixtures.rs` | Generate image fixtures (PNG/JPEG, EXIF orientation, ICC) and synthesize video clips via ffmpeg. |
| `core/src/testing/media_bench.rs` | Dependency-free timing harness (`bench`, `bench_for`, `BenchStats`). |
| `core/tests/media_fixtures_test.rs` | Verifies fixtures decode and metadata round-trips. |
| `core/tests/media_bench_test.rs` | Sample `#[ignore]`d benchmarks (rotation, tag union, transcode). |

Both modules live under `core/src/testing/`, which is always compiled into
`sd-core`, so they are reachable from `core/tests/` integration tests **and**
from the `sd-bench` crate (which depends on `sd-core`).

## Fixtures: committed vs generated

Everything is **generated**, not committed.

### Images

`media_fixtures::default_image_specs()` describes a curated set spanning aspect
ratios and EXIF orientations:

- `landscape_16x9.png`, `portrait_9x16.png`, `square_1x1.jpg` — varied aspect
  ratios for layout work (C-02).
- `exif_orientation_1/6/8.jpg` — JPEGs carrying EXIF Orientation tags for
  rotation normalization (B-04).
- `icc_tagged.jpg` — JPEG with an embedded ICC profile to verify preservation
  (B-04).

Each fixture is a deterministic RGB gradient encoded with the `image` crate,
kept under ~8 KB. EXIF and ICC are injected by hand:

- `inject_exif_orientation(jpeg, n)` splices a minimal APP1 EXIF segment
  (big-endian TIFF, single Orientation tag) after the SOI marker.
- `inject_icc_profile(jpeg, bytes)` splices an APP2 `ICC_PROFILE` segment.
- `synthetic_icc_profile()` returns a tiny structurally-plausible ICC header
  (correct size + `acsp` signature, empty tag table). It is enough to test that
  a transform **preserves** the profile bytes; it is not color-managed. B-04 can
  substitute a real profile when color correctness matters.

Round-trip readers `read_exif_orientation()` and `read_icc_profile()` parse the
metadata back for verification.

Write the whole set to a directory:

```rust
use sd_core::testing::media_fixtures;

let dir = tempfile::tempdir()?;
let paths = media_fixtures::write_image_fixtures(dir.path())?;
```

### Video

Real codec data requires ffmpeg, which is **not** a build dependency. Clips are
synthesized at test time from ffmpeg's `testsrc` source (64x64, ~1s, a few KB):

```rust
use sd_core::testing::media_fixtures;

if media_fixtures::ffmpeg_available() {
    let clip = media_fixtures::synthesize_clip(dir.path(), "h264", 1)?;
    // or one clip per codec the local ffmpeg supports:
    let clips = media_fixtures::synthesize_all_clips(dir.path())?;
}
```

`ffmpeg_available()` shells out to `ffmpeg -version`. Always guard ffmpeg paths
with it; `synthesize_clip` returns `FixtureError::FfmpegMissing` when ffmpeg is
absent so suites can skip instead of fail. Supported codec names: `h264`,
`hevc`, `vp9`, `mpeg4`. Codecs the local ffmpeg build cannot encode are skipped
by `synthesize_all_clips`.

## Benchmark harness

The repo does not use criterion, so benchmarks run as ordinary `#[ignore]`d
release tests via the harness in `media_bench.rs`:

```rust
use sd_core::testing::media_bench::bench;

let stats = bench("rotate90", 200, || {
    let rotated = image::imageops::rotate90(&img);
    std::hint::black_box(rotated);
});
println!("{stats}"); // name, iters, mean/min/max, ops/s
```

- `bench(name, iters, op)` — fixed iteration count, returns `BenchStats`.
- `bench_for(name, duration, op)` — runs until a minimum wall-clock elapses
  (for very fast ops such as cache-hit resolution).

### Running the sample benchmarks

```bash
cargo test -p sd-core --release --test media_bench_test -- --ignored --nocapture
```

The samples benchmark trivial-but-real operations today (image rotation, tag-set
union, ffmpeg transcode) so the harness is runnable before the media tasks land.

## How later tasks plug in

- **B-01 (TranscodeJob):** use `synthesize_clip` / `synthesize_all_clips` for
  integration-test inputs; replace the closure in `bench_transcode_h264_to_vp9`
  with the real `TranscodeJob` to track encode throughput.
- **B-04 (batch rotation):** drive correctness tests with `exif_orientation_*`
  and `icc_tagged` fixtures (assert orientation normalizes and the ICC profile
  survives via `read_icc_profile`); replace the `bench_image_rotation` closure
  with the real rotate job.
- **A-05 (effective-tag cache):** replace `bench_tag_resolution_placeholder`
  with cached vs uncached resolution and use `bench_for` to show the >10x cache
  speedup the acceptance criteria require.

## Constraints honored

- No network downloads; no committed media binaries.
- ffmpeg-dependent code is runtime-guarded and skips gracefully when absent.
- Tabs, `cargo fmt`, `tracing` (not `println!`) in library code.
