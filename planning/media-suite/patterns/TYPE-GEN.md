# TypeScript Type-Generation Gate

Canonical, repo-verified workflow for keeping the generated TypeScript client in
sync with Rust `Type`-deriving structs. Established by task **G-02** so every
later media-suite epic that touches frontend-facing Rust types regenerates and
ships matching TS, and so CI fails on drift.

## What the system is

- **Generator:** a Rust binary,
  [core/src/bin/generate_typescript_types.rs](../../../core/src/bin/generate_typescript_types.rs),
  built on Specta plus the repo's rspc-inspired type extraction in
  `sd_core::infra::wire::type_extraction`.
- **What it reads:** all operations/queries registered through the `register_*`
  macros, plus every struct/enum that derives `Type` (specta) and is reachable
  from them.
- **What it writes:** a single file,
  [packages/ts-client/src/generated/types.ts](../../../packages/ts-client/src/generated/types.ts).
  The file is written **in place** (overwritten each run) and is committed to
  the repo. Its header says `DO NOT EDIT`.
- **Consumers:** `@sd/ts-client`, the Tauri app, and the web interface all rely
  on these types for end-to-end type safety.

## When to regenerate

Regenerate whenever you change a Rust type that is public to the frontend,
i.e. anything that derives `Type` and is reachable from a registered action or
query: input/output structs, shared domain enums, etc. Adding, removing, or
renaming a field or variant all require regeneration.

## The exact command

```bash
cargo run --bin generate_typescript_types --manifest-path core/Cargo.toml
```

Equivalent shortcuts:

```bash
just generate-types                       # justfile recipe
bun run --filter @sd/ts-client generate-types   # package.json script
```

Output lands at `packages/ts-client/src/generated/types.ts`. Commit that file
in the same change as the Rust type edit.

## The drift check

[scripts/check-ts-types.sh](../../../scripts/check-ts-types.sh) detects when the
committed TS is stale relative to the Rust types. It:

1. Snapshots the committed `types.ts` to a temp file.
2. Runs the generator (which rewrites `types.ts` in place).
3. Diffs the regenerated file against the snapshot.
4. **Restores the original** via an `EXIT` trap, so the check never leaves a
   spurious diff in the working tree (true for both the pass and fail paths).
5. Exits `0` when in sync, or `1` and prints the drift diff plus the fix command.

Run it locally:

```bash
./scripts/check-ts-types.sh     # or: just check-types
```

### CI wiring

The check runs in the `clippy` job of
[.github/workflows/ci.yml](../../../.github/workflows/ci.yml), right after
Clippy, as the step **"Check TypeScript types are in sync with Rust"**. That job
already sets up the system + Rust toolchain and compiles the workspace, so the
generator reuses those artifacts. A PR that edits a `Type` struct without
regenerating fails this step.

## Fixing a drift failure

When the check (or CI) fails:

```bash
cargo run --bin generate_typescript_types --manifest-path core/Cargo.toml
git add packages/ts-client/src/generated/types.ts
git commit
```

## Verification (G-02)

The script was verified end-to-end by substituting a stub generator on `PATH`
(the real generator requires building core):

- **In sync:** stub leaves `types.ts` unchanged → script prints
  `TypeScript types are in sync with Rust.`, exits `0`, working tree clean.
- **Drift:** stub appends a line (simulating a new Rust field) → script prints
  `TypeScript types are OUT OF SYNC ...`, shows the unified diff, exits `1`, and
  the `EXIT` trap restores `types.ts` so `git status` stays clean.

To reproduce with the real toolchain, run `./scripts/check-ts-types.sh` on a
clean tree (passes), then change a `Type`-deriving struct in `core/` without
regenerating and run it again (fails).
