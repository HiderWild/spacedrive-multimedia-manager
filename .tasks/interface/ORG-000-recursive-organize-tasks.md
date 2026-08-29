---
id: ORG-000
title: Recursive organize tasks
status: In Progress
assignee: codex-team
priority: High
tags: [interface, core, organize, windows, destructive-files]
whitepaper: docs/superpowers/specs/2026-08-27-recursive-organize-tasks-design.md
---

## Description

Turn the approved recursive organize-task design into a Windows-first workflow that snapshots one directory tree, lets you mark entries as Keep, Discard, or Move, shows subtree progress, previews media quickly, and commits destructive work only after an explicit review.

The approved design is the behavior contract. The implementation plan translates that contract into test-first work. This file is the only execution-status source for the current implementation run.

## Stable Contracts

- Design: `docs/superpowers/specs/2026-08-27-recursive-organize-tasks-design.md`
- Implementation plan: `docs/superpowers/plans/2026-08-27-recursive-organize-tasks-implementation.md`
- Superseded design: `docs/superpowers/specs/2026-06-05-organize-view-design.md`
- Platform scope: Windows only
- Destructive rule: only Discard and confirmed Move produce filesystem mutations. Keep means reviewed.
- Snapshot rule: task membership is frozen at creation. Later filesystem drift is reported, not silently absorbed.

## Status Vocabulary

- `Blocked`: a prerequisite or external decision prevents work.
- `Ready`: all prerequisites are merged and the task can be dispatched.
- `In Flight`: an agent owns the task.
- `Candidate`: implementation is committed and awaits review or the serial merge window.
- `Review`: specification or quality review is running.
- `Merge Window`: the coordinator is running the lane check and integrating one candidate.
- `Merged`: the accepted commit is on the integration branch.
- `Verified`: a milestone verifier confirmed the integrated behavior.

## Execution DAG

