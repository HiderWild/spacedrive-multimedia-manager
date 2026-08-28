# Spacedrive Core v2 Development Guide

## Quick Start

### Development Workflow

1. Start daemon: `.\scripts\invoke-spacedrive-cargo.ps1 run --bin sd-daemon`
2. Make code changes
3. Run tests: `.\scripts\invoke-spacedrive-cargo.ps1 test`
4. Rebuild and restart: `.\scripts\invoke-spacedrive-cargo.ps1 run --bin sd-cli -- restart`
5. Test via CLI: `.\scripts\invoke-spacedrive-cargo.ps1 run --bin sd-cli -- <command>`

### Common Commands

```powershell
.\scripts\invoke-spacedrive-cargo.ps1 build
.\scripts\invoke-spacedrive-cargo.ps1 test
.\scripts\invoke-spacedrive-cargo.ps1 test <test_name>
.\scripts\invoke-spacedrive-cargo.ps1 clippy
.\scripts\invoke-spacedrive-cargo.ps1 fmt
.\scripts\invoke-spacedrive-cargo.ps1 run --bin sd-cli -- <command>
```

The binary name is `sd-cli`, not `spacedrive`.

### Cargo build artifact policy

Do not run bare `cargo` commands in this repository. Every Cargo operation that may compile, including `build`, `test`, `run`, `check`, `clippy`, `bench`, `doc`, and `xtask`, must use `scripts/invoke-spacedrive-cargo.ps1` or an entry point that explicitly integrates the same policy.

The policy has one artifact directory: `<main-worktree>\target`. It obtains the main root from the first existing, non-prunable record returned by `git worktree list --porcelain`. Do not set or reuse a user-global `CARGO_TARGET_DIR`, pass a separate target directory, or keep a second profile tree.

Before each compile-producing Cargo command, the wrapper:

1. Acquires the repository build lock derived from the canonical git common directory.
2. Enumerates every registered worktree without pruning records.
3. Deletes only each worktree's exact `target` and `apps\tauri\src-tauri\target` candidates after safety checks.
4. Records missing and prunable worktrees as skipped.
5. Sets `CARGO_TARGET_DIR` to `<main-worktree>\target` and holds the lock until Cargo exits.

Any cleanup failure stops the command. The policy rejects worktree roots, git directories, drive and user roots, external paths, and reparse points. Do not use `-KeepOtherProfile`, `-TargetDir`, a global `CARGO_TARGET_DIR`, or a cache tool to preserve or create another repository artifact tree.

When you need a manual PowerShell flow, import the documented policy and call its guarded function. Do not reconstruct the cleanup commands yourself:

```powershell
. .\scripts\build-policy.ps1
$repoRoot = git rev-parse --show-toplevel
$code = Invoke-SpacedriveCargo -RepoRoot $repoRoot -CargoArguments @('test', '-p', 'sd-core')
if ($code -ne 0) { exit $code }
```

### Async media derivatives (add ≠ generate)

Photo **add / watcher / index** paths must not generate thumbnails or face vectors inline.

- Status lives on `sidecar.status` (`pending` | `ready` | `failed`)
  - thumbnail: `kind=thumb`, variant e.g. `grid@1x`
  - face embedding: `kind=embeddings`, variant `face`
  - scene embedding: `kind=embeddings`, variant `scene` (CLIP/DINO for visual clustering)

### Scene embedding (GPU)

- Feature flags: `scene-embed` (ORT CPU), `scene-embed-cuda` (ORT + CUDA EP)
- Backends: OpenCLIP ViT-B/32 (default), DINOv2 ViT-B/14, histogram baseline
- Env: `SD_SCENE_EMBED_BACKEND=openclip|dinov2|histogram`, `SD_SCENE_EMBED_MAX_CONCURRENT=2`
- Weights: `{data_dir}/models/image_embedding/{openclip-vit-b-32,dinov2-vit-b-14}.onnx`
- Job: `SceneEmbedJob` drains pending `embeddings/scene` sidecars
- Eval: `ops::media::scene_embed::eval::evaluate_backends()` + `scripts/bench-scene-embed.ps1`
- Helpers: `ops::media::derivative_queue`
  - `schedule_derivative_enqueue` — mark pending + debounced job flush (watcher path)
  - `enqueue_thumbnails_for_entries` — batch mark + dispatch one job
  - `derivative_status_for_content` — snapshot of readiness
