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
| ORG-BE-02 | SQLite migration, entities, task repository, and persistence tests | ORG-BE-01, ORG-BUILD-00, ORG-BUILD-BASELINE | Core migration and organize persistence files only | Merged | Gauss | `bc69c18b2` through `0b1424840`; 26/26 `organize_repository` tests pass. Includes atomic pending-addition acceptance, safe two-phase nested-set rebuild, malformed-tree rejection, cursor/decision/immutability contracts, and final P0/P1 review with no remaining BE-02 blocker |
| ORG-FE-01 | Task routes, sidebar entry, task creation flow, and task-scoped route shell | ORG-TS-01 | Interface navigation and organize route shell | Blocked | Unassigned | Waiting for ORG-TS-01 generated DTOs |
| ORG-PREV-01 | Deterministic directory preview-sequence algorithm and backend tests | ORG-BE-03, ORG-PLAN, ORG-BUILD-00 | Preview-sequence query module and tests only | Ready | Unassigned | BE-03 task manifest and task-scoped ownership contract are now present; ready for implementation |
| ORG-BE-03 | Task create, list, get, snapshot scan, and change-scan operations | ORG-BE-01, ORG-BE-02 | Core organize operation and scanner files | Merged | coordinator | Create/query and Windows snapshot implementation committed in `4c15b2042`, `953f1c35e`, `c841fac9e`, and `0cca5528b`; 27/27 repository tests and 3/3 wire-flow tests pass through the shared build wrapper |
| ORG-TS-01 | Regenerate and validate Rust-to-TypeScript contracts | ORG-BE-03 | Generated TypeScript types only | Ready | Unassigned | BE-03 public DTOs and operation registrations are stable; regenerate and validate types before frontend work |
| ORG-FE-02 | Virtualized direct children, Ctrl/Shift/Ctrl+A, correct lasso, and selection tests | ORG-FE-01 | Organize selection, lasso, virtualization, and card files only | Blocked | Unassigned | Waiting for ORG-FE-01 route shell |
| ORG-BE-04 | Decision mutation, subtree overwrite analysis, progress projection, and delete compaction | ORG-BE-01, ORG-BE-02 | Core organize decision/query files | Ready | Unassigned | Backend prerequisites are merged; implementation can proceed against the approved decision and progress contracts |
| ORG-FE-03 | Decision bar, progress, override dialogs, and Move picker | ORG-FE-02, ORG-BE-04 | Organize decision and action UI files only | Blocked | Unassigned | Waiting for selection workspace and backend decision contract |
| ORG-PREV-02 | Shared image/video/directory preview UI for Explorer and organize tasks | ORG-PREV-01, ORG-TS-01 | Shared preview components and integration adapters | Blocked | Unassigned | Waiting for dependencies |
| ORG-BE-05 | Commit-plan normalization, preflight drift detection, destructive job, and recovery-safe persistence | ORG-BE-04 | Core organize commit and job files | Blocked | Unassigned | Waiting for ORG-BE-04 |
| ORG-TS-02 | Regenerate final contracts after decision and commit operations | ORG-BE-04, ORG-BE-05 | Generated TypeScript types only | Blocked | Unassigned | Waiting for dependencies |
| ORG-FE-04 | Review summary, overwrite confirmation, Move destination, commit confirmation, and result UI | ORG-FE-03, ORG-PREV-02, ORG-TS-02 | Organize review and commit UI | Blocked | Unassigned | Waiting for dependencies |
| ORG-INT-01 | Vertical smoke flow for create, mark, review, commit, restart, and drift | ORG-FE-04, ORG-BE-05 | Integration tests and minimal test fixtures | Blocked | Unassigned | Waiting for feature slices |
| ORG-VERIFY-01 | Stage verification after the first four merged implementation lanes | First four implementation lanes | Read-only verification | Blocked | Unassigned | Waiting for milestone |
| ORG-VERIFY-02 | Final build, focused suites, type drift, task validation, and Windows smoke evidence | ORG-INT-01 | Read-only verification | Blocked | Unassigned | Waiting for ORG-INT-01 |
| ORG-REVIEW-01 | Final scope, contract, destructive-safety, and code-quality review | ORG-VERIFY-02 | Read-only review | Blocked | Unassigned | Waiting for ORG-VERIFY-02 |

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

- [ ] You can create one organize task from a Windows directory and its recursive membership remains frozen.
- [ ] Restarting the daemon and app preserves task membership, decisions, progress, destinations, and commit history.
- [ ] Keep, Discard, Move, and unmarked states follow the approved sparse-subtree semantics.
- [ ] Marking a subtree Discard asks for confirmation only when it would overwrite Keep or Move work.
- [ ] A subtree containing only unmarked or Discard descendants can be compacted to one Discard operation without an overwrite prompt.
- [ ] Progress counts every marked effective item and exposes descendant progress at parent directories.
- [ ] Selection replaces by default. Ctrl adds or toggles. Lasso uses stable geometry and keeps working with virtualization.
- [ ] The item grid stays bounded when browsing directories with thousands of media entries.
- [ ] Images, videos, and directories have deterministic previews in both Explorer and organize tasks.
- [ ] Review shows the exact effective Discard and Move plan before any destructive operation starts.
- [ ] Commit preflight detects filesystem drift and prevents stale destructive actions from executing silently.
- [ ] Discard remains permanent deletion, with explicit confirmation and compacted ancestor operations.
- [ ] Move integrates with saved locations and reports per-item failures without corrupting task decisions.
- [ ] Focused unit, contract, integration, generated-type, task-validator, and Windows smoke checks pass.

## Deferred Issues

Record non-blocking findings here with trigger, impact, severity, workaround, and evidence. A recorded issue does not become current scope automatically.

- None.