| ID | Responsibility | Depends on | Write scope | Status | Owner | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| ORG-PLAN | Map current code and write the executable plan | Approved design | This task file and the implementation plan | Merged | coordinator + planning agents | Plan committed as `2d6e38024`; mapper facts incorporated; placeholder and diff checks passed |
| ORG-BUILD-00 | Enforce one shared Rust artifact tree and destructive pre-build cleanup across registered worktrees | User-approved cleanup design | `AGENTS.md` and focused PowerShell build tooling | Merged | build-policy agent | `AGENTS.md`, shared wrapper/policy, PowerShell and shell entrypoint coverage committed; focused fixtures and independent Windows-first P0/P1 review passed |
| ORG-BE-01 | Windows path identity, tree intervals, sparse decisions, progress, and topology rules | ORG-PLAN, ORG-BUILD-00 | Core organize domain files and colocated tests only | Merged | Poincare/Singer/Erdos | Candidate `a299f3ed6`; final specification and quality reviews P0/P1=0; implementation is included in the passing `sd-core` check |
| ORG-BE-02 | SQLite migration, entities, task repository, and persistence tests | ORG-BE-01, ORG-BUILD-00, ORG-BUILD-BASELINE | Core migration and organize persistence files only | Merged | Gauss | `bc69c18b2` through `0b1424840`; current shared-wrapper verification passes 33/33 `organize_repository` tests. Includes atomic pending-addition acceptance, safe two-phase nested-set rebuild, malformed-tree rejection, cursor/decision/immutability contracts, pending-addition visibility as neutral unmarked items, and final P0/P1 review with no remaining BE-02 blocker |
| ORG-FE-01 | Task routes, sidebar entry, task creation flow, and task-scoped route shell | ORG-TS-01 | Interface navigation and organize route shell | Merged | coordinator/FE-01 | `fa477ca5c`; task list/create route, sidebar entry, task-scoped shell, generated DTO consumption, and preview wiring are present |
| ORG-PREV-01 | Deterministic directory preview-sequence algorithm and backend tests | ORG-BE-03, ORG-PLAN, ORG-BUILD-00 | Preview-sequence query module and tests only | Merged | Planck | `7120ce6ce` plus the task-root physical-path correction; shared-wrapper `preview_sequence` integration tests pass 4/4 and the snapshot media candidate unit test passes |
| ORG-BE-03 | Task create, list, get, snapshot scan, and change-scan operations | ORG-BE-01, ORG-BE-02 | Core organize operation and scanner files | Verified | coordinator | Create/query and Windows snapshot implementation committed in `4c15b2042`, `953f1c35e`, `c841fac9e`, and `0cca5528b`; initial/retry snapshot schema validity fixed in `fcbfddb01` and `5ebf5b4aa`; shared-wrapper verification passes `organize_repository` 31/31 and Windows `organize_task_flow` 6/6 |
| ORG-TS-01 | Regenerate and validate Rust-to-TypeScript contracts | ORG-BE-03, ORG-BE-04, ORG-BE-05, ORG-PREV-01 | Generated TypeScript types only | Merged | coordinator | `673c2df29`; generated `packages/ts-client/src/generated/types.ts` contains organize actions/queries and the shared Cargo wrapper completed successfully |
| ORG-FE-02 | Virtualized direct children, Ctrl/Shift/Ctrl+A, correct lasso, and selection tests | ORG-FE-01 | Organize selection, lasso, virtualization, and card files only | Merged | coordinator/FE-02 | `2e21aa211`, `e14c7127a`, `1a99b55a7`, `831ed4ef8`, `3880751b9`, `8b4d3730a`, `411e31d60`, `9326d4a13`, `6f10becf7`; UUID-based selection, measured grid width, blank-click clearing, real row virtualization, rendered-item thumbnails, cursor pagination, current-filter Ctrl+A, directory progress, plain replacement, Ctrl union from pointer-down baseline, and edge-scroll wiring are covered by Bun tests |
| ORG-BE-04 | Decision mutation, subtree overwrite analysis, progress projection, and delete compaction | ORG-BE-01, ORG-BE-02 | Core organize decision/query files | Merged | coordinator/Einstein | `de5df2edc`; decision mutation, typed confirmation, recursive progress, and delete-root compaction are implemented and covered by the unified backend suite |
| ORG-FE-03 | Decision bar, progress, override dialogs, and Move picker | ORG-FE-02, ORG-BE-04 | Organize decision and action UI files only | Merged | coordinator/FE-03 | `8930bc41f`, `36e14a43b`, `b150efd92`; Keep/Discard/Move, progress, change acceptance with safe confirmation defaults, current-library Locations, native browse, move-conflict confirmation, stale revision handling, and typed generated DTO wiring are implemented |
| ORG-PREV-02 | Shared image/video/directory preview UI for Explorer and organize tasks | ORG-PREV-01, ORG-TS-01 | Shared preview components and integration adapters | Merged | coordinator/PREV-02 | `4519de0c7`, `b608b186f`; bounded image/video/directory preview pane is wired into the task workspace, supports keyboard movement within the sampled directory sequence, and opens the existing full preview contract |
| ORG-BE-05 | Commit-plan normalization, preflight drift detection, destructive job, and recovery-safe persistence | ORG-BE-04 | Core organize commit and job files | Merged | coordinator | `ef9aa771f`, `7ef0675df`, `90ca15ff9`, `7b9abccfb`, `635f58253`, `ab41a5f9e`, `c9c1e5449`, `f1af8bc5e`; planner, preflight, independent root settlement, durable child dispatch intent, checkpoint-first resume, parent-owned child recovery, and linked-child resume rejection are implemented |
| ORG-TS-02 | Regenerate final contracts after decision and commit operations | ORG-BE-04, ORG-BE-05 | Generated TypeScript types only | Merged | coordinator | `8136d60ce` and existing `673c2df29`; contract hygiene confirms the new route uses generated DTOs and no legacy JSON write path remains |
| ORG-FE-04 | Review summary, overwrite confirmation, Move destination, commit confirmation, and result UI | ORG-FE-03, ORG-PREV-02, ORG-TS-02 | Organize review and commit UI | Merged | coordinator/FE-04 | `eae588c7e`, `8f20e5149`, `a17e319cb`, `665c3f788`, `c481c6e6f`, `ef34e9d03`; single Review commit entry, exact plan summary, lifecycle controls, safe Escape/outside-click/focus behavior, and typed legacy migration boundary are covered by targeted tests |
| ORG-CHANGES-01 | Recursive task-scoped change review and breadcrumb navigation | ORG-BE-03, ORG-FE-04, ORG-TS-02 | Organize change query/panel, children breadcrumb DTO, and focused tests | Merged | coordinator | `00437e353`; `organize.changes` returns all pending/changed/missing snapshot items in stable revision-bound cursor pages, change review merges pages without duplicate IDs, and `organize.children` returns an ancestor breadcrumb used by task navigation |
| ORG-INT-01 | Vertical smoke flow for create, mark, review, commit, restart, and drift | ORG-FE-04, ORG-BE-05 | Integration tests and minimal test fixtures | Candidate | coordinator | WebDriver harness migrated in `80ae1ac53`, now enters through the Explorer PathBar `Organize` action, captures and deletes its created task record, uses image/video-shaped fixtures, and covers lasso/conflict/reload/drift-gated commit/disk effects/lifecycle; legacy dead code removed in `a49502b63`, Windows snapshot fixtures added in `201988ca3`, and static Python checks pass, but live execution is unavailable because Selenium is not installed and no WebView2 DevTools app target is running |
| ORG-VERIFY-01 | Stage verification after the first four merged implementation lanes | First four implementation lanes | Read-only verification | Verified | coordinator | `sd-core` compiled after recovery/lasso/type fixes; focused frontend tests and generated-contract hygiene pass |
| ORG-VERIFY-02 | Final build, focused suites, type drift, task validation, and Windows smoke evidence | ORG-INT-01 | Read-only verification | Blocked | coordinator | Shared-wrapper `sd-core` organize tests pass 48/48 (`organize_repository` 34/34, `organize_decision_repository` 4/4, `organize_task_flow` 6/6, `preview_sequence` 4/4). Combined frontend organize/preview suite passes 48/48 with 105 assertions; full `packages/interface` typecheck passes after the tracked asset/module declarations and Spacebot stub typing fix in `fe630b4dc`; generated-type drift check completed through the shared wrapper with no diff; task-validator `validate` passes through the shared wrapper; Tauri legacy tests remain previously passing. Live Windows WebDriver smoke remains blocked because Selenium is unavailable, no WebView2 DevTools app target is running, and the installed driver does not match the runtime |
| ORG-REVIEW-01 | Final scope, contract, destructive-safety, and code-quality review | ORG-VERIFY-02 | Read-only review | Verified | final review agent + coordinator | Final candidate `f1af8bc5e`; independent review found no code P0/P1 after durable child intent, checkpoint-first recovery, parent-owned linked-child recovery, and manual linked-child rejection. Startup auto-resume remains an explicit pre-existing product policy boundary, documented below |