- Query: `media.derivativeStatus` (`DerivativeStatusQuery`) for UI polling
- Watcher `run_processors` only runs content hash + schedule; `ThumbnailJob` drains the queue
- Bulk index (Content/Deep) batch-enqueues thumbs after discovery, non-blocking

### Bulk import / thumbnail throttling (Docker & WSL)

Bulk photo import used to spawn unbounded concurrent decodes and multi-size thumbnails, which OOMs 2G containers and freezes the host. Defaults are now conservative:

- Import thumbnails: only `grid@1x` (see `ThumbnailVariants::import_defaults`)
- Job defaults: `batch_size=16`, `max_concurrent=2` (overridable)
- Watcher default: `thumbstrip` **disabled** (opt-in; ~6s per video)

Environment knobs (also set in `apps/server/docker-compose.yml`):

```bash
SD_THUMB_MAX_CONCURRENT=2   # in-flight thumbnail generations
SD_THUMB_BATCH_SIZE=16      # discovery batch size before process loop
FFMPEG_PATH=/path/to/ffmpeg # optional NVENC-capable binary (also accepts FFMPEG=)
```

### WSL2 Docker + NVIDIA GPU

1. Host: current NVIDIA driver with WSL support; in WSL run `nvidia-smi`.
2. Install NVIDIA Container Toolkit; verify:
   `docker run --rm --gpus all nvidia/cuda:12.4.1-base-ubuntu22.04 nvidia-smi`
3. CPU image (default, higher memory limit for import):
   `docker compose -f apps/server/docker-compose.yml up -d --build`
4. GPU overlay (CUDA runtime image + device reservation):
   `docker compose -f apps/server/docker-compose.yml -f apps/server/docker-compose.gpu.yml up -d --build`
5. Confirm: `docker compose exec spacedrive nvidia-smi`
6. Operator guide: `apps/server/README-GPU.md`
7. NVENC example overlay: `apps/server/docker-compose.nvenc.example.yml`
8. Import results template: `docs/superpowers/plans/2026-07-11-import-bench-report.md`

Notes: stock Debian/Ubuntu `ffmpeg` usually lacks `h264_nvenc`. GPU passthrough mainly enables future AI/ORT/Whisper and any host-side NVENC ffmpeg you install. **JPEG/HEIC thumbnails remain CPU-bound** unless you enable the optional turbojpeg path; use concurrency limits above to stop import freezes.

Optional fast JPEG (libjpeg-turbo) feature chain:

```powershell
# Host build (needs libturbojpeg + NASM/cmake on some platforms)
.\scripts\invoke-spacedrive-cargo.ps1 build -p sd-server --features heif,ffmpeg,turbojpeg

# GPU image enables turbojpeg by default (see apps/server/Dockerfile.gpu)
```

Offline decode/resize micro-bench (not a full daemon import):

```powershell
./scripts/bench-thumbnail-import.ps1 -Count 100 -Concurrency 2
./scripts/bench-thumbnail-import.ps1 -Count 100 -Concurrency 8
```

Server logs thumbnail knobs and ffmpeg HW encoder presence at startup (`log_import_runtime_hints`).

### Disk / target cache control

Rust artifacts live only in the main worktree's `target` directory. The guarded wrapper clears every registered worktree artifact candidate before compile-producing commands, so do not offload, preserve, or independently clean repository targets.

Do not set workspace-wide `incremental = false` for daily coding; it saves disk but makes rebuilds much slower. Release profile already disables incremental.

Profiles of note in root `Cargo.toml`:

- `dev`: `debug = 0` (smaller target; use `dev-debug` for full debugger info)
- `release`: `strip = true`, `opt-level = "s"`, `lto = "thin"`, `incremental = false`

### Common Mistakes

