# Spacedrive Local Build Adaptations

This document describes code modifications made to the Spacedrive open-source repository
to enable local building and running on Windows without access to the private `spaceui`
and `spacebot` packages.

## Why These Adaptations Are Needed

The Spacedrive open-source repository references two private packages that are not
included in the public codebase:

1. **`@spacebot/api-client`** — An AI chat/agent API client module. The open-source
   code imports this package in `SpacebotContext.tsx`, `VoiceOverlay.tsx`, and other
   Spacebot-related components, but the package itself is not published or included.

2. **`@spacedrive/ai`** — An AI interaction components package (part of SpaceUI). It
   provides `ModelSelector`, `MessageBubble`, `ChatComposer`, `InlineWorkerCard`, and
   other components used by Spacebot features. The package exists as a prebuilt dist in
   the bun store but was excluded from Vite's dependency optimization, causing CJS/ESM
   interop failures for its dependencies (specifically `style-to-js`).

Without these packages, the frontend JS crashes at runtime, resulting in a transparent
Tauri window (no visible content). The Tauri window is configured with `"transparent":
true` in `tauri.conf.json`, so if WebView content fails to render, the window appears
completely transparent.

## Changes Made

### 1. `apps/tauri/vite.config.ts` — Three modifications

**a) Added `@spacebot/api-client` alias fallback (lines 96-103)**

When `hasSpacebot` is `false` (the `spacebot` repo is not available locally), the
original config had an empty fallback array `[]`. We added an alias that redirects
`@spacebot/api-client` to a local stub file:

```typescript
...(hasSpacebot
    ? [{ find: /^@spacebot\/api-client$/, replacement: `${spacebot}/api-client/src` }]
    : [
        {
            find: /^@spacebot\/api-client$/,
            replacement: path.resolve(__dirname, './stubs/spacebot-api-client.ts'),
        },
    ]),
```

**Why**: Without this alias, `import { apiClient, ... } from '@spacebot/api-client'`
in `SpacebotContext.tsx` would fail to resolve, causing the entire frontend to crash.

**b) Removed `@spacedrive/ai` from `optimizeDeps.exclude` (line 123)**

Changed from:
```typescript
exclude: ['@spacedrive/ai', '@spacedrive/primitives', '@spacedrive/tokens']
```
To:
```typescript
exclude: ['@spacedrive/primitives', '@spacedrive/tokens', '@spacebot/api-client']
```

**Why**: `@spacedrive/ai` has CJS dependencies (`style-to-js`, `hast-util-to-jsx-runtime`,
`react-markdown`, etc.) that require Vite's pre-bundling (dependency optimization) to
properly handle CJS-to-ESM interop. When excluded, Vite served these modules via `@fs`
paths without CJS interop, causing `SyntaxError: does not provide an export named
'default'` at runtime. This was the root cause of the transparent window.

Also added `@spacebot/api-client` to `optimizeDeps.exclude` since it's resolved via
alias to a local TS stub and should not be pre-bundled.

**c) Removed `rollupOptions.external` for `@spacebot/api-client` (removed ~5 lines)**

The original config marked `@spacebot/api-client` as an external dependency when
`hasSpacebot` was false:
```typescript
rollupOptions: {
    external: [...(!hasSpacebot ? ['@spacebot/api-client'] : [])],
}
```

**Why**: `rollupOptions.external` tells Vite/Rollup to NOT bundle the module, treating
it as an external dependency expected to be available at runtime. This conflicts with
the alias approach — we want Vite to resolve the import via the alias and bundle the
stub, not leave it as an unresolved external dependency.

### 2. `apps/tauri/stubs/spacebot-api-client.ts` — New file

A TypeScript stub module providing mock implementations for all exports that
`@spacebot/api-client` would normally provide:

- `apiClient` — Object with mock methods (`chat.completions.create`, `audio.speech.create`,
  `listPortalConversations`, `portalHistory`, `portalSend`, etc.) that throw
  "Spacebot is not available" errors or return empty data
- `getEventsUrl()` — Returns empty string (no SSE endpoint)
- `setServerUrl()` — No-op
- `mockSpacebotUnavailable()` — Throws error
- Type exports: `ChatCompletion`, `ChatCompletionChunk`, `InboundMessageEvent`,
  `OutboundMessageEvent`, `TypingStateEvent`, `PortalConversationResponse`,
  `PortalConversationSummary`, `Task`, `UpdateTaskRequest`, `TimelineItem`,
  `WorkerListItem`, `TtsProfile`

**Why**: Spacebot UI components import these symbols from `@spacebot/api-client`.
The stub enables the UI to load and render, with Spacebot features gracefully disabled
(displaying "unavailable" states rather than crashing the entire app).

### 3. `node_modules/@spacebot/api-client/` — Stub package (not committed)

Created `package.json` and `index.js` directly in `node_modules/@spacebot/api-client/`
to enable Vite's standard node_modules resolution for bare module specifiers.

**Why**: Vite's import analysis plugin resolves bare specifiers like
`@spacebot/api-client` through node_modules BEFORE the alias plugin runs. Having a
real package in node_modules ensures Vite can find the module at the import analysis
stage. The alias then rewrites the resolved path to the stub file.

**Caveat**: This stub is in `node_modules/` which is gitignored and will be lost on
`bun install` / `npm install`. A postinstall script or Vite plugin could recreate it
automatically. Currently it must be manually recreated after dependency reinstalls.

### 4. `crates/task-system/src/system.rs` — Rust API update

Changed `fetch_update` to `try_update` on line 634:
```rust
// Before:
.fetch_update(Ordering::Release, Ordering::Acquire, |last_worker_id| {
// After:
.try_update(Ordering::Release, Ordering::Acquire, |last_worker_id| {
```

**Why**: The `rust-build` conda environment uses a newer Rust version where
`AtomicUsize::fetch_update` has been renamed to `try_update`. The semantics are
identical — both attempt an atomic compare-and-swap update, returning `Ok(new)` on
success or `Err(current)` on failure. This is a Rust standard library API evolution.

## Summary

The transparent window issue had two root causes:

1. **`@spacebot/api-client` resolution failure** — The open-source repo references a
   private package without a fallback, causing Vite import resolution to fail.

2. **`@spacedrive/ai` CJS interop failure** — Excluding `@spacedrive/ai` from Vite's
   dependency optimization prevented proper CJS-to-ESM conversion for its transitive
   dependencies (`style-to-js` via `hast-util-to-jsx-runtime`), causing a runtime
   `SyntaxError`.

Both issues are inherent to the open-source codebase's reliance on private packages
without proper fallback mechanisms. The adaptations provide those fallbacks so the
app can build and run with Spacebot/SpaceUI features gracefully disabled.