## Planning Lanes

| ID | Output | Status | Owner | Evidence |
| --- | --- | --- | --- | --- |
| ORG-MAP-BE | Exact backend files, patterns, tests, and commands for the implementation plan | Candidate | backend mapper | Report received: Core modules, migrations, jobs, Windows paths, type generation, and risks mapped; incorporated into plan |
| ORG-MAP-FE | Exact frontend files, patterns, tests, and commands for the implementation plan | Candidate | Copernicus | Report received: route, selection, virtualization, preview, generated-type, and test boundaries mapped; awaiting plan incorporation |
| ORG-PLAN-DRAFT | Full test-first plan draft derived from the approved design and repository facts | Merged | Jason | Commit `aaa54b8b4` cherry-picked as `2d6e38024`; 2,198 lines; placeholder scan clean |
| ORG-BUILD-MAP | Existing build scripts, exact artifact roots, and safe shared-target integration points | Candidate | build mapper | Report received: root and Tauri target paths, bypassing entry points, safety risks, fixture tests, and root-target recommendation mapped |

## Build Discipline Substeps

| ID | Responsibility | Depends on | Status | Owner | Evidence |
| --- | --- | --- | --- | --- | --- |
| ORG-BUILD-00A | Test-first safe artifact discovery/cleanup, shared target resolver, wrapper, and policy documentation | ORG-BUILD-00A-SCOPE | Merged | Ptolemy + coordinator | `44ee62cc0a4e9d27d9d2607d8865f4c7c0db1d42` plus scoped documentation `70579803a7e8e364e01d9d671c06d2ce4cac1586`; 12/12 fixture, AST, diff and quality review passed |
| ORG-BUILD-00A-R | Recover ORG-BUILD-00A from any residual worktree and finish the minimal policy slice | ORG-BUILD-00A | Merged | Archimedes | Recovery and P1 fixes accepted; no residual worktree remains |
| ORG-BUILD-00B | Wire every project build entry point to the shared wrapper and resolver | ORG-BUILD-00A | Merged | coordinator + entrypoint agents | Complete Windows-first local build graph is covered by 00B-PW, 00B-SH and 00B-R; final independent review passed |
| ORG-BUILD-00B-PW | Wire PowerShell start/run/restart/cache entry points | ORG-BUILD-00A | Merged | Turing/Socrates | Candidate `0d48fcfce`; transitive Tauri dev/package chain covered; final independent review P0/P1=0 |
| ORG-BUILD-00B-SH | Wire justfile, Tauri package, and shell Cargo entry points | ORG-BUILD-00A | Merged | Feynman/Socrates | Candidate `0d48fcfce`; direct shell/package and Tauri paths covered; final independent review P0/P1=0 |
| ORG-BUILD-00B-R | Wire residual user-facing compile entrypoints and repository-wide bypass checks | ORG-BUILD-00B-PW, ORG-BUILD-00B-SH | Merged | Helmholtz/Franklin | Candidate `f0c140746`; 9/9 bounded fixture checks, Bash syntax, JSON/JSONC parse, PowerShell fixture/AST, residual bypass scan, and independent quality review P0/P1=0 |
| ORG-BUILD-00C | Independent policy review and fixture verification | ORG-BUILD-00B-PW, ORG-BUILD-00B-SH, ORG-BUILD-00B-R | Merged | Russell + coordinator | Complete transitive Windows-first policy review passed; non-Windows CI/fallback explicitly out of scope |
| ORG-BUILD-BASELINE | Repair pre-existing sd-core compile blockers required for focused Rust verification | ORG-BUILD-00C | Merged | James | `e9bb9a0aa`; exact four-file scope; clean Windows wrapper `check -p sd-core -j 2` passes after BE-02 compile repair, with only pre-existing warnings/future-incompatibility notice |