- Running `spacedrive` instead of `sd-cli` (the binary name is `sd-cli`)
- Forgetting to restart daemon after rebuilding
- Using `println!` instead of `tracing` macros (`info!`, `debug!`, etc)
- Implementing `Wire` manually instead of using `register_*` macros
- Blocking the async runtime with synchronous I/O operations

### Quick tips

- On frontend apps, such as the interface in React, you must ALWAYS ensure type-safety based on the auto generated TypeScript types from `ts-client`. Never cast to as any or redefine backend types. our hooks are typesafe with correct input/output types, but sometimes you might need to access types directly from the `ts-client`.
- If you have changed types on the backend that are public to the frontend (have `Type` derive), then you must regenerate the types using `.\scripts\invoke-spacedrive-cargo.ps1 run --bin generate_typescript_types` and commit the updated `packages/ts-client/src/generated/types.ts`. CI runs `scripts/check-ts-types.sh` and fails if the committed types drift from Rust. Use that entry only after it integrates this same build policy.
- Read the `.mdx` files in /docs for context on any part of the app, they are kept up to date.
-

## Architecture Overview

Spacedrive uses daemon-client architecture. A single daemon process manages core functionality. Multiple clients (CLI, GraphQL server, desktop app) connect via Unix domain sockets.

### CQRS and DDD Pattern

- **Domain** (`src/domain/`): Core data structures and business logic (nouns)
- **Operations** (`src/ops/`): Actions and queries (verbs)
- **Actions**: State-changing operations (writes)
- **Queries**: Data retrieval without state changes (reads)

### Feature Module Structure

Each feature lives in its own module under `src/ops/`. Example: `src/ops/files/share`

```
src/ops/files/share/
├── action.rs      # State-changing logic
├── input.rs       # Action input structures
├── output.rs      # Action output structures
└── job.rs         # Long-running job implementation (if needed)
```

Complete feature example:

```rust
// src/ops/files/share/input.rs
#[derive(Debug, Serialize, Deserialize)]
pub struct ShareFileInput {
    pub file_id: i32,
    pub recipient: String,
}

// src/ops/files/share/output.rs
#[derive(Debug, Serialize, Deserialize)]
pub struct ShareFileOutput {
    pub share_id: String,
    pub url: String,
}

// src/ops/files/share/action.rs
use super::{ShareFileInput, ShareFileOutput};

pub struct ShareFileAction;

crate::register_library_action!(ShareFileAction, "files.share");

impl Action for ShareFileAction {
    type Input = ShareFileInput;
    type Output = ShareFileOutput;

    async fn run(input: Self::Input, ctx: &ActionContext) -> Result<Self::Output> {
        // Implementation
    }
}
```

## Communication Architecture

Spacedrive supports multiple communication patterns for different platforms and use cases.

### Daemon-Client Communication (Tauri Desktop, CLI, Web)

The Tauri desktop app, CLI, and web interface connect to a daemon process via Unix domain sockets (or WebSockets for web). Communication uses JSON-RPC 2.0 with Wire method strings.

**Registration Macros:**

Never implement `Wire` manually. Use registration macros:

```rust
// Queries
crate::register_query!(NetworkStatusQuery, "network.status");
// Generates: "query:network.status"

// Library Actions
crate::register_library_action!(FileCopyAction, "files.copy");
// Generates: "action:files.copy.input"

// Core Actions
crate::register_core_action!(LibraryCreateAction, "libraries.create");
// Generates: "action:libraries.create.input"
```

**Registry System:**

The `inventory` crate collects operations at compile time. When you use `register_query!` or `register_library_action!`, the operation automatically appears in global `QUERIES` and `ACTIONS` hashmaps at startup. You never manually register operations.

Location: `core/src/ops/registry.rs`

### Tauri Desktop Development

The Tauri app (`apps/tauri/`) is the primary desktop application for Spacedrive. It connects to the daemon via the TypeScript client.

**Development Workflow:**

```bash
# Install dependencies
bun install
```

Tauri development and production commands may compile Rust. Run them only after their entry points explicitly integrate `scripts/build-policy.ps1`, including the same cleanup, shared target, and lock. Until then, use `scripts/invoke-spacedrive-cargo.ps1` for Rust operations and do not bypass the policy through the Tauri CLI.

