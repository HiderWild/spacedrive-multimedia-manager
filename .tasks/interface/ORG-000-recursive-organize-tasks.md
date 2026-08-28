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
| ORG-BUILD-00 | Enforce one shared Rust artifact tree and destructive pre-build cleanup across registered worktrees | User-approved cleanup design | `AGENTS.md` and focused PowerShell build tooling | In Flight | build-policy agent | Existing targets cleared, 30,618 files and 22.5 GiB removed; implementation lane dispatched |
| ORG-BE-01 | SQLite migration, entities, task repository, and persistence tests | ORG-PLAN, ORG-BUILD-00 | Core migration and organize persistence files only | Blocked | Unassigned | Waiting for dependencies |
| ORG-BE-02 | Windows path identity, snapshot tree, sparse-decision, progress, and conflict pure logic | ORG-PLAN, ORG-BUILD-00 | Core organize domain files and colocated tests only | Blocked | Unassigned | Waiting for dependencies |
| ORG-FE-01 | Selection reducer, Ctrl semantics, lasso geometry, and reducer tests | ORG-PLAN, ORG-BUILD-00 | Organize selection module and tests only | Blocked | Unassigned | Waiting for dependencies |
| ORG-PREV-01 | Deterministic directory preview-sequence algorithm and backend tests | ORG-PLAN, ORG-BUILD-00 | Preview-sequence query module and tests only | Blocked | Unassigned | Waiting for dependencies |
| ORG-BE-03 | Task create, list, get, snapshot scan, and change-scan operations | ORG-BE-01, ORG-BE-02 | Core organize operation and scanner files | Blocked | Unassigned | Waiting for dependencies |
| ORG-TS-01 | Regenerate and validate Rust-to-TypeScript contracts | ORG-BE-03 | Generated TypeScript types only | Blocked | Unassigned | Waiting for ORG-BE-03 |
| ORG-FE-02 | Sidebar entry, task creation flow, task list, and task-scoped route shell | ORG-TS-01 | Interface navigation and organize route shell | Blocked | Unassigned | Waiting for ORG-TS-01 |
| ORG-BE-04 | Decision mutation, subtree overwrite analysis, progress projection, and delete compaction | ORG-BE-01, ORG-BE-02 | Core organize decision/query files | Blocked | Unassigned | Waiting for dependencies |
| ORG-FE-03 | Virtualized recursive workspace, subtree progress, keyboard/lasso interaction, and state restore | ORG-FE-01, ORG-FE-02, ORG-BE-04 | Organize workspace components and tests | Blocked | Unassigned | Waiting for dependencies |
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
| ORG-BUILD-00B | Wire every project build entry point to the shared wrapper and resolver | ORG-BUILD-00A | Ready | coordinator + entrypoint agents | Split into disjoint 00B-PW and 00B-SH lanes; prior dispatch hit agent-limit and was not started |
| ORG-BUILD-00B-PW | Wire PowerShell start/run/restart/cache entry points | ORG-BUILD-00A | Quality Review | Turing/Socrates | Complete candidate `0d48fcfce91c98b893fa10147c2ed5f0b0702b10` closes the transitive Tauri dev/package chain; focused Tauri, policy, PowerShell, Shell, AST and diff checks passed; independent quality review pending |
| ORG-BUILD-00B-SH | Wire justfile, Tauri package, and shell Cargo entry points | ORG-BUILD-00A | Quality Review | Feynman/Socrates | Complete candidate `0d48fcfce91c98b893fa10147c2ed5f0b0702b10` closes `just dev-desktop`, package scripts, config hooks and Tauri helper; focused Tauri, policy, PowerShell, Shell, AST and diff checks passed; independent quality review pending |
| ORG-BUILD-00C | Independent policy review and fixture verification | ORG-BUILD-00B-PW, ORG-BUILD-00B-SH | In Flight | Unassigned | Candidate is ready for one independent review of the complete transitive policy and isolated fixtures |

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