## Acceptance Criteria

- [x] You can create one organize task from a Windows directory and its recursive membership remains frozen.
- [x] Restarting the daemon and app preserves task membership, decisions, progress, destinations, and commit history through persisted task/checkpoint contracts.
- [x] Keep, Discard, Move, and unmarked states follow the approved sparse-subtree semantics.
- [x] Marking a subtree Discard asks for confirmation only when it would overwrite Keep or Move work.
- [x] A subtree containing only unmarked or Discard descendants can be compacted to one Discard operation without an overwrite prompt.
- [x] Progress counts every marked effective item and exposes descendant progress at parent directories.
- [x] Selection replaces by default. Ctrl adds or toggles. Lasso uses stable geometry and keeps working with virtualization.
- [x] The item grid stays bounded when browsing directories with thousands of media entries.
- [x] Images, videos, and directories have deterministic previews in both Explorer and organize tasks.
- [x] Review shows the exact effective Discard and Move plan before any destructive operation starts.
- [x] Commit preflight detects filesystem drift and prevents stale destructive actions from executing silently.
- [x] Discard remains permanent deletion, with explicit confirmation and compacted ancestor operations.
- [x] Move integrates with saved locations and reports per-item failures without corrupting task decisions.
- [ ] Focused unit, contract, integration, generated-type, task-validator, and Windows smoke checks pass. Final shared-wrapper Rust suites, Tauri legacy tests, task validation, combined frontend/preview tests, and the full interface typecheck pass. The live Windows WebDriver/release-bundle smoke check remains unavailable because the required Selenium and running WebView2 DevTools target are not present in this environment.

## Deferred Issues

Record non-blocking findings here with trigger, impact, severity, workaround, and evidence. A recorded issue does not become current scope automatically.

- The interface baseline required tracked Vite asset/module declarations and a local `@spacebot/api-client` stub declaration to make the existing package typecheck contract executable. Those declarations are now committed in `fe630b4dc`, and `bun run --cwd packages/interface typecheck` exits 0. Severity: resolved implementation prerequisite, retained here as evidence of why the typecheck gate was previously blocked.
- The preview test runtime does not provide the browser `window` global used by the existing Prism lazy loader, so the test logs an isolated load warning while its pure preview assertions pass. The Vite-only SVG glob is guarded for Bun and the package-export test passes. Severity: P2 test-runtime rough edge, not an organize contract failure.
- The task page now exposes current-library Locations, physical pinned Paths from the current Space layout, native Windows directory picker, and up to five distinct physical destinations derived from the task's visible effective Move decisions. The generated organize contract has no dedicated recent-destinations field, so destinations outside the loaded root change-details page are not fabricated. Severity: P2 capability gap, not a data-safety blocker.
- Change acceptance now queries all pending additions and externally changed or missing records in the fixed task snapshot through `organize.changes`, using revision-bound cursor pages and an explicit Load more action. The panel still permits accepting the records already loaded if a later page fails, and reports that partial-load condition. Severity: bounded recoverable UI failure, not a destructive-safety bypass.
- No live Tauri/browser smoke was run because the workspace has no frontend `dist` and the in-app browser cannot launch this desktop test target. The Tauri legacy Rust unit suite passes with process-local config overrides. Severity: P2 verification gap.
- Library startup intentionally leaves interrupted jobs paused because the existing `resume_interrupted_jobs_after_load` call remains disabled. The current contract therefore requires an explicit user resume of the organize parent; linked Move/Delete children cannot be resumed independently. Trigger: a future product decision to enable start-up execution must add a real startup smoke test and preserve parent-owned child reconciliation. Severity: P2 policy boundary, not a current data-safety blocker.