**TypeScript Client:**

The TypeScript client (`packages/ts-client/`) is auto-generated from Rust types using Specta:

```powershell
# Generate TypeScript types
.\scripts\invoke-spacedrive-cargo.ps1 run --bin generate_typescript_types
```

**Output:** `packages/ts-client/src/generated.ts`

**Architecture:**

```
Tauri App (React)
    ↓
@sd/ts-client (TypeScript)
    ↓
Daemon (Unix Socket / IPC)
    ↓
RpcServer (Rust)
    ↓
Operation Registry
```

### Native Prototypes (iOS, macOS)

**Note:** iOS and macOS apps are experimental prototypes, not production apps.

Native prototypes embed the core directly as a library via FFI rather than connecting to a daemon. These are located in `apps/ios/` and `apps/macos/` but are private and not documented for public use.

**Swift Client Generation:**

For the prototypes, Swift types can be generated:

```powershell
.\scripts\invoke-spacedrive-cargo.ps1 run --bin generate_swift_types
```

Output: `packages/swift-client/Sources/SpacedriveClient/`

### Extension System (WASM)

Extensions run as sandboxed WASM modules that interact with Spacedrive core via host functions. Extensions are distributed as compiled `.wasm` files.

**Architecture:**

```
Extension.wasm (compiled Rust)
    ↓
spacedrive-sdk (Rust crate)
    ↓
Host Functions (FFI boundary)
    ↓
Core (VDFS, Jobs, AI, etc.)
```

**Key Components:**

**SDK Location:** `crates/sdk/`

- High-level Rust API abstracting FFI details
- Procedural macros for extension definition
- Type-safe job, model, and action builders

**Extension Development:**

Extensions use procedural macros to minimize boilerplate:

```rust
use spacedrive_sdk::prelude::*;

#[extension(
    id = "test-extension",
    name = "Test Extension",
    version = "0.1.0",
    jobs = [test_counter],
)]
struct TestExtension;

#[derive(Serialize, Deserialize, Default)]
pub struct CounterState {
    pub current: u32,
    pub target: u32,
    pub processed: Vec<String>,
}

#[job(name = "counter")]
fn test_counter(ctx: &JobContext, state: &mut CounterState) -> Result<()> {
    ctx.log(&format!("Starting counter (current: {}, target: {})",
        state.current, state.target));

    while state.current < state.target {
        if ctx.check_interrupt() {
            ctx.checkpoint(state)?;
            return Err(Error::OperationFailed("Interrupted".into()));
        }

        state.current += 1;
        ctx.report_progress(
            state.current as f32 / state.target as f32,
            &format!("Counted {}/{}", state.current, state.target),
        );

        if state.current % 10 == 0 {
            ctx.checkpoint(state)?;
        }
    }

    Ok(())
}
```

**Host Functions:**

Extensions import minimal FFI functions:

```rust
#[link(wasm_import_module = "spacedrive")]
extern "C" {
    fn spacedrive_log(level: u32, msg_ptr: *const u8, msg_len: usize);
    fn register_job(
        job_name_ptr: *const u8,
        job_name_len: u32,
        export_fn_ptr: *const u8,
        export_fn_len: u32,
        resumable: u32,
    ) -> i32;
}
```

**Building Extensions:**

```powershell
# From the repository root
.\scripts\invoke-spacedrive-cargo.ps1 build --manifest-path extensions\test-extension\Cargo.toml --target wasm32-unknown-unknown --release

# Output: <main-worktree>\target\wasm32-unknown-unknown\release\extension_name.wasm
```

**Extension Capabilities:**

Extensions can define:

- Models: Data structures stored in `models` table (content-scoped, standalone, or entry-scoped)
- Jobs: Long-running resumable operations
- Actions: User-invoked operations with preview-commit workflow
- Agents: Autonomous logic with memory and event handling
- UI: Custom views via `ui_manifest.json`

**Example Use Cases:**

- Photos extension: Face detection, scene tagging, album organization
- Finance extension: Receipt extraction, expense tracking
- Research extension: Citation extraction, knowledge graphs

**Key Benefits:**

- Single `.wasm` file works on all platforms
- True sandboxing (WASM isolation)
- Resumable jobs with checkpointing
- Type-safe API with procedural macros
- No core modifications needed for new features

**Documentation:**

- `/docs/sdk/sdk.md` - Complete SDK specification and API reference
- `extensions/test-extension/` - Working example extension
- `crates/sdk/` - SDK implementation
- `crates/sdk-macros/` - SDK procedural macros

**Status:** SDK implementation in progress. Test extension compiles to WASM successfully. Core integration for loading and executing WASM modules is next phase.

## Code Standards

### Import Organization

Group imports with blank lines between groups:

```rust
// Standard library
use std::path::PathBuf;
use std::sync::Arc;

// External crates
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// Local modules
use crate::domain::library::Library;
use crate::ops::Action;
```

### Naming Conventions

- Functions/variables: `snake_case`
- Types: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`

### Error Handling

Use `Result<T, E>` for all fallible operations. Use `thiserror` for custom errors, `anyhow` for application errors.

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShareError {
    #[error("File not found: {0}")]
    FileNotFound(i32),

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

pub async fn share_file(id: i32) -> Result<String, ShareError> {
    let file = find_file(id).await.ok_or(ShareError::FileNotFound(id))?;
    // Implementation
    Ok(share_url)
}
```

### Async Code

- Use `async/await` syntax
- Prefer `tokio` primitives (`tokio::sync::RwLock`, `tokio::spawn`)
- Avoid blocking operations (use `tokio::fs` not `std::fs`)
- Use `tokio::task::spawn_blocking` for CPU-intensive work

### Resumable Jobs

Store job state within the job struct. Use `#[serde(skip)]` for non-persistent fields.

```rust
#[derive(Serialize, Deserialize)]
pub struct FileCopyJob {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub copied_files: Vec<PathBuf>,  // Persisted for resumability

    #[serde(skip)]
    pub progress_tx: Option<tokio::sync::mpsc::Sender<Progress>>,  // Not persisted
}

impl Job for FileCopyJob {
    async fn run(&mut self, ctx: &JobContext) -> Result<()> {
        ctx.log().info("Starting file copy job");

        for file in &self.files_to_copy {
            if self.copied_files.contains(file) {
                continue;  // Skip already copied files on resume
            }

            copy_file(file).await?;
            self.copied_files.push(file.clone());
        }

        Ok(())
    }
}
```

### Documentation

**Core principle:** Explain WHY, not WHAT. Keep comments as short as possible. One sentence explaining rationale beats a paragraph restating code.

**Module docs (`//!`):**
- Add a title with `#` for the module name
- Explain what the module does in plain language (not bullet points)
- Include design rationale naturally in prose
- Add runnable code examples showing usage

````rust
//! # File Sharing System
//!
//! `core::ops::files::share` provides temporary file sharing via signed URLs.
//! Share links expire after 7 days by default to prevent indefinite access to
//! private files. UUID v5 deterministic IDs ensure the same file generates
//! consistent share URLs across devices without coordination.
//!
//! ## Example
//! ```rust,no_run
//! use spacedrive_core::ops::files::share::{ShareFileAction, ShareFileInput};
//!
//! let input = ShareFileInput { file_id: 123, recipient: "user@example.com" };
//! let output = ShareFileAction::run(input, &ctx).await?;
//! ```
````

**Function docs (`///`):**
- First line: brief one-liner
- Second paragraph: explain design rationale and why this exists
- Document error handling philosophy when relevant
- Explain non-obvious behavior and platform differences

```rust
/// Creates a share link with automatic expiration.
///
/// Share links use signed JWTs so the daemon can validate them without
/// database lookups on every request. Expiration is enforced server-side
/// to prevent timezone manipulation. Recipients without library access
/// get read-only access to the specific file only.
///
/// Returns `ShareError::PermissionDenied` if the file is private and
/// the recipient isn't a library member. The share is still created
/// but marked inactive for audit logging.
pub async fn share_file(input: ShareFileInput) -> Result<ShareFileOutput>
```

**Inline comments:**
- Delete comments that restate obvious code
- Explain WHY for decisions, not WHAT the code does
- Use one sentence when possible
- Only expand for truly non-obvious consequences

```rust
// Good: explains WHY
// Lowercase for case-insensitive search matching.
let ext = path.extension().map(|e| e.to_lowercase());

// Bad: restates code
// Extract file extension and convert to lowercase
let ext = path.extension().map(|e| e.to_lowercase());

// Good: explains consequence
// Preserve ephemeral UUIDs so tags attached during browsing survive promotion to managed location.
let uuid = ephemeral_cache.get(path).unwrap_or_else(|| Uuid::new_v4());

// Bad: verbose explanation of obvious behavior
// UUID assignment strategy:
// 1. First check if there's an ephemeral UUID
// 2. If not, generate a new one
let uuid = ephemeral_cache.get(path).unwrap_or_else(|| Uuid::new_v4());
```

**Error handling comments:**
Explain strategy and recovery, not just "log and continue".

```rust
// Good: explains recovery
// Best-effort: continue with remaining moves, stale paths cleaned up on next reindex.
Err(e) => ctx.log(format!("Failed to move: {}", e)),

// Bad: states the obvious
// Log error but continue
Err(e) => ctx.log(format!("Failed to move: {}", e)),
```

**Platform-specific comments:**
Explain consequences, not implementation blockers.

```rust
// Good: explains why and fallback
#[cfg(windows)]
pub fn get_inode(_metadata: &std::fs::Metadata) -> Option<u64> {
    // Windows file indices are unstable across reboots; fall back to path-only matching.
    None
}

// Bad: over-explains implementation details
#[cfg(windows)]
pub fn get_inode(_metadata: &std::fs::Metadata) -> Option<u64> {
    // Windows doesn't have inodes.
    // The method `file_index()` is unstable (issue #63010).
    // Returning None is safe as the field is Optional.
    None
}
```

**Never use:**
- Placeholder comments ("for now", "TODO: extract this later")
- Markdown formatting (`**bold**`, `_italic_`) in code comments
- ASCII diagrams (put those in `/docs/` if needed)
- Section divider comments (`// ========== Section ==========`)
- Comments explaining removed code during refactors

Track future work in GitHub issues, not code comments.

### Formatting

Run `.\scripts\invoke-spacedrive-cargo.ps1 fmt` before committing. Tabs for indentation. No emojis.

## Logging

### Setup

Use `tracing_subscriber` in main or examples:

```rust
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("sd_core=info"))
        )
        .init();
}
```

## Writing Style

This applies to all documentation, code comments, and design documents.

Use clear, simple language. Write short, impactful sentences. Use active voice. Focus on practical, actionable information.

Address the reader directly with "you" and "your". Support claims with data and examples when possible.

Avoid these constructions:

- Em dashes (use commas or periods)
- "Not only this, but also this"
- Metaphors and cliches
- Generalizations
- Setup language like "in conclusion"
- Unnecessary adjectives and adverbs
- Emojis, hashtags, markdown formatting in prose

Avoid these words:
comprehensive, delve, utilize, harness, realm, tapestry, unlock, revolutionary, groundbreaking, remarkable, pivotal

### Macros

Use `tracing` macros, never `println!`:

```rust
use tracing::{info, warn, error, debug};

info!("Server started on port {}", port);
debug!(file_id = %id, "Processing file");
warn!(error = %e, "Retrying operation");
error!("Failed to connect to database");
```

### Job Logging

Use `ctx.log()` in job implementations for automatic `job_id` tagging:

```rust
impl Job for MyJob {
    async fn run(&mut self, ctx: &JobContext) -> Result<()> {
        ctx.log().info("Job started");
        ctx.log().debug!(progress = %self.progress, "Processing");
        Ok(())
    }
}
```

### Log Levels

- `debug`: Detailed flow for troubleshooting
- `info`: User-relevant events (server start, job completion)
- `warn`: Recoverable issues (retry, fallback)
- `error`: Failures requiring attention

### Environment Control

Use `RUST_LOG` environment variable:

```powershell
$env:RUST_LOG = 'debug'; .\scripts\invoke-spacedrive-cargo.ps1 run --bin sd-cli
$env:RUST_LOG = 'sd_core=trace'; .\scripts\invoke-spacedrive-cargo.ps1 run
$env:RUST_LOG = 'sd_core::ops=debug'; .\scripts\invoke-spacedrive-cargo.ps1 run
```

## Testing

### Test Organization

- Unit tests: Colocated in `#[cfg(test)]` modules
- Integration tests: `tests/` directory at crate root

```rust
// src/ops/files/share/action.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_share_file() {
        let input = ShareFileInput {
            file_id: 1,
            recipient: "test@example.com".to_string(),
        };

        let output = share_file(input).await.unwrap();
        assert!(!output.share_id.is_empty());
    }
}
```

### Running Tests

```powershell
.\scripts\invoke-spacedrive-cargo.ps1 test
.\scripts\invoke-spacedrive-cargo.ps1 test test_share_file
.\scripts\invoke-spacedrive-cargo.ps1 test --lib
.\scripts\invoke-spacedrive-cargo.ps1 test -- --nocapture
```

## Task Tracking

Spacedrive uses a file-based task system in `/.tasks/` to track features, epics, and development work. All task files are version-controlled alongside the code.

### When to Create Tasks

Create tasks for work that:

- Introduces a new feature or capability
- Refactors a significant system or module
- Fixes a bug requiring architectural changes
- Implements a whitepaper specification

Do not create tasks for:

- Routine code formatting or style fixes
- Trivial bug fixes (single line changes)
- Documentation updates to existing features
- Dependency version bumps

### Task Structure

Each task is a Markdown file: `CATEGORY-###-title-slug.md`

```yaml
---
id: CORE-042
title: "Implement file sharing API"
status: "In Progress"
assignee: "james"
priority: "High"
tags: ["core", "networking"]
whitepaper: "Section 4.2" # And/or design_doc: DESIGN_DOC_NAME.md
---

## Description
Brief overview of what needs to be done and why.

## Implementation Steps
- [ ] Create share action in src/ops/files/share
- [ ] Add database schema for shares table
- [ ] Implement expiration logic

## Acceptance Criteria
- Share links work across all platforms
- Expired shares return 404
- Tests cover edge cases
```

### Managing Tasks

```powershell
# List your active tasks
.\scripts\invoke-spacedrive-cargo.ps1 run -p task-validator -- list --assignee "yourname" --status "In Progress"

# List high priority tasks
.\scripts\invoke-spacedrive-cargo.ps1 run -p task-validator -- list --priority "High" --sort-by id

# Validate before committing (automatic via git hook)
.\scripts\invoke-spacedrive-cargo.ps1 run -p task-validator -- validate
```

### Task Lifecycle

1. Create task file in `/.tasks/` with `status: "To Do"`
2. Update status to `"In Progress"` when you start work
3. Complete implementation and tests
4. Update status to `"Done"` and commit

Full documentation: `/docs/core/task-tracking.md`

## Debugging

### Log Files

Job logs live in the `job_logs` directory in the data folder root.

### Daemon Restart

After rebuilding, restart the daemon to use the latest code:

```powershell
.\scripts\invoke-spacedrive-cargo.ps1 build
.\scripts\invoke-spacedrive-cargo.ps1 run --bin sd-cli -- restart
```

### Verbose Logging

```powershell
$env:RUST_LOG = 'debug'; .\scripts\invoke-spacedrive-cargo.ps1 run --bin sd-daemon
$env:RUST_LOG = 'sd_core::jobs=trace'; .\scripts\invoke-spacedrive-cargo.ps1 run
```

## Documentation Locations

- Core architecture: `/docs/core/`
- Design docs and RFCs: `/docs/core/design/`
- Application docs: `/docs/`
- Daemon details: `/docs/core/daemon.md`
- Task tracking: `/docs/core/task-tracking.md`
