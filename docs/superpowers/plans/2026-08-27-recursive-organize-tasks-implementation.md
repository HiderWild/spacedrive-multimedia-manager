# Recursive Organize Tasks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the per-directory Organize Explorer view with Windows-first recursive organize tasks backed by a fixed SQLite snapshot, recursive decisions and progress, bounded shared preview, and preflighted move and permanent-delete jobs.

**Architecture:** Core owns Windows path identity, the fixed snapshot, sparse subtree decisions, progress, drift detection, and operation settlement in two local library tables. The React task route consumes generated `@sd/ts-client` DTOs, pages and virtualizes direct children, keeps selection transient, and reuses native file operations plus Quick Preview renderers. This plan implements the Windows desktop flow only. Existing cross-platform build targets receive the smallest explicit unsupported result required to compile, with no Unix traversal or reusable platform framework.

**Tech Stack:** Rust, Tokio, SeaORM/SQLite, Specta, persisted Spacedrive jobs, React 19, TypeScript, TanStack Query, `@tanstack/react-virtual`, Bun tests, Tauri 2, Python WebDriver.

---

## Scope Guardrails

- The approved design at `docs/superpowers/specs/2026-08-27-recursive-organize-tasks-design.md` is the business contract.
- The old plan is useful only for locating existing code. Its Explorer `ViewMode`, per-directory JSON persistence, immediate delete, and recursive 10,000-record preview behavior must be removed.
- Implement Windows desktop/Tauri only. Do not add Unix traversal, sync registration, a workflow framework, plugin hooks, cloud continuation, event sourcing, or generic task abstractions.
- Keep and Clear are metadata-only. Discard and Move become filesystem actions only after commit confirmation and preflight.
- No action may hash file contents, decode media, or generate thumbnails while creating or refreshing the snapshot.

## Dependency Graph

```text
ORG-BE-01
  -> ORG-BE-02
     -> ORG-BE-03
        -> ORG-BE-04
           -> ORG-BE-05
        -> ORG-PREV-01
ORG-BE-03 + ORG-BE-04 + ORG-BE-05 + ORG-PREV-01
  -> ORG-TS-01
     -> ORG-FE-01
        -> ORG-FE-02
           -> ORG-FE-03
ORG-PREV-01 + ORG-TS-01 + ORG-FE-02
  -> ORG-PREV-02
ORG-BE-05 + ORG-FE-03 + ORG-PREV-02
  -> ORG-FE-04
ORG-FE-04
  -> ORG-TS-02
     -> ORG-INT-01
```

Backend and frontend workers may proceed in parallel after the frozen contract below is accepted. Frontend code must still wait for ORG-TS-01 before it imports the generated DTOs. ORG-TS-01 and ORG-TS-02 are strictly serial and are the only tasks allowed to run `generate_typescript_types` or stage `packages/ts-client/src/generated/types.ts`.

Treat these as P1 dependency gates:

1. ORG-FE-01 adds the new routes while the old Explorer `ViewMode` remains temporarily reachable. Do not merge or release that intermediate state until the separate ORG-TS-02 retirement increment is green.
2. ORG-TS-02 must replace the old WebDriver JSON-persistence assertions before deleting the old Tauri commands and Explorer route.
3. No frontend task starts before ORG-TS-01 has generated and committed the public DTOs. Handwritten response types and temporary casts are prohibited.

## File Structure

### Core persistence and domain rules

- Create `core/src/infra/db/migration/m20260827_000001_create_organize_tasks.rs`
  - Create only `organize_tasks` and `organize_task_items`, including partial indexes and SQLite `CHECK` constraints.
- Modify `core/src/infra/db/migration/mod.rs`
  - Register the migration after `m20260713_000001_ai_exclusion`.
- Create `core/src/infra/db/entities/organize_task.rs`
  - Map the task row. Do not implement `Syncable`.
- Create `core/src/infra/db/entities/organize_task_item.rs`
  - Map snapshot, decision, drift, and operation fields plus task/parent relations. Do not implement `Syncable`.
- Modify `core/src/infra/db/entities/mod.rs`
  - Export both local-only entities and active models.
- Create `core/src/ops/organize/mod.rs`
  - Export the organize operation modules and nothing platform-generic.
- Modify `core/src/ops/mod.rs`
  - Add `pub mod organize;`.
- Create `core/src/ops/organize/model.rs`
  - Own all public Specta DTOs and internal row-to-wire conversions.
- Create `core/src/ops/organize/error.rs`
  - Own stable organize error codes and conversions into action/query/job errors.
- Create `core/src/ops/organize/path.rs`
  - Own Windows canonical display paths, case-insensitive keys, ancestry/overlap, volume-root checks, and move topology validation.
- Create `core/src/ops/organize/tree.rs`
  - Own interval/unit reduction, selection normalization, sparse decision conflict resolution, progress reduction, and defensive operation compaction.
- Create `core/src/ops/organize/repository.rs`
  - Own every SQL read and transaction touching the two organize tables.

### Core task creation, snapshot, query, decision, commit, and lifecycle

- Create `core/src/ops/organize/create/{mod.rs,action.rs}`
  - Validate the root, return typed create/overlap/unsupported outcomes, insert `scanning`, and dispatch the snapshot job.
- Create `core/src/ops/organize/snapshot/{mod.rs,job.rs,windows.rs,unsupported.rs,change_job.rs}`
  - Enumerate Windows metadata with an explicit stack, persist batches, rebuild intervals, and compare live scope without changing the included denominator.
- Create `core/src/ops/organize/query/{mod.rs,list.rs,get.rs,children.rs,resolve_root.rs,commit_plan.rs}`
  - Register the task list/detail/paged-child/root-availability/commit-plan queries.
- Create `core/src/ops/organize/decision/{mod.rs,action.rs}`
  - Register transactional set/clear decisions and typed confirmation outcomes.
- Create `core/src/ops/organize/lifecycle/{mod.rs,scan_changes.rs,accept_changes.rs,retry_snapshot.rs,finish.rs,reopen.rs,delete_task.rs}`
  - Own fixed-snapshot maintenance and task lifecycle actions.
- Create `core/src/ops/organize/commit/{mod.rs,plan.rs,preflight.rs,action.rs,job.rs}`
  - Build compact plans, validate topology, preflight all roots, dispatch existing child jobs, checkpoint, reconcile, and settle per-root state.
- Create `core/tests/organize_repository.rs`
  - Exercise the real migration and repository transactions in temporary SQLite databases.
- Create `core/tests/organize_task_flow.rs`
  - Exercise Windows snapshot, drift, commit ordering, partial settlement, and resume reconciliation against process-owned temporary directories.
- Create `core/tests/organize_wire_contract.rs`
  - Assert exact registry names and public Specta type extraction.

### Shared preview sequence

- Create `core/src/ops/files/preview_sequence/{mod.rs,query.rs,sampler.rs,walk.rs}`
  - Own the bounded file-manager preview query, deterministic representative sampler, Windows live walk, and manifest-backed candidate read.
- Modify `core/src/ops/files/mod.rs`
  - Export `preview_sequence`.
- Create `core/tests/preview_sequence.rs`
  - Cover candidate budgets, reparse behavior, manifest boundaries, and deterministic media selection.
- Modify `packages/interface/src/components/QuickPreview/ContentRenderer.tsx`
  - Export the existing renderer for task-pane reuse and pass a directory preview context.
- Modify `packages/interface/src/components/QuickPreview/DirectoryPreview.tsx`
  - Replace the current `limit: null` listing with `files.preview_sequence`; show a contact sheet, one exact renderer, or a bounded direct-child fallback.
- Modify `packages/interface/src/components/QuickPreview/index.ts`
  - Export the shared content and directory preview surfaces.
- Create `packages/interface/src/components/QuickPreview/PreviewSequence.tsx`
  - Own sequence keyboard navigation, contact-sheet presentation, paused video tiles, and opening an item in full Quick Preview.
- Create `packages/interface/src/components/QuickPreview/__tests__/previewSequence.test.tsx`
  - Verify contact sheet, one-item behavior, video pause, sequence navigation, and sampled state.

### Generated TypeScript contract

- Modify `packages/ts-client/src/generated/types.ts`
  - Regenerate from Rust only. Never hand-edit.
- Create `packages/ts-client/src/__tests__/organizeContract.test.ts`
  - Compile-time/runtime assertions for exact method mappings and discriminants.

### New task route and UI

- Modify `packages/interface/src/router.tsx`
  - Add `/organize` and `/organize/:taskId` under `ShellLayout`.
- Modify `packages/interface/src/components/TabManager/TabManagerContext.tsx`
  - Add per-tab `organizeStates` keyed by task UUID. Do not remove the old `ViewMode` in the new-shell increment.
- Modify `packages/interface/src/components/TabManager/TabNavigationSync.tsx`
  - Derive task-list/task-detail tab titles without changing selection persistence.
- Create `packages/interface/src/routes/organize/index.ts`
  - Export route components.
- Create `packages/interface/src/routes/organize/OrganizeTasksPage.tsx`
  - Render all task records and the legacy migration banner.
- Create `packages/interface/src/routes/organize/OrganizeTaskPage.tsx`
  - Compose header, filters, virtualized center, and shared right preview.
- Create `packages/interface/src/routes/organize/useOrganizeTask.ts`
  - Own typed query/mutation calls, invalidation, stale-revision refetch, and no optimistic decision writes.
- Create `packages/interface/src/routes/organize/selection.ts`
  - Own a new plain/Ctrl/Shift/Ctrl+A reducer keyed only by task item UUID. Do not import Explorer `SelectionContext`.
- Create `packages/interface/src/routes/organize/lasso.ts`
  - Own new rendered-item geometry, backward shrink behavior, and edge-scroll velocity. Do not reuse `OrganizeCenterPane` lasso state.
- Create `packages/interface/src/routes/organize/virtualization.ts`
  - Own list/grid row calculations and the measured DOM-card budget.
- Create `packages/interface/src/routes/organize/OrganizeVirtualList.tsx`
  - Render paged direct children with `useVirtualizer`, two-row overscan, and list layout.
- Create `packages/interface/src/routes/organize/OrganizeVirtualGrid.tsx`
  - Render paged direct children with row virtualization and grid layout.
- Create `packages/interface/src/routes/organize/OrganizeItemCard.tsx`
  - Render snapshot file metadata, progress segments, inherited/explicit decision, drift, and failure badges.
- Create `packages/interface/src/routes/organize/OrganizeHeader.tsx`
  - Render task identity/status/progress and scan/execute/finish actions.
- Create `packages/interface/src/routes/organize/OrganizeFilters.tsx`
  - Render All, Unmarked, Keep, Discard, Move, Failed, and Changes.
- Create `packages/interface/src/routes/organize/OrganizeDecisionBar.tsx`
  - Apply Keep, Discard, Move, or Clear to the complete selection scope.
- Create `packages/interface/src/routes/organize/OrganizeConflictDialog.tsx`
  - Render backend counts and enforce focus-specific Enter confirmation.
- Create `packages/interface/src/routes/organize/OrganizeMovePicker.tsx`
  - Order current task recents, Locations, pinned physical Paths, and native browse.
- Create `packages/interface/src/routes/organize/OrganizeCommitDialog.tsx`
  - Render the current revision's compact plan, drift, unmarked count, permanent-delete warning, and conflict policy.
- Create `packages/interface/src/routes/organize/OrganizeChangesPanel.tsx`
  - Start a change scan and accept selected additions/missing/changed items with explicit preserve/override confirmation.
- Create `packages/interface/src/routes/organize/OrganizeLifecycleDialogs.tsx`
  - Own finish-unmarked, reopen, retry snapshot, and metadata-only delete confirmation.
- Create `packages/interface/src/routes/organize/OrganizePreviewPane.tsx`
  - Render focused files/directories through shared Quick Preview components.
- Create `packages/interface/src/routes/organize/thumbnailCache.ts`
  - Retain only the useful in-memory thumbnail cache, with path + size + last-write + directory key identity.
- Create `packages/interface/src/routes/organize/useOrganizeThumbnail.ts`
  - Load only rendered/overscan thumbnails.
- Create `packages/interface/src/routes/organize/OrganizeThumbnail.tsx`
  - Render cached sidecars with `FileComponent.Thumb` fallback.
- Create `packages/interface/src/routes/organize/__tests__/{selection.test.ts,lasso.test.ts,virtualization.test.tsx,taskPresentation.test.tsx,decisionFlow.test.ts,movePicker.test.ts,commitFlow.test.ts,lifecycle.test.ts,contractHygiene.test.ts}`
  - Cover the frontend white-box contract.
- Modify `packages/interface/package.json` and `bun.lock`
  - Add `@testing-library/react` and `happy-dom` only for DOM-bound virtualization and interaction tests.
- Create `packages/interface/src/test/setup-dom.ts`
  - Install and clean a Happy DOM window for explicit Bun test preloads.

### Entry points, sidebar, migration, then separate old-entry retirement

- Create `packages/interface/src/components/SpacesSidebar/OrganizeTasksGroup.tsx`
  - Show at most five non-completed tasks plus All tasks between the space switcher and user space content.
- Modify `packages/interface/src/components/SpacesSidebar/index.tsx`
  - Mount the task group in the required order.
- Modify `packages/interface/src/routes/explorer/components/VirtualPathBar.tsx`
  - Add Organize/Open task action for the current physical directory.
- Modify `packages/interface/src/routes/explorer/hooks/useFileContextMenu.ts`
  - Add Organize/Open task for a physical directory card.
- Create `packages/interface/src/routes/organize/legacy/{types.ts,importLegacy.ts,LegacyImportBanner.tsx}`
  - Parse only the retained Tauri DTO, create non-overlapping recursive tasks, apply mapped Keep/Discard decisions through `organize.set_decision`, and archive only after success.
- Create `apps/tauri/src-tauri/src/legacy_organize.rs`
  - List, parse, and archive `organize/v1/*.json`; expose no save or delete command.
- Modify `apps/tauri/src-tauri/src/main.rs`
  - Register only `list_legacy_organize_states`, `read_legacy_organize_state`, and `archive_legacy_organize_state`.
- Modify `apps/tauri/src/platform.ts` and `packages/interface/src/contexts/PlatformContext.tsx`
  - Replace old active persistence methods with typed legacy list/read/archive methods.
- ORG-FE-01 does not edit `packages/interface/src/routes/explorer/panes/ExplorerPaneBody.tsx`, `packages/interface/src/routes/explorer/context.tsx`, `packages/interface/src/ShellLayout.tsx`, or `packages/interface/src/routes/explorer/organize/`. Keeping that new-shell increment separate makes rollback and route-conflict diagnosis deterministic.
- ORG-TS-02 is the dedicated retirement increment. Only after the new route, legacy import, and shared preview are green does it perform the deletions and old-entry edits below.
- Delete `apps/tauri/src-tauri/src/organize.rs` after migration tests pass.
- Delete `packages/interface/src/routes/explorer/organize/` in full after shared preview/thumbnail responsibilities have moved.
- Modify `packages/interface/src/routes/explorer/context.tsx`, `packages/interface/src/routes/explorer/ViewModeMenu.tsx`, `packages/interface/src/routes/explorer/panes/ExplorerPaneBody.tsx`, `packages/interface/src/routes/explorer/views/SearchView/SearchView.tsx`, `packages/interface/src/routes/explorer/views/RecentsView/RecentsView.tsx`, `packages/ts-client/src/stores/viewPreferences.ts`
  - Remove the old `organize` ViewMode and fallbacks.
- Modify `packages/interface/src/ShellLayout.tsx`, `packages/interface/src/components/Inspector/Inspector.tsx`, `packages/interface/src/components/Inspector/variants/FileInspector.tsx`
  - Remove old organize-specific inspector wiring; the task route owns its right preview pane.
- Delete `packages/interface/src/organizeLayoutSizing.ts`, `packages/interface/src/__tests__/organizeLayoutSizing.test.ts`, and `packages/interface/src/__tests__/organizeLayoutIntegration.test.ts`.
- Modify `packages/interface/src/locales/en/{explorer.json,sidebar.json,settings.json}` and `packages/interface/src/locales/zh/{explorer.json,sidebar.json,settings.json}`
  - Replace view-mode copy with task, decision, drift, commit, migration, and lifecycle copy.
- Modify `packages/interface/src/Settings/pages/helpSettingsContent.ts`, `packages/interface/src/Settings/pages/HelpSettings.tsx`, and `packages/interface/src/Settings/pages/__tests__/helpSettingsContent.test.ts`
  - Describe shared preview sequence controls rather than the retired inspector tabs.
- Regenerate `packages/interface/src/i18n/types.d.ts` with `bun run --filter @sd/interface generate:i18n-types`. Never hand-edit that generated declaration file.
- Replace Organize-specific sections in `tests/webdriver/test_real_tauri_app.py`
  - Keep one real Windows recursive task flow and remove direct JSON command tests.

## Frozen Cross-Task Contract

### Fixed snapshot and Windows identity

1. `organize_tasks.root_path` is a canonical display-form Windows path. `root_path_key` is the same path with `\` separators, volume-root trailing separator preserved, and Unicode lowercase applied for comparison.
2. The identity tuple for physical access is `(device_slug, root_path_key)`. Creation accepts only `SdPath::Physical` whose slug resolves to the current device through existing `SdPath`/device helpers.
3. `std::fs::canonicalize` plus `crate::common::utils::strip_windows_extended_prefix` produces the display path. UNC stays `\\server\share\...`; drive paths stay `C:\...`.
4. Included manifest rows are the fixed snapshot boundary. New live paths become `pending_addition` rows and contribute zero units and zero bytes until accepted.
5. Included rows have non-null `tree_start`, `tree_end`, and `unit_count`. Pending additions have all three null until acceptance rebuilds the included tree.
6. Snapshot traversal uses `symlink_metadata`, includes hidden/system entries, records reparse points as leaves, and never follows them.

### Public Rust DTOs

All types below live in `core/src/ops/organize/model.rs`, derive `Debug`, `Clone`, `Serialize`, `Deserialize`, `specta::Type`, and use the stated names in every task.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeTaskStatus { Scanning, Active, Committing, Completed, Failed }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeItemKind { File, Directory, ReparsePoint, Unreadable }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeMembershipState { Included, PendingAddition }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeExternalState { Present, Changed, Missing, Unreadable }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeDecisionKind { Keep, Discard, Move }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeOperationState { None, Pending, Running, Applied, Failed }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum OrganizeDecisionInput {
	Keep,
	Discard,
	Move { destination: SdPath },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum OrganizeSelectionInput {
	Items { item_ids: Vec<Uuid> },
	DirectChildren {
		parent_item_id: Uuid,
		filter: OrganizeItemFilter,
		excluded_item_ids: Vec<Uuid>,
	},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeItemFilter { All, Unmarked, Keep, Discard, Move, Failed, Changed, Missing }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeItemSort { Name, Modified, Size, Progress }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeSortDirection { Asc, Desc }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum OrganizeDecisionSource {
	Explicit,
	Inherited { ancestor_item_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OrganizeProgressSummary {
	pub total_units: u64,
	pub processed_units: u64,
	pub keep_units: u64,
	pub discard_units: u64,
	pub move_units: u64,
	pub unmarked_units: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeTaskSummary {
	pub id: Uuid,
	pub name: String,
	pub root_path: String,
	pub root_sd_path: SdPath,
	pub status: OrganizeTaskStatus,
	pub revision: i64,
	pub snapshot_version: i32,
	pub total_entries: u64,
	pub total_bytes: u64,
	pub progress: OrganizeProgressSummary,
	pub scan_issue_count: u64,
	pub pending_addition_count: u64,
	pub failed_operation_count: u64,
	pub changed_count: u64,
	pub missing_count: u64,
	pub scan_job_id: Option<JobId>,
	pub commit_job_id: Option<JobId>,
	pub last_error: Option<String>,
	pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OrganizeItemView {
	pub item_id: Uuid,
	pub parent_item_id: Option<Uuid>,
	pub file: File,
	pub item_kind: OrganizeItemKind,
	pub explicit_decision: Option<OrganizeDecisionKind>,
	pub effective_decision: Option<OrganizeDecisionKind>,
	pub decision_source: Option<OrganizeDecisionSource>,
	pub progress: OrganizeProgressSummary,
	pub move_destination: Option<SdPath>,
	pub external_state: OrganizeExternalState,
	pub operation_state: OrganizeOperationState,
	pub last_error: Option<String>,
}
```

`OrganizeItemView.file.id` is always the task item UUID. If `entry_uuid` enrichment succeeds, copy content metadata, sidecars, and media metadata from the indexed `File`, then overwrite `id`, `sd_path`, name, size, kind, extension, and modified time with snapshot authority.

### Query inputs and outputs

```rust
pub struct OrganizeListInput {
	pub statuses: Option<Vec<OrganizeTaskStatus>>,
	pub cursor: Option<String>,
	pub limit: u32,
}
pub struct OrganizeListOutput { pub tasks: Vec<OrganizeTaskSummary>, pub next_cursor: Option<String> }

pub struct OrganizeGetInput { pub task_id: Uuid }
pub struct OrganizeGetOutput { pub task: OrganizeTaskSummary, pub root_item_id: Uuid }

pub struct OrganizeChildrenInput {
	pub task_id: Uuid,
	pub parent_item_id: Uuid,
	pub cursor: Option<String>,
	pub limit: u32,
	pub sort: OrganizeItemSort,
	pub direction: OrganizeSortDirection,
	pub filter: OrganizeItemFilter,
}
pub struct OrganizeBreadcrumb { pub item_id: Uuid, pub name: String }
pub struct OrganizeChildrenOutput {
	pub revision: i64,
	pub items: Vec<OrganizeItemView>,
	pub breadcrumb: Vec<OrganizeBreadcrumb>,
	pub next_cursor: Option<String>,
	pub matching_child_count: u64,
}

pub struct OrganizeResolveRootInput { pub root: SdPath }
pub enum OrganizeRootAvailability {
	Creatable,
	OpenExisting { task_id: Uuid },
	Unavailable { reason: OrganizeCreateRejection },
}

pub struct OrganizeCommitPlanInput { pub task_id: Uuid, pub expected_revision: i64 }
pub struct OrganizePlanRoot { pub item_id: Uuid, pub source: SdPath, pub units: u64, pub bytes: u64 }
pub struct OrganizeMoveGroup { pub destination: SdPath, pub roots: Vec<OrganizePlanRoot>, pub units: u64, pub bytes: u64 }
pub struct OrganizeCommitPlanOutput {
	pub revision: i64,
	pub move_groups: Vec<OrganizeMoveGroup>,
	pub discard_roots: Vec<OrganizePlanRoot>,
	pub keep_units: u64,
	pub unmarked_units: u64,
	pub pending_addition_count: u64,
	pub changed_or_missing_roots: Vec<Uuid>,
	pub failed_operation_roots: Vec<Uuid>,
	pub unsafe_conflicts: Vec<OrganizeTopologyConflict>,
	pub can_commit: bool,
	pub blocking_reasons: Vec<OrganizeCommitBlockReason>,
}
```

Limits clamp to `1..=100` for `organize.list`, `1..=200` for `organize.children`, and `1..=12` for preview. Cursors are opaque JSON encoded with URL-safe base64 and contain the stable sort value plus item UUID tie-breaker. Invalid cursors return `QueryError::InvalidInput` and never fall back to the first page.

Specta uses the repository's existing externally tagged enum representation for these DTOs. Unit variants are strings, such as `"Creatable"`, `"Discard"`, and `"RejectedPermanentConfirmation"`. Data variants are one-key objects, such as `{ OpenExisting: { task_id } }`, `{ Move: { destination } }`, and `{ StaleRevision: { current_revision } }`. Frontend tests must use those generated shapes exactly.

`organize.resolve_root` is the narrow backend support query required to render **Open organize task** before mutation. It uses the same canonical overlap logic as creation and adds no new business state.

### Mutation outcomes and failure semantics

```rust
pub enum OrganizeCreateRejection {
	UnsupportedPlatform,
	UnsupportedPathKind,
	RootMissing { path: String },
	RootNotDirectory { path: String },
	PermissionDenied { path: String },
}
pub struct OrganizeCreateInput { pub root: SdPath, pub name: Option<String> }
pub enum OrganizeCreateOutcome {
	Created { task_id: Uuid, status: OrganizeTaskStatus, snapshot_job: JobReceipt },
	Overlap { existing_task_id: Uuid },
	Rejected { reason: OrganizeCreateRejection },
}

pub struct OrganizeSetDecisionInput {
	pub task_id: Uuid,
	pub selection: OrganizeSelectionInput,
	pub decision: Option<OrganizeDecisionInput>,
	pub expected_revision: i64,
	pub confirm_descendant_override: bool,
	pub confirm_ancestor_split: bool,
}
pub enum OrganizeDecisionConflictKind { DescendantOverride, AncestorSplit }
pub enum OrganizeDecisionOutcome {
	Applied { revision: i64, task_summary: OrganizeTaskSummary, affected_roots: Vec<Uuid> },
	ConfirmationRequired {
		conflict_kind: OrganizeDecisionConflictKind,
		keep_units: u64,
		discard_units: u64,
		move_units: u64,
		unmarked_units: u64,
		affected_bytes: u64,
		conflicting_roots: Vec<Uuid>,
	},
	StaleRevision { current_revision: i64 },
	InheritedNoOp { revision: i64, ancestor_item_id: Uuid },
	RejectedImmutable { applied_root_item_id: Uuid },
	RejectedState { status: OrganizeTaskStatus },
}

pub struct OrganizeScanChangesInput { pub task_id: Uuid, pub expected_revision: i64 }
pub enum OrganizeJobStartOutcome {
	Started { revision: i64, job: JobReceipt },
	StaleRevision { current_revision: i64 },
	RejectedState { status: OrganizeTaskStatus },
	UnsupportedPlatform,
}

pub struct OrganizeAcceptChangesInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
	pub include_addition_ids: Vec<Uuid>,
	pub remove_missing_ids: Vec<Uuid>,
	pub refresh_changed_ids: Vec<Uuid>,
	pub preserve_changed_decisions: bool,
	pub confirm_inherited_destructive: bool,
}
pub enum OrganizeAcceptChangesOutcome {
	Applied { revision: i64, task_summary: OrganizeTaskSummary },
	ConfirmationRequired { discard_units: u64, move_units: u64, affected_bytes: u64, conflicting_roots: Vec<Uuid> },
	StaleRevision { current_revision: i64 },
	RejectedState { status: OrganizeTaskStatus },
}

pub struct OrganizeCommitInput {
	pub task_id: Uuid,
	pub expected_revision: i64,
	pub permanent_delete_confirmed: bool,
	pub move_conflict_policy: FileConflictResolution,
	#[serde(default)]
	pub allow_current_subtree_drift: bool,
}
pub enum OrganizeCommitOutcome {
	Started { job: JobReceipt },
	StaleRevision { current_revision: i64 },
	RejectedState { status: OrganizeTaskStatus },
	RejectedPermanentConfirmation,
	RejectedBlockedPlan { reasons: Vec<OrganizeCommitBlockReason> },
}

pub struct OrganizeFinishInput { pub task_id: Uuid, pub expected_revision: i64, pub confirm_unmarked: bool }
pub enum OrganizeFinishOutcome {
	Completed { revision: i64 },
	ConfirmationRequired { unmarked_units: u64 },
	StaleRevision { current_revision: i64 },
	RejectedPendingOperations { pending: u64, running: u64, failed: u64 },
	RejectedState { status: OrganizeTaskStatus },
}
```

`organize.reopen`, `organize.retry_snapshot`, and `organize.delete_task` use dedicated input structs with `task_id` and `expected_revision`. Their outputs are tagged with `Applied`, `StaleRevision`, and `RejectedState`; retry also returns `JobReceipt`. Business rejections are output variants so TypeScript can branch without parsing error text. Database, serialization, and job-dispatch failures remain transport errors and must roll back the active transaction.

### Decision and progress state

- A file, empty directory, reparse point, or unreadable item is one unit. A readable non-empty directory is the sum of child units.
- Only explicit roots persist decisions. The closest explicit ancestor supplies the effective decision and operation state.
- Keep and null decisions always have `operation_state = none`. Discard/Move set `pending`; commit changes them to `running`, then `applied` or `failed`.
- Applied roots are immutable. Failed roots remain processed, mutable, and retryable.
- Same-decision directory compression removes redundant descendants. Move is the same decision only when normalized destinations match.
- Unmarked descendants do not conflict with a parent decision. Mixed explicit descendants require no-mutation confirmation.
- Confirmed ancestor split clears the covering ancestor, leaves siblings unmarked, and inserts only the requested child decision. Cleared/overridden descendants are not restored from history.
- Batch normalization sorts by `tree_start` and drops selected descendants covered by a selected ancestor.
- Progress always satisfies `keep + discard + move = processed` and `processed + unmarked = total`.

### Commit and recovery state

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrganizeCommitPhase { Preflight, MoveGroups, DeleteRoots, Reconcile, Settle }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeCommitCheckpoint {
	pub phase: OrganizeCommitPhase,
	pub next_move_group: usize,
	pub active_child_job_id: Option<JobId>,
	pub delete_dispatched: bool,
	pub completed_root_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Job)]
pub struct OrganizeCommitJob {
	pub task_id: Uuid,
	pub locked_revision: i64,
	pub plan: OrganizeCommitPlanOutput,
	pub move_conflict_policy: FileConflictResolution,
	pub allow_current_subtree_drift: bool,
	pub checkpoint: OrganizeCommitCheckpoint,
}
```

Preflight completes for every physical root before any child job dispatch. Move groups run first. One confirmed `DeleteJob::permanent` receives all remaining compact Discard roots second. Before dispatch and after receiving the child job ID, persist root operation states and checkpoint state. On resume, use `JobManager::get_job` or `get_job_info`, then reconcile source existence before any later group dispatch. Cancellation stops later groups but never rewrites a settled child result.

### Shared preview contract

```rust
pub struct PreviewSequenceContext { pub task_id: Uuid, pub item_id: Uuid }
pub struct PreviewSequenceInput {
	pub directory: SdPath,
	pub organize: Option<PreviewSequenceContext>,
	pub limit: u32,
}
pub struct PreviewSequenceOutput {
	pub files: Vec<File>,
	pub observed_image_count: u32,
	pub observed_video_count: u32,
	pub candidate_budget_exhausted: bool,
}
```

Live preview inspects at most 128 directories, 4,096 entries, and 256 media candidates with breadth-first traversal. Manifest preview reads only the selected included interval. Both paths feed the same deterministic branch round-robin sampler, return at most 12 items, include at least one video when any exists, and cap videos at three when images exist.

## ORG-BE-01: Windows Path, Tree, Decision, and Planning Rules

**Depends on:** none

**Files:**
- Create: `core/src/ops/organize/{mod.rs,model.rs,error.rs,path.rs,tree.rs}`
- Modify: `core/src/ops/mod.rs`
- Test: colocated `#[cfg(test)]` modules in `path.rs` and `tree.rs`

**Input/output and failure contract:** Pure path functions accept Windows paths only and return `OrganizeError::UnsupportedPlatform`, `InvalidPhysicalPath`, or `UnsafeTopology` without filesystem mutation. Pure tree functions accept flat depth-first drafts/decision roots and return exact intervals, unit counts, conflicts, progress, and compact physical roots.

- [ ] **Step 1: Add RED tests for Windows identity and overlap**

```rust
#[test]
fn normalizes_drive_unc_and_sibling_prefixes() {
	assert_eq!(windows_path_key(Path::new(r"c:/Photos/Trip/"), false).unwrap(), r"c:\photos\trip");
	assert_eq!(windows_path_key(Path::new(r"\\?\UNC\NAS\Media\"), false).unwrap(), r"\\nas\media");
	assert!(paths_overlap(r"c:\photo", r"C:\PHOTO\2026"));
	assert!(!paths_overlap(r"c:\photo", r"C:\photos"));
	assert_eq!(windows_path_key(Path::new(r"C:\"), true).unwrap(), r"c:\");
}

#[test]
fn rejects_destination_inside_source_and_move_cycles() {
	let sources = vec![(Uuid::from_u128(1), r"c:\a".into(), r"c:\b".into()), (Uuid::from_u128(2), r"c:\b".into(), r"c:\a".into())];
	assert!(matches!(validate_move_topology(&sources, &[]), Err(OrganizeError::UnsafeTopology(_))));
	assert!(matches!(validate_move_destination(r"c:\a", r"c:\a\child", &[]), Err(OrganizeError::UnsafeTopology(_))));
}
```

- [ ] **Step 2: Run RED path tests**

Run: `cargo test -p sd-core ops::organize::path::tests -- --nocapture`

Expected: FAIL because `ops::organize`, `windows_path_key`, overlap, and topology validators do not exist.

- [ ] **Step 3: Implement the minimal Windows path API**

```rust
pub struct WindowsPathIdentity {
	pub display_path: PathBuf,
	pub path_key: String,
	pub device_slug: String,
	pub is_volume_root: bool,
}

pub async fn canonicalize_task_root(root: &SdPath) -> Result<WindowsPathIdentity, OrganizeError>;
pub fn windows_path_key(path: &Path, preserve_volume_root_separator: bool) -> Result<String, OrganizeError>;
pub fn paths_overlap(left_key: &str, right_key: &str) -> bool;
pub fn is_path_ancestor(ancestor_key: &str, descendant_key: &str) -> bool;
pub fn validate_move_destination(source_key: &str, destination_key: &str, discard_keys: &[String]) -> Result<(), OrganizeError>;
pub fn validate_move_topology(moves: &[(Uuid, String, String)], discard_keys: &[String]) -> Result<(), OrganizeError>;
```

Implementation uses component-aware comparison, never raw `starts_with`, and delegates extended-prefix stripping to `crate::common::utils::strip_windows_extended_prefix`.

- [ ] **Step 4: Add RED table/property tests for units, conflicts, and compaction**

```rust
#[test]
fn units_do_not_double_count_non_empty_directories() {
	let tree = build_tree(vec![dir(""), file("a.jpg", 10), dir("album"), file("album/b.jpg", 20), dir("empty")]).unwrap();
	assert_eq!(tree.node("").unit_count, 3);
	assert_eq!(tree.node("album").unit_count, 1);
	assert_eq!(tree.node("empty").unit_count, 1);
}

#[test]
fn all_discard_descendants_collapse_but_mixed_descendants_confirm() {
	let fixture = decision_fixture();
	let collapse = resolve_set_decision(&fixture, &[fixture.parent], DecisionValue::discard(), false, false).unwrap();
	assert!(matches!(collapse, DecisionResolution::Apply(ref patch) if patch.delete_roots.len() == 2 && patch.upsert_roots.len() == 1));

	let mixed = mixed_decision_fixture();
	let outcome = resolve_set_decision(&mixed, &[mixed.parent], DecisionValue::discard(), false, false).unwrap();
	assert!(matches!(outcome, DecisionResolution::ConfirmationRequired { keep_units: 2, move_units: 3, affected_bytes: 500, .. }));
}

#[test]
fn ancestor_split_leaves_siblings_unmarked_and_applied_is_immutable() {
	let fixture = ancestor_keep_fixture();
	let split = resolve_set_decision(&fixture, &[fixture.child], DecisionValue::discard(), false, true).unwrap();
	assert!(matches!(split, DecisionResolution::Apply(ref patch) if patch.delete_roots == vec![fixture.parent] && patch.upsert_roots[0].item_id == fixture.child));
	assert!(matches!(resolve_set_decision(&applied_fixture(), &[fixture.child], DecisionValue::keep(), true, true), Err(OrganizeError::AppliedDecisionImmutable(_))));
}

#[test]
fn generated_progress_invariants_hold() {
	for seed in 0..128_u64 {
		let fixture = generated_tree(seed, 64);
		let progress = reduce_progress(&fixture.nodes, &fixture.decisions).unwrap();
		assert!(progress.processed_units <= progress.total_units);
		assert_eq!(progress.keep_units + progress.discard_units + progress.move_units, progress.processed_units);
		assert_eq!(progress.processed_units + progress.unmarked_units, progress.total_units);
	}
}
```

- [ ] **Step 5: Run RED tree tests**

Run: `cargo test -p sd-core ops::organize::tree::tests -- --nocapture`

Expected: FAIL because tree drafts, decision resolution, progress reduction, and compaction are absent.

- [ ] **Step 6: Implement the minimal pure tree API**

```rust
pub struct TreeItemDraft { pub item_id: Uuid, pub parent_item_id: Option<Uuid>, pub kind: OrganizeItemKind, pub size_bytes: u64 }
pub struct TreeItemComputed { pub item_id: Uuid, pub tree_start: i64, pub tree_end: i64, pub unit_count: u64, pub aggregate_size_bytes: u64 }
pub struct ExplicitDecisionRoot { pub item_id: Uuid, pub tree_start: i64, pub tree_end: i64, pub unit_count: u64, pub aggregate_size_bytes: u64, pub decision: DecisionValue, pub operation_state: OrganizeOperationState }
pub struct DecisionPatch { pub delete_roots: Vec<Uuid>, pub upsert_roots: Vec<ExplicitDecisionRoot> }
pub enum DecisionResolution {
	Apply(DecisionPatch),
	ConfirmationRequired { conflict_kind: OrganizeDecisionConflictKind, keep_units: u64, discard_units: u64, move_units: u64, unmarked_units: u64, affected_bytes: u64, conflicting_roots: Vec<Uuid> },
	InheritedNoOp { ancestor_item_id: Uuid },
}

pub fn compute_tree(items: &[TreeItemDraft]) -> Result<Vec<TreeItemComputed>, OrganizeError>;
pub fn normalize_selection(selected: &[Uuid], intervals: &HashMap<Uuid, (i64, i64)>) -> Result<Vec<Uuid>, OrganizeError>;
pub fn resolve_set_decision(state: &DecisionTreeState, selected: &[Uuid], requested: Option<DecisionValue>, confirm_descendant_override: bool, confirm_ancestor_split: bool) -> Result<DecisionResolution, OrganizeError>;
pub fn reduce_progress(nodes: &[TreeItemComputed], decisions: &[ExplicitDecisionRoot]) -> Result<OrganizeProgressSummary, OrganizeError>;
pub fn compact_operation_roots(decisions: &[ExplicitDecisionRoot]) -> Vec<ExplicitDecisionRoot>;
```

The compactor sorts by `tree_start` and rejects/drops nested physical roots defensively. The execution sequence type always stores Move groups before Discard roots.

- [ ] **Step 7: Run GREEN rule tests and format**

Run: `cargo test -p sd-core ops::organize::path::tests -- --nocapture`

Run: `cargo test -p sd-core ops::organize::tree::tests -- --nocapture`

Expected: PASS for path, interval, unit, sparse progress, conflict, ancestor split, immutable applied, normalization, compaction, ordering, and generated invariant cases.

Run: `cargo fmt --check`

Expected: PASS.

- [ ] **Step 8: Commit ORG-BE-01**

```bash
git add core/src/ops/mod.rs core/src/ops/organize/mod.rs core/src/ops/organize/model.rs core/src/ops/organize/error.rs core/src/ops/organize/path.rs core/src/ops/organize/tree.rs
git commit -m "feat(organize): add recursive task rules"
```

## ORG-BE-02: Two-Table Migration and Transactional Repository

**Depends on:** ORG-BE-01

**Files:**
- Create: `core/src/infra/db/migration/m20260827_000001_create_organize_tasks.rs`
- Create: `core/src/infra/db/entities/{organize_task.rs,organize_task_item.rs}`
- Modify: `core/src/infra/db/migration/mod.rs`, `core/src/infra/db/entities/mod.rs`
- Create: `core/src/ops/organize/repository.rs`
- Create test: `core/tests/organize_repository.rs`

**Input/output and failure contract:** Repository writes are transaction-scoped and return `OrganizeError::StaleRevision`, `InvalidTaskState`, `AppliedDecisionImmutable`, or `sea_orm::DbErr`. Confirmation and stale outcomes commit zero rows. Revision increments exactly once per successful decision, accepted scope change, operation settlement, finish, or reopen.

- [ ] **Step 1: Add RED migration/index/constraint tests**

```rust
#[tokio::test]
async fn migration_creates_only_two_organize_tables_and_required_indexes() {
	let db = migrated_temp_db().await;
	let names = sqlite_names(db.conn(), "table", "organize_%").await;
	assert_eq!(names, vec!["organize_task_items", "organize_tasks"]);
	let indexes = sqlite_names(db.conn(), "index", "idx_organize_%").await;
	assert!(indexes.contains(&"idx_organize_items_task_parent_name".to_string()));
	assert!(indexes.contains(&"idx_organize_items_task_decision_tree".to_string()));
	assert!(indexes.contains(&"idx_organize_items_task_membership_external".to_string()));
}

#[tokio::test]
async fn move_destination_and_operation_state_checks_reject_invalid_rows() {
	let db = migrated_temp_db().await;
	assert!(insert_invalid_move_without_destination(db.conn()).await.is_err());
	assert!(insert_keep_with_pending_operation(db.conn()).await.is_err());
}
```

- [ ] **Step 2: Run RED repository test target**

Run: `cargo test -p sd-core --test organize_repository migration_ -- --nocapture`

Expected: FAIL because the migration, entities, and helper target do not exist.

- [ ] **Step 3: Implement the schema and entities**

Use string columns for enum persistence, UUID columns for wire identity, signed `i64` for counts/bytes/revision, and UTC timestamps. Add a partial unique SQLite index for `(task_id, tree_start) WHERE membership_state = 'included'`. Add foreign keys `task_id ON DELETE CASCADE`, `parent_id ON DELETE CASCADE`, optional `volume_id ON DELETE SET NULL`, and optional `root_entry_uuid`/`entry_uuid` without a hard foreign key because indexed accelerators may disappear.

Entity fields must match the approved schema exactly. `tree_start`, `tree_end`, and `unit_count` are `Option<i64>` only to represent pending additions. Neither model implements or registers `Syncable`.

- [ ] **Step 4: Add RED overlap, paging, cascade, and decision transaction tests**

```rust
#[tokio::test]
async fn overlap_blocks_equal_ancestor_and_descendant_but_not_sibling_or_completed() {
	let repo = seeded_repository().await;
	let active = repo.insert_task(task(r"C:\Photos", OrganizeTaskStatus::Active)).await.unwrap();
	assert_eq!(repo.find_overlapping_active(r"c:\photos").await.unwrap(), Some(active.id));
	assert_eq!(repo.find_overlapping_active(r"C:\Photos\Trips").await.unwrap(), Some(active.id));
	assert_eq!(repo.find_overlapping_active(r"C:\").await.unwrap(), Some(active.id));
	assert_eq!(repo.find_overlapping_active(r"C:\Photographs").await.unwrap(), None);
	repo.set_completed(active.id).await.unwrap();
	assert_eq!(repo.find_overlapping_active(r"C:\Photos\Trips").await.unwrap(), None);
}

#[tokio::test]
async fn decision_transaction_is_atomic_and_revision_increments_once() {
	let repo = seeded_decision_repository().await;
	let before = repo.get_task_revision(TASK_ID).await.unwrap();
	let outcome = repo.apply_decision(DecisionTransactionRequest::discard_directory(TASK_ID, PARENT_ID, before, true)).await.unwrap();
	assert!(matches!(outcome, OrganizeDecisionOutcome::Applied { revision, .. } if revision == before + 1));
	assert_eq!(repo.explicit_decision_ids(TASK_ID).await.unwrap(), vec![PARENT_ID]);
}

#[tokio::test]
async fn stale_and_confirmation_results_change_nothing() {
	let repo = seeded_mixed_decision_repository().await;
	let snapshot = repo.dump_decisions(TASK_ID).await.unwrap();
	assert!(matches!(repo.apply_decision(request_with_revision(0)).await.unwrap(), OrganizeDecisionOutcome::StaleRevision { .. }));
	assert_eq!(repo.dump_decisions(TASK_ID).await.unwrap(), snapshot);
	assert!(matches!(repo.apply_decision(unconfirmed_parent_override()).await.unwrap(), OrganizeDecisionOutcome::ConfirmationRequired { .. }));
	assert_eq!(repo.dump_decisions(TASK_ID).await.unwrap(), snapshot);
}

#[tokio::test]
async fn direct_children_filter_exclusions_and_cursor_are_stable() {
	let repo = repository_with_equal_names(450).await;
	let first = repo.children(children_request(None, 200)).await.unwrap();
	let second = repo.children(children_request(first.next_cursor.clone(), 200)).await.unwrap();
	let third = repo.children(children_request(second.next_cursor.clone(), 200)).await.unwrap();
	let ids = first.items.into_iter().chain(second.items).chain(third.items).map(|item| item.item_id).collect::<HashSet<_>>();
	assert_eq!(ids.len(), 450);
	let selected = repo.resolve_selection(TASK_ID, REVISION, direct_children_unmarked(vec![EXCLUDED_ID])).await.unwrap();
	assert!(!selected.contains(&EXCLUDED_ID));
}
```

- [ ] **Step 5: Run RED transaction tests**

Run: `cargo test -p sd-core --test organize_repository -- --nocapture`

Expected: FAIL because `OrganizeRepository` and transaction APIs are absent.

- [ ] **Step 6: Implement the repository API**

```rust
pub struct OrganizeRepository<'db> { db: &'db DatabaseConnection }

impl<'db> OrganizeRepository<'db> {
	pub fn new(db: &'db DatabaseConnection) -> Self;
	pub async fn insert_scanning_task(&self, draft: NewOrganizeTask) -> Result<organize_task::Model, OrganizeError>;
	pub async fn find_overlapping_active(&self, root_path_key: &str) -> Result<Option<Uuid>, OrganizeError>;
	pub async fn replace_included_snapshot(&self, task_id: Uuid, items: Vec<SnapshotItemDraft>, totals: SnapshotTotals) -> Result<i64, OrganizeError>;
	pub async fn fail_snapshot(&self, task_id: Uuid, message: String) -> Result<(), OrganizeError>;
	pub async fn list_tasks(&self, input: OrganizeListInput) -> Result<OrganizeListOutput, OrganizeError>;
	pub async fn get_task(&self, task_id: Uuid) -> Result<OrganizeGetOutput, OrganizeError>;
	pub async fn children(&self, input: OrganizeChildrenInput) -> Result<OrganizeChildrenOutput, OrganizeError>;
	pub async fn resolve_selection(&self, task_id: Uuid, expected_revision: i64, selection: OrganizeSelectionInput) -> Result<Vec<organize_task_item::Model>, OrganizeError>;
	pub async fn apply_decision(&self, request: DecisionTransactionRequest) -> Result<OrganizeDecisionOutcome, OrganizeError>;
	pub async fn store_change_scan(&self, task_id: Uuid, result: ChangeScanResult) -> Result<i64, OrganizeError>;
	pub async fn accept_changes(&self, input: OrganizeAcceptChangesInput) -> Result<OrganizeAcceptChangesOutcome, OrganizeError>;
	pub async fn lock_for_commit(&self, task_id: Uuid, expected_revision: i64, job_id: JobId) -> Result<i64, OrganizeError>;
	pub async fn settle_operation_roots(&self, task_id: Uuid, settlements: Vec<OperationSettlement>) -> Result<i64, OrganizeError>;
	pub async fn finish(&self, input: OrganizeFinishInput) -> Result<OrganizeFinishOutcome, OrganizeError>;
	pub async fn reopen(&self, task_id: Uuid, expected_revision: i64) -> Result<OrganizeLifecycleOutcome, OrganizeError>;
	pub async fn delete_task_metadata(&self, task_id: Uuid, expected_revision: i64) -> Result<OrganizeLifecycleOutcome, OrganizeError>;
}
```

Use `DatabaseTransaction` for snapshot replacement, decisions, accepted changes, commit lock, settlement, and lifecycle transitions. Query filters compute effective decision/progress using included intervals and sparse explicit roots, not cached aggregate columns.

- [ ] **Step 7: Run GREEN repository tests**

Run: `cargo test -p sd-core --test organize_repository -- --nocapture`

Expected: PASS for schema, constraints, no sync registration, overlap, cascade metadata deletion, paging, selection scope, transaction atomicity, stale no-op, confirmation no-op, accepted denominator changes, and interval rebuild.

- [ ] **Step 8: Commit ORG-BE-02**

```bash
git add core/src/infra/db/migration/mod.rs core/src/infra/db/migration/m20260827_000001_create_organize_tasks.rs core/src/infra/db/entities/mod.rs core/src/infra/db/entities/organize_task.rs core/src/infra/db/entities/organize_task_item.rs core/src/ops/organize/repository.rs core/tests/organize_repository.rs
git commit -m "feat(organize): persist recursive task manifests"
```

## ORG-BE-03: Create, Snapshot, List, Detail, Children, and Reopen Data

**Depends on:** ORG-BE-02

**Files:**
- Create: `core/src/ops/organize/create/{mod.rs,action.rs}`
- Create: `core/src/ops/organize/snapshot/{mod.rs,job.rs,windows.rs,unsupported.rs}`
- Create: `core/src/ops/organize/query/{mod.rs,list.rs,get.rs,children.rs,resolve_root.rs}`
- Modify: `core/src/ops/organize/mod.rs`
- Extend test: `core/tests/organize_task_flow.rs`

**Input/output and failure contract:** Creation canonicalizes before inserting. Missing/not-directory/permission/unsupported/overlap outcomes leave no orphan row. A root enumeration failure clears partial included rows and marks the task failed. Descendant failures persist one unreadable issue item and allow activation.

- [ ] **Step 1: Add RED Windows snapshot tests**

```rust
#[cfg(windows)]
#[tokio::test]
async fn snapshot_is_recursive_metadata_only_and_stable_after_live_addition() {
	let fixture = WindowsTreeFixture::new().await;
	fixture.file("album/one.JPG", b"one").await;
	fixture.hidden_file("album/.hidden.png", b"hidden").await;
	fixture.empty_dir("empty").await;
	let result = scan_windows_snapshot(fixture.root(), TEST_TASK_ID, TEST_DEVICE_SLUG, progress_sink()).await.unwrap();
	assert_eq!(result.root.unit_count, 3);
	assert!(result.items.iter().any(|item| item.relative_path == r"album\one.JPG"));
	assert!(result.items.iter().any(|item| item.relative_path == r"album\.hidden.png"));
	assert!(result.items.iter().all(|item| item.content_hash.is_none()));
	fixture.file("album/later.jpg", b"later").await;
	assert_eq!(result.root.unit_count, 3);
}

#[cfg(windows)]
#[tokio::test]
async fn reparse_point_is_one_leaf_and_is_not_followed() {
	let fixture = reparse_fixture_or_skip().await;
	let result = scan_windows_snapshot(fixture.root(), TEST_TASK_ID, TEST_DEVICE_SLUG, progress_sink()).await.unwrap();
	let link = result.items.iter().find(|item| item.relative_path == "linked").unwrap();
	assert_eq!(link.kind, OrganizeItemKind::ReparsePoint);
	assert_eq!(link.unit_count, 1);
	assert!(!result.items.iter().any(|item| item.relative_path.starts_with(r"linked\")));
}
```

- [ ] **Step 2: Run RED snapshot tests**

Run: `cargo test -p sd-core --test organize_task_flow -- --nocapture`

Expected: FAIL because snapshot modules and test fixture APIs do not exist.

- [ ] **Step 3: Implement snapshot drafts, traversal, and persisted job**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotItemDraft {
	pub uuid: Uuid,
	pub parent_uuid: Option<Uuid>,
	pub relative_path: String,
	pub relative_path_key: String,
	pub name: String,
	pub extension: Option<String>,
	pub kind: OrganizeItemKind,
	pub size_bytes: u64,
	pub modified_at_100ns: Option<i64>,
	pub metadata_signature: [u8; 16],
	pub external_state: OrganizeExternalState,
}

#[derive(Debug, Clone, Serialize, Deserialize, Job)]
pub struct OrganizeSnapshotJob { pub task_id: Uuid, pub root_path: PathBuf, pub device_slug: String }

pub async fn scan_windows_snapshot(root: &Path, task_id: Uuid, device_slug: &str, progress: SnapshotProgressSink) -> Result<SnapshotScanResult, OrganizeError>;
pub fn metadata_signature(relative_path_key: &str, kind: OrganizeItemKind, size_bytes: u64, modified_at_100ns: Option<i64>, extension: Option<&str>) -> [u8; 16];
```

`OrganizeSnapshotJob::run` calls `tokio::task::spawn_blocking`, persists batches of at most 500 drafts, reports `GenericProgress` with discovered entry count, finalizes bottom-up intervals/units/bytes in one transaction, sets `status = active`, clears `scan_job_id`, and increments revision. The unsupported module implements the same scan entry function and returns `UnsupportedPlatform`.

- [ ] **Step 4: Add RED create/query/registry tests**

```rust
#[cfg(windows)]
#[tokio::test]
async fn create_returns_immediately_and_task_reopens_from_sqlite() {
	let app = OrganizeTestApp::new().await;
	let outcome = app.create(OrganizeCreateInput { root: app.root_sd_path(), name: None }).await.unwrap();
	let (task_id, receipt) = match outcome { OrganizeCreateOutcome::Created { task_id, snapshot_job, .. } => (task_id, snapshot_job), other => panic!("unexpected create outcome: {other:?}") };
	app.wait(receipt.id).await.unwrap();
	drop(app);
	let reopened = OrganizeTestApp::open_existing().await;
	let detail = reopened.get(task_id).await.unwrap();
	assert_eq!(detail.task.status, OrganizeTaskStatus::Active);
	assert_eq!(detail.task.root_path, reopened.expected_root_display());
}

#[tokio::test]
async fn children_clamps_page_and_uses_item_uuid_tiebreaker() {
	let app = active_task_with_children(350).await;
	let page = app.children(OrganizeChildrenInput { limit: 900, ..default_children_input(app.task_id(), app.root_item_id()) }).await.unwrap();
	assert_eq!(page.items.len(), 200);
	assert!(page.next_cursor.is_some());
	assert!(page.items.windows(2).all(|pair| stable_item_order(pair[0].clone(), pair[1].clone())));
}
```

- [ ] **Step 5: Run RED create/query tests**

Run: `cargo test -p sd-core --test organize_task_flow -- --nocapture`

Expected: FAIL because create/list/get/children/root-resolution operations are not registered.

- [ ] **Step 6: Implement and register actions/queries**

```rust
pub struct OrganizeCreateAction { input: OrganizeCreateInput }
impl LibraryAction for OrganizeCreateAction {
	type Input = OrganizeCreateInput;
	type Output = OrganizeCreateOutcome;
	fn from_input(input: Self::Input) -> Result<Self, String> { Ok(Self { input }) }
	async fn execute(self, library: Arc<Library>, _context: Arc<CoreContext>) -> Result<Self::Output, ActionError>;
	fn action_kind(&self) -> &'static str { "organize.create" }
	async fn validate(&self, _library: &Arc<Library>, _context: Arc<CoreContext>) -> Result<ValidationResult, ActionError> { Ok(ValidationResult::Success { metadata: None }) }
}
crate::register_library_action!(OrganizeCreateAction, "organize.create");

pub struct OrganizeListQuery { input: OrganizeListInput }
pub struct OrganizeGetQuery { input: OrganizeGetInput }
pub struct OrganizeChildrenQuery { input: OrganizeChildrenInput }
pub struct OrganizeResolveRootQuery { input: OrganizeResolveRootInput }
crate::register_library_query!(OrganizeListQuery, "organize.list");
crate::register_library_query!(OrganizeGetQuery, "organize.get");
crate::register_library_query!(OrganizeChildrenQuery, "organize.children");
crate::register_library_query!(OrganizeResolveRootQuery, "organize.resolve_root");
```

Default task name uses the directory name. For a volume root, resolve the existing volume display name and append `Organize`. Resolve optional `entry_uuid`/`volume_id` only as enrichment. Snapshot membership remains path-manifest authority.

- [ ] **Step 7: Run GREEN snapshot/create/read tests**

Run: `cargo test -p sd-core organize -- --nocapture`

Expected: PASS for Windows identity, metadata-only recursive scan, unreadable issue continuation, root failure cleanup, overlap outcome, page clamps, stable cursors, and restart reopen.

- [ ] **Step 8: Commit ORG-BE-03**

```bash
git add core/src/ops/organize/mod.rs core/src/ops/organize/create core/src/ops/organize/snapshot core/src/ops/organize/query core/tests/organize_task_flow.rs
git commit -m "feat(organize): create and reopen recursive snapshots"
```

## ORG-BE-04: Decisions, Recursive Progress, Drift, and Lifecycle

**Depends on:** ORG-BE-03

**Files:**
- Create: `core/src/ops/organize/decision/{mod.rs,action.rs}`
- Create: `core/src/ops/organize/snapshot/change_job.rs`
- Create: `core/src/ops/organize/lifecycle/{mod.rs,scan_changes.rs,accept_changes.rs,retry_snapshot.rs,finish.rs,reopen.rs,delete_task.rs}`
- Modify: `core/src/ops/organize/{mod.rs,repository.rs,model.rs}`
- Extend tests: `core/tests/{organize_repository.rs,organize_task_flow.rs}`

**Input/output and failure contract:** Every decision batch is all-or-nothing. Confirmation, stale revision, immutable applied root, and invalid status make no changes. Change scans leave task status active, set `scan_job_id`, disable commit/finish, and increment revision only when results settle. Accepted changes rebuild the included tree in one transaction.

- [ ] **Step 1: Add RED decision action tests**

```rust
#[tokio::test]
async fn set_decision_collapses_discard_and_returns_exact_mixed_confirmation() {
	let app = active_mixed_task().await;
	let collapse = app.set_decision(discard_children_request(app.revision())).await.unwrap();
	assert!(matches!(collapse, OrganizeDecisionOutcome::Applied { affected_roots, .. } if affected_roots == vec![app.album_id()]));
	let mixed = app.set_decision(unconfirmed_mixed_parent_discard(app.revision())).await.unwrap();
	assert!(matches!(mixed, OrganizeDecisionOutcome::ConfirmationRequired { keep_units: 2, move_units: 1, unmarked_units: 4, affected_bytes: 700, .. }));
}

#[tokio::test]
async fn inherited_same_is_noop_and_clear_requires_ancestor_split() {
	let app = task_with_parent_keep().await;
	assert!(matches!(app.set_decision(child_keep(app.revision())).await.unwrap(), OrganizeDecisionOutcome::InheritedNoOp { ancestor_item_id, .. } if ancestor_item_id == app.parent_id()));
	assert!(matches!(app.set_decision(clear_child_without_split(app.revision())).await.unwrap(), OrganizeDecisionOutcome::ConfirmationRequired { conflict_kind: OrganizeDecisionConflictKind::AncestorSplit, .. }));
}
```

- [ ] **Step 2: Run RED decision tests**

Run: `cargo test -p sd-core --test organize_repository -- --nocapture`

Expected: FAIL because `organize.set_decision` and transaction outcome mapping are absent.

- [ ] **Step 3: Implement and register the decision action**

```rust
pub struct OrganizeSetDecisionAction { input: OrganizeSetDecisionInput }
impl LibraryAction for OrganizeSetDecisionAction {
	type Input = OrganizeSetDecisionInput;
	type Output = OrganizeDecisionOutcome;
	fn from_input(input: Self::Input) -> Result<Self, String> { Ok(Self { input }) }
	async fn execute(self, library: Arc<Library>, _context: Arc<CoreContext>) -> Result<Self::Output, ActionError> {
		OrganizeRepository::new(library.db().conn()).apply_decision(self.input.into()).await.map_err(Into::into)
	}
	fn action_kind(&self) -> &'static str { "organize.set_decision" }
	async fn validate(&self, _library: &Arc<Library>, _context: Arc<CoreContext>) -> Result<ValidationResult, ActionError> { Ok(ValidationResult::Success { metadata: None }) }
}
crate::register_library_action!(OrganizeSetDecisionAction, "organize.set_decision");
```

The repository resolves `DirectChildren` under the same revision and filter, normalizes nested roots, then invokes the pure tree resolver inside one SQL transaction.

- [ ] **Step 4: Add RED change scan and lifecycle tests**

```rust
#[cfg(windows)]
#[tokio::test]
async fn change_scan_reports_add_missing_changed_without_denominator_change() {
	let app = snapshotted_disk_task().await;
	let before = app.get().await.task.progress.total_units;
	app.add_file("new.jpg", b"new").await;
	app.remove_file("gone.jpg").await;
	app.replace_file_with_new_metadata("changed.jpg", b"changed").await;
	let receipt = started_job(app.scan_changes(app.revision()).await.unwrap());
	app.wait(receipt.id).await.unwrap();
	let after = app.get().await.task;
	assert_eq!(after.progress.total_units, before);
	assert_eq!(after.pending_addition_count, 1);
	assert_eq!(after.missing_count, 1);
	assert_eq!(after.changed_count, 1);
}

#[tokio::test]
async fn accept_changes_rebuilds_units_and_finish_requires_explicit_unmarked_confirmation() {
	let app = task_with_scanned_changes().await;
	let accepted = app.accept(include_remove_refresh_input(app.revision())).await.unwrap();
	assert!(matches!(accepted, OrganizeAcceptChangesOutcome::Applied { .. }));
	assert!(matches!(app.finish(OrganizeFinishInput { task_id: app.task_id(), expected_revision: app.revision(), confirm_unmarked: false }).await.unwrap(), OrganizeFinishOutcome::ConfirmationRequired { unmarked_units } if unmarked_units > 0));
	assert!(matches!(app.finish(OrganizeFinishInput { task_id: app.task_id(), expected_revision: app.revision(), confirm_unmarked: true }).await.unwrap(), OrganizeFinishOutcome::Completed { .. }));
}
```

- [ ] **Step 5: Run RED drift/lifecycle tests**

Run: `cargo test -p sd-core --test organize_task_flow -- --nocapture`

Expected: FAIL because change-scan and lifecycle jobs/actions are absent.

- [ ] **Step 6: Implement change scan and lifecycle actions**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Job)]
pub struct OrganizeChangeScanJob { pub task_id: Uuid, pub expected_revision: i64, pub root_path: PathBuf }

pub struct OrganizeScanChangesAction { input: OrganizeScanChangesInput }
pub struct OrganizeAcceptChangesAction { input: OrganizeAcceptChangesInput }
pub struct OrganizeRetrySnapshotAction { input: OrganizeRetrySnapshotInput }
pub struct OrganizeFinishAction { input: OrganizeFinishInput }
pub struct OrganizeReopenAction { input: OrganizeReopenInput }
pub struct OrganizeDeleteTaskAction { input: OrganizeDeleteTaskInput }

crate::register_library_action!(OrganizeScanChangesAction, "organize.scan_changes");
crate::register_library_action!(OrganizeAcceptChangesAction, "organize.accept_changes");
crate::register_library_action!(OrganizeRetrySnapshotAction, "organize.retry_snapshot");
crate::register_library_action!(OrganizeFinishAction, "organize.finish");
crate::register_library_action!(OrganizeReopenAction, "organize.reopen");
crate::register_library_action!(OrganizeDeleteTaskAction, "organize.delete_task");
```

The scan compares normalized relative keys and metadata signatures. It excludes applied Discard/Move intervals from missing/change detection. Acceptance includes additions, removes missing rows, refreshes changed metadata, clears changed decisions unless explicitly preserved, confirms inherited Discard/Move additions before mutation, rebuilds intervals/units, and increments revision once.

- [ ] **Step 7: Run GREEN decision/drift/lifecycle tests**

Run: `cargo test -p sd-core organize -- --nocapture`

Expected: PASS for batch scope, compression, conflict confirmations, ancestor split, progress categories, drift storage, acceptance, scan lock, failed-scan recovery, finish, reopen, retry, and metadata-only task deletion.

- [ ] **Step 8: Commit ORG-BE-04**

```bash
git add core/src/ops/organize/model.rs core/src/ops/organize/repository.rs core/src/ops/organize/mod.rs core/src/ops/organize/decision core/src/ops/organize/snapshot/change_job.rs core/src/ops/organize/lifecycle core/tests/organize_repository.rs core/tests/organize_task_flow.rs
git commit -m "feat(organize): add recursive decisions and lifecycle"
```

## ORG-BE-05: Commit Plan, Global Preflight, Child Jobs, and Resume Settlement

**Depends on:** ORG-BE-04

**Files:**
- Create: `core/src/ops/organize/commit/{mod.rs,plan.rs,preflight.rs,action.rs,job.rs}`
- Create: `core/src/ops/organize/query/commit_plan.rs`
- Modify: `core/src/ops/organize/query/mod.rs`, `core/src/ops/organize/model.rs`, `core/src/ops/organize/repository.rs`
- Extend test: `core/tests/organize_task_flow.rs`

**Input/output and failure contract:** `organize.commit_plan` is read-only. `organize.commit` verifies confirmation/revision/state/plan, atomically sets `committing`, and dispatches one resumable parent job. Preflight drift blocks every side effect unless the explicit current-subtree override is true. Independent operation failures settle per root and the task returns active.

- [ ] **Step 1: Add RED plan/topology/preflight tests**

```rust
#[tokio::test]
async fn commit_plan_contains_move_groups_then_compact_discard_roots_and_no_keep_jobs() {
	let app = decided_commit_fixture().await;
	let plan = app.commit_plan(app.revision()).await.unwrap();
	assert_eq!(plan.move_groups.len(), 2);
	assert_eq!(plan.discard_roots.iter().map(|root| root.item_id).collect::<Vec<_>>(), vec![app.discard_parent_id()]);
	assert!(plan.move_groups.iter().flat_map(|group| &group.roots).all(|root| root.item_id != app.keep_id()));
}

#[cfg(windows)]
#[tokio::test]
async fn drift_preflight_performs_zero_moves_and_zero_deletes() {
	let app = decided_disk_fixture().await;
	app.add_file("discard-dir/unreviewed.jpg", b"new").await;
	let outcome = app.commit(commit_input(app.revision(), false)).await.unwrap();
	let job = started_commit(outcome);
	assert!(app.wait(job.id).await.is_err());
	assert!(app.source("move-me.jpg").exists());
	assert!(app.source("delete-me.jpg").exists());
	assert!(app.destination("move-me.jpg").not_exists());
}
```

- [ ] **Step 2: Run RED commit-plan/preflight tests**

Run: `cargo test -p sd-core --test organize_task_flow -- --nocapture`

Expected: FAIL because commit planner, query, action, and job do not exist.

- [ ] **Step 3: Implement plan and preflight APIs**

```rust
pub fn build_commit_plan(task: &organize_task::Model, decisions: &[organize_task_item::Model]) -> Result<OrganizeCommitPlanOutput, OrganizeError>;
pub async fn preflight_all_roots(repo: &OrganizeRepository<'_>, task: &organize_task::Model, plan: &OrganizeCommitPlanOutput, allow_current_subtree_drift: bool) -> Result<PreflightReport, OrganizeError>;
pub async fn preflight_file(root: &OrganizePlanRoot, snapshot: &organize_task_item::Model) -> Result<PreflightRootResult, OrganizeError>;
pub async fn preflight_directory(root: &OrganizePlanRoot, included_interval: &[organize_task_item::Model]) -> Result<PreflightRootResult, OrganizeError>;
```

File roots compare existence, kind, and signature. Directory roots enumerate current metadata without following reparse points and compare the full included interval. Missing explicit roots block until accepted. `allow_current_subtree_drift` permits current descendants only after the frontend's explicit warning; unsafe source/destination topology still blocks.

- [ ] **Step 4: Add RED execution, partial failure, and resume tests**

```rust
#[cfg(windows)]
#[tokio::test]
async fn commit_moves_before_permanent_delete_and_settles_each_root() {
	let app = executable_disk_fixture().await;
	let receipt = started_commit(app.commit(commit_input(app.revision(), false)).await.unwrap());
	app.wait(receipt.id).await.unwrap();
	assert!(app.destination("move-me.jpg").exists());
	assert!(app.source("delete-me.jpg").not_exists());
	assert!(app.source("delete-dir").not_exists());
	assert!(app.source("keep-me.jpg").exists());
	assert_eq!(app.operation_state(app.move_id()).await, OrganizeOperationState::Applied);
	assert_eq!(app.operation_state(app.discard_id()).await, OrganizeOperationState::Applied);
}

#[cfg(windows)]
#[tokio::test]
async fn failed_root_remains_retryable_while_independent_root_applies() {
	let app = partial_failure_fixture().await;
	let receipt = started_commit(app.commit(commit_input(app.revision(), false)).await.unwrap());
	let _ = app.wait(receipt.id).await;
	assert_eq!(app.operation_state(app.good_id()).await, OrganizeOperationState::Applied);
	assert_eq!(app.operation_state(app.bad_id()).await, OrganizeOperationState::Failed);
	assert!(app.item_error(app.bad_id()).await.contains("permission"));
}

#[cfg(windows)]
#[tokio::test]
async fn resumed_parent_reconciles_dispatched_child_before_next_group() {
	let app = interrupted_after_child_dispatch_fixture().await;
	app.restart_job_manager().await;
	app.wait_for_parent_resume().await;
	assert_eq!(app.child_dispatch_count_for_first_group(), 1);
	assert!(app.destination("first.jpg").exists());
	assert!(app.destination("second.jpg").exists());
}
```

- [ ] **Step 5: Run RED execution/resume tests**

Run: `cargo test -p sd-core --test organize_task_flow -- --nocapture`

Expected: FAIL because no parent commit job dispatches or reconciles existing child jobs.

- [ ] **Step 6: Implement query, action, and resumable parent job**

```rust
pub struct OrganizeCommitPlanQuery { input: OrganizeCommitPlanInput }
crate::register_library_query!(OrganizeCommitPlanQuery, "organize.commit_plan");

pub struct OrganizeCommitAction { input: OrganizeCommitInput }
crate::register_library_action!(OrganizeCommitAction, "organize.commit");

impl Job for OrganizeCommitJob {
	const NAME: &'static str = "organize_commit";
	const RESUMABLE: bool = true;
	const DESCRIPTION: Option<&'static str> = Some("Preflight and execute organize task actions");
}

impl DynJob for OrganizeCommitJob { fn job_name(&self) -> &'static str { Self::NAME } }

#[async_trait::async_trait]
impl JobHandler for OrganizeCommitJob {
	type Output = OrganizeCommitOutput;
	async fn run(&mut self, ctx: JobContext<'_>) -> JobResult<Self::Output>;
	async fn on_cancel(&mut self, ctx: &JobContext<'_>) -> JobResult<()>;
}
```

For each move group, dispatch `FileCopyJob::new(SdPathBatch::new(sources), destination).with_options(CopyOptions { delete_after_copy: true, conflict_resolution: Some(policy), overwrite: policy == FileConflictResolution::Overwrite, verify_checksum: false, preserve_timestamps: true, move_mode: Some(MoveMode::Move), copy_method: CopyMethod::Auto })`. For deletes, dispatch `DeleteJob::permanent(SdPathBatch::new(sources), true)`. Persist/checkpoint between groups, wait with `JobHandle::wait`, and reconcile each source path before settlement.

- [ ] **Step 7: Run GREEN commit tests**

Run: `cargo test -p sd-core --test organize_task_flow -- --nocapture`

Expected: PASS for read-only plan, compact roots, topology blocks, all-root preflight, move-before-delete, permanent confirmation, Keep no-op, independent group continuation, partial settlement, cancellation, and restart reconciliation.

- [ ] **Step 8: Commit ORG-BE-05**

```bash
git add core/src/ops/organize/model.rs core/src/ops/organize/repository.rs core/src/ops/organize/query/mod.rs core/src/ops/organize/query/commit_plan.rs core/src/ops/organize/commit core/tests/organize_task_flow.rs
git commit -m "feat(organize): execute preflighted task plans"
```

## ORG-PREV-01: Bounded Shared Preview Sequence Backend

**Depends on:** ORG-BE-03

**Files:**
- Create: `core/src/ops/files/preview_sequence/{mod.rs,query.rs,sampler.rs,walk.rs}`
- Modify: `core/src/ops/files/mod.rs`
- Create test: `core/tests/preview_sequence.rs`

**Input/output and failure contract:** Live scans accept only current-device Windows physical directories. Manifest scans require task/item ownership and stay inside the included interval. Budget exhaustion returns available samples with `candidate_budget_exhausted = true`; it is not an error. Missing/unreadable roots return a path-bearing query error.

- [ ] **Step 1: Add RED deterministic sampling tests**

```rust
#[test]
fn samples_many_branches_round_robin_and_caps_video_when_images_exist() {
	let candidates = media_candidates(&[("a", 6, 2), ("b", 6, 2), ("c", 6, 2)]);
	let selected = select_representatives(candidates, 12);
	assert_eq!(selected.len(), 12);
	assert_eq!(selected.iter().filter(|item| item.media_kind == PreviewMediaKind::Video).count(), 3);
	assert_eq!(selected[0].first_branch, "a");
	assert_eq!(selected[1].first_branch, "b");
	assert_eq!(selected[2].first_branch, "c");
	assert_eq!(selected, select_representatives(media_candidates(&[("a", 6, 2), ("b", 6, 2), ("c", 6, 2)]), 12));
}

#[test]
fn video_only_can_fill_sequence_and_mixed_always_includes_video() {
	assert_eq!(select_representatives(video_candidates(20), 12).len(), 12);
	assert!(select_representatives(mixed_candidates(), 12).iter().any(|item| item.media_kind == PreviewMediaKind::Video));
}
```

- [ ] **Step 2: Run RED sampler tests**

Run: `cargo test -p sd-core --test preview_sequence -- --nocapture`

Expected: FAIL because preview sampler types and functions do not exist.

- [ ] **Step 3: Implement the candidate and sampler API**

```rust
pub enum PreviewMediaKind { Image, Video }
pub struct PreviewCandidate {
	pub file: File,
	pub media_kind: PreviewMediaKind,
	pub first_branch: String,
	pub captured_at: Option<DateTime<Utc>>,
	pub modified_at: DateTime<Utc>,
	pub normalized_path: String,
}
pub fn select_representatives(candidates: Vec<PreviewCandidate>, limit: usize) -> Vec<PreviewCandidate>;
```

Sort each branch by captured time when present, otherwise modified descending, then normalized path. Take one per branch, then a second per branch, then global fill. Enforce the mixed-media video rules after deterministic ordering without randomization.

- [ ] **Step 4: Add RED live-budget and manifest-boundary tests**

```rust
#[cfg(windows)]
#[tokio::test]
async fn live_walk_stops_at_all_three_budgets_and_skips_reparse_points() {
	let fixture = oversized_preview_fixture().await;
	let scan = walk_preview_candidates(fixture.root(), PreviewBudget::default()).await.unwrap();
	assert!(scan.directories_seen <= 128);
	assert!(scan.entries_seen <= 4096);
	assert!(scan.candidates.len() <= 256);
	assert!(scan.budget_exhausted);
	assert!(!scan.visited_paths.contains(&fixture.reparse_target()));
}

#[tokio::test]
async fn organize_context_reads_fixed_interval_not_new_live_media() {
	let app = preview_manifest_fixture().await;
	app.add_live_media_after_snapshot("album/later.jpg").await;
	let output = app.preview(PreviewSequenceInput { directory: app.album_sd_path(), organize: Some(PreviewSequenceContext { task_id: app.task_id(), item_id: app.album_item_id() }), limit: 12 }).await.unwrap();
	assert!(!output.files.iter().any(|file| file.name == "later.jpg"));
}
```

- [ ] **Step 5: Run RED preview integration tests**

Run: `cargo test -p sd-core --test preview_sequence -- --nocapture`

Expected: FAIL because live walk, manifest candidate query, and wire query are absent.

- [ ] **Step 6: Implement and register `files.preview_sequence`**

```rust
pub struct PreviewBudget { pub max_directories: usize, pub max_entries: usize, pub max_candidates: usize }
impl Default for PreviewBudget { fn default() -> Self { Self { max_directories: 128, max_entries: 4096, max_candidates: 256 } } }
pub async fn walk_preview_candidates(root: &Path, budget: PreviewBudget) -> Result<PreviewWalkResult, OrganizeError>;
pub async fn manifest_preview_candidates(repo: &OrganizeRepository<'_>, context: PreviewSequenceContext) -> Result<Vec<PreviewCandidate>, OrganizeError>;

pub struct PreviewSequenceQuery { input: PreviewSequenceInput }
impl LibraryQuery for PreviewSequenceQuery {
	type Input = PreviewSequenceInput;
	type Output = PreviewSequenceOutput;
	fn from_input(input: Self::Input) -> QueryResult<Self> { Ok(Self { input }) }
	async fn execute(self, context: Arc<CoreContext>, session: SessionContext) -> QueryResult<Self::Output>;
}
crate::register_library_query!(PreviewSequenceQuery, "files.preview_sequence");
```

Use extension/media metadata only to classify candidates. Do not hash, decode, or generate sidecars. Enrich indexed candidates with existing `File::from_entry_uuids`; construct snapshot-authoritative fallback `File` values for unindexed rows.

- [ ] **Step 7: Run GREEN preview backend tests**

Run: `cargo test -p sd-core --test preview_sequence -- --nocapture`

Expected: PASS for one/many branches, image/video/mixed sets, deterministic output, candidate caps, three-video cap, reparse skip, budget status, and fixed manifest interval.

- [ ] **Step 8: Commit ORG-PREV-01**

```bash
git add core/src/ops/files/mod.rs core/src/ops/files/preview_sequence core/tests/preview_sequence.rs
git commit -m "feat(files): add bounded preview sequences"
```

## ORG-TS-01: Register the Wire Contract and Generate TypeScript DTOs

**Depends on:** ORG-BE-03, ORG-BE-04, ORG-BE-05, ORG-PREV-01

**Files:**
- Create: `core/tests/organize_wire_contract.rs`
- Regenerate: `packages/ts-client/src/generated/types.ts`
- Create: `packages/ts-client/src/__tests__/organizeContract.test.ts`

**Input/output and failure contract:** Every method name must match the approved wire name exactly. All public inputs/outputs and nested DTOs derive Specta `Type`. Generated TypeScript is the only backend-contract source available to React.

- [ ] **Step 1: Add RED registry tests**

```rust
#[test]
fn organize_registry_contains_exact_methods() {
	let actions = [
		"action:organize.create.input", "action:organize.set_decision.input", "action:organize.scan_changes.input",
		"action:organize.accept_changes.input", "action:organize.commit.input", "action:organize.retry_snapshot.input",
		"action:organize.finish.input", "action:organize.reopen.input", "action:organize.delete_task.input",
	];
	let queries = [
		"query:organize.list", "query:organize.get", "query:organize.children", "query:organize.resolve_root",
		"query:organize.commit_plan", "query:files.preview_sequence",
	];
	assert!(actions.iter().all(|method| LIBRARY_ACTIONS.contains_key(method)));
	assert!(queries.iter().all(|method| LIBRARY_QUERIES.contains_key(method)));
}

#[test]
fn type_extraction_contains_contract_roots() {
	let (actions, queries, _) = generate_spacedrive_api();
	let action_outputs = actions.iter().map(|item| item.output_type_name.as_str()).collect::<HashSet<_>>();
	let query_outputs = queries.iter().map(|item| item.output_type_name.as_str()).collect::<HashSet<_>>();
	assert!(action_outputs.contains("OrganizeCreateOutcome"));
	assert!(action_outputs.contains("OrganizeDecisionOutcome"));
	assert!(query_outputs.contains("OrganizeChildrenOutput"));
	assert!(query_outputs.contains("OrganizeCommitPlanOutput"));
	assert!(query_outputs.contains("PreviewSequenceOutput"));
}
```

- [ ] **Step 2: Run RED wire tests**

Run: `cargo test -p sd-core --test organize_wire_contract -- --nocapture`

Expected: FAIL on any missing registration, missing `Type` derive, or inconsistent nested DTO.

- [ ] **Step 3: Fix only registration/derive defects and generate types**

Run: `cargo run --bin generate_typescript_types`

Expected: rewrites `packages/ts-client/src/generated/types.ts` with all organize and preview DTOs plus exact `WIRE_METHODS` entries.

- [ ] **Step 4: Add generated-contract RED/GREEN test**

```ts
import {describe, expect, test} from 'bun:test';
import {WIRE_METHODS} from '../generated/types';
import type {OrganizeDecisionOutcome, OrganizeSelectionInput} from '../generated/types';

describe('recursive organize generated contract', () => {
	test('maps exact action and query methods', () => {
		expect(WIRE_METHODS.libraryActions['organize.set_decision']).toBe('action:organize.set_decision.input');
		expect(WIRE_METHODS.libraryQueries['organize.children']).toBe('query:organize.children');
		expect(WIRE_METHODS.libraryQueries['files.preview_sequence']).toBe('query:files.preview_sequence');
	});

	test('keeps tagged selection and outcome shapes', () => {
		const selection: OrganizeSelectionInput = {DirectChildren: {parent_item_id: crypto.randomUUID(), filter: 'unmarked', excluded_item_ids: []}};
		const outcome: OrganizeDecisionOutcome = {StaleRevision: {current_revision: 7}};
		expect('DirectChildren' in selection).toBe(true);
		expect('StaleRevision' in outcome).toBe(true);
	});
});
```

Run: `bun test packages/ts-client/src/__tests__/organizeContract.test.ts`

Expected: PASS using generated exports with no local DTO copies.

- [ ] **Step 5: Run GREEN type drift check**

Run: `bash scripts/check-ts-types.sh`

Expected: PASS with no generated diff.

- [ ] **Step 6: Commit ORG-TS-01**

```bash
git add core/tests/organize_wire_contract.rs packages/ts-client/src/generated/types.ts packages/ts-client/src/__tests__/organizeContract.test.ts
git commit -m "feat(organize): generate recursive task contracts"
```

## ORG-FE-01: Task Routes, Explorer Entry Points, Sidebar, and Persisted Task Navigation

**Depends on:** ORG-TS-01. This is the new-shell increment. ORG-TS-02 retires the old Explorer entry points later.

**Files:**
- Modify: `packages/interface/src/router.tsx`
- Modify: `packages/interface/src/components/TabManager/{TabManagerContext.tsx,TabNavigationSync.tsx}`
- Create: `packages/interface/src/components/SpacesSidebar/OrganizeTasksGroup.tsx`
- Modify: `packages/interface/src/components/SpacesSidebar/index.tsx`
- Modify: `packages/interface/src/routes/explorer/components/VirtualPathBar.tsx`
- Modify: `packages/interface/src/routes/explorer/hooks/useFileContextMenu.ts`
- Create: `packages/interface/src/routes/organize/{index.ts,OrganizeTasksPage.tsx,OrganizeTaskPage.tsx,useOrganizeTask.ts,OrganizeHeader.tsx,OrganizeFilters.tsx}`
- Test: `packages/interface/src/routes/organize/__tests__/lifecycle.test.ts`

**Input/output and failure contract:** Entry points appear only for physical paths. `organize.resolve_root` chooses Organize/Open Existing. Created tasks navigate immediately to `/organize/<taskId>`. Existing task metadata remains readable on non-Windows while mutating controls stay disabled.

Repository boundary for this task: add `/organize` and `/organize/:taskId` in `packages/interface/src/router.tsx`, and do not edit `ExplorerPaneBody.tsx`, Explorer `context.tsx`, `ShellLayout.tsx`, or `routes/explorer/organize`. The temporary new-route/old-ViewMode coexistence is a P1 integration state, not a releasable endpoint.

- [ ] **Step 1: Add RED route/sidebar/availability tests**

```ts
import {describe, expect, test} from 'bun:test';
import {deriveOrganizeEntryAction, taskSidebarRows} from '../useOrganizeTask';

describe('organize task entry and sidebar', () => {
	test('uses backend root availability without frontend path comparison', () => {
		expect(deriveOrganizeEntryAction('Creatable')).toEqual({kind: 'create'});
		expect(deriveOrganizeEntryAction({OpenExisting: {task_id: 'task-1'}})).toEqual({kind: 'open', taskId: 'task-1'});
	});

	test('shows five non-completed tasks and all-tasks row', () => {
		const rows = taskSidebarRows(makeTaskSummaries(8));
		expect(rows.filter((row) => row.kind === 'task')).toHaveLength(5);
		expect(rows.at(-1)).toEqual({kind: 'all'});
	});
});
```

- [ ] **Step 2: Run RED frontend route test**

Run: `bun test packages/interface/src/routes/organize/__tests__/lifecycle.test.ts`

Expected: FAIL because route helpers and task components do not exist.

- [ ] **Step 3: Implement typed query/mutation hooks and route state**

```ts
export type OrganizeLayoutMode = 'list' | 'grid';
export interface OrganizeTabState {
	currentItemId: string | null;
	layout: OrganizeLayoutMode;
	filter: OrganizeItemFilter;
	sort: OrganizeItemSort;
	direction: OrganizeSortDirection;
	scrollTop: number;
}

export function useOrganizeTask(taskId: string) {
	const detail = useLibraryQuery({type: 'organize.get', input: {task_id: taskId}});
	const setDecision = useLibraryMutation('organize.set_decision');
	const scanChanges = useLibraryMutation('organize.scan_changes');
	const commit = useLibraryMutation('organize.commit');
	return {detail, setDecision, scanChanges, commit};
}

export function deriveOrganizeEntryAction(availability: OrganizeRootAvailability): {kind: 'create'} | {kind: 'open'; taskId: string} | {kind: 'disabled'};
export function taskSidebarRows(tasks: OrganizeTaskSummary[]): Array<{kind: 'task'; task: OrganizeTaskSummary} | {kind: 'all'}>;
```

Extend `TabExplorerState` with `organizeStates: Record<string, OrganizeTabState>`, initialize it to `{}`, and migrate older localStorage records by filling the missing field. Selection does not enter persisted tab state.

- [ ] **Step 4: Implement routes, task list, sidebar group, and entry actions**

Add route objects with `<OrganizeTasksPage />` and `<OrganizeTaskPage />`. Mount `<OrganizeTasksGroup />` after `<SpaceSwitcher />` and before pinned space items/groups. The path-bar and directory context menu call `organize.resolve_root`, then either `organize.create` or `navigate('/organize/' + taskId)`.

The task page initially renders header/status/overall progress, filters, a loading center, and a right preview placeholder. It restores current item/layout/filter/sort/scroll from the active tab's `organizeStates[taskId]` and clears transient selection whenever `currentItemId` changes. Prefer the existing `useLibraryQuery` and `useLibraryMutation` hooks for every generated operation. Do not call the wire client directly from components.

- [ ] **Step 5: Run GREEN route/type checks**

Run: `bun test packages/interface/src/routes/organize/__tests__/lifecycle.test.ts`

Expected: PASS.

Run: `bun run --filter @sd/interface typecheck`

Expected: PASS with generated DTO imports and no response casts.

Run: `bun run --filter @sd/interface lint`

Expected: PASS for the new route, sidebar group, and Explorer entry-point edits.

- [ ] **Step 6: Commit ORG-FE-01**

```bash
git add packages/interface/src/router.tsx packages/interface/src/components/TabManager/TabManagerContext.tsx packages/interface/src/components/TabManager/TabNavigationSync.tsx packages/interface/src/components/SpacesSidebar/OrganizeTasksGroup.tsx packages/interface/src/components/SpacesSidebar/index.tsx packages/interface/src/routes/explorer/components/VirtualPathBar.tsx packages/interface/src/routes/explorer/hooks/useFileContextMenu.ts packages/interface/src/routes/organize
git commit -m "feat(organize): add recursive task routes"
```

## ORG-FE-02: Virtualized Direct Children, Ctrl/Shift/Ctrl+A, and Correct Lasso

**Depends on:** ORG-FE-01

**Files:**
- Create: `packages/interface/src/routes/organize/{selection.ts,lasso.ts,virtualization.ts,OrganizeVirtualList.tsx,OrganizeVirtualGrid.tsx,OrganizeItemCard.tsx,thumbnailCache.ts,useOrganizeThumbnail.ts,OrganizeThumbnail.tsx}`
- Modify: `packages/interface/src/routes/organize/OrganizeTaskPage.tsx`
- Modify: `packages/interface/package.json`, `bun.lock`
- Create: `packages/interface/src/test/setup-dom.ts`
- Test: `packages/interface/src/routes/organize/__tests__/{selection.test.ts,lasso.test.ts,virtualization.test.tsx,taskPresentation.test.tsx}`

**Input/output and failure contract:** Selection keys are task item UUIDs. This task creates an isolated reducer and lasso state; it must not import Explorer `SelectionContext`, `useSelection`, or old `OrganizeCenterPane`. Ctrl+A stores a backend `DirectChildren` scope plus exclusions, so it does not materialize all IDs. Lasso uses pointer-down baseline plus current intersections, supports backward shrink, and only intersects mounted virtual cards while edge scrolling mounts/recomputes more cards.

- [ ] **Step 1: Add RED selection and lasso reducer tests**

```ts
test('plain replaces, ctrl toggles, shift ranges, and navigation clears', () => {
	let state = createSelectionState();
	state = reduceSelection(state, {type: 'plainClick', itemId: 'b', orderedIds: ['a', 'b', 'c', 'd']});
	expect(selectedIds(state)).toEqual(['b']);
	state = reduceSelection(state, {type: 'ctrlClick', itemId: 'd'});
	expect(selectedIds(state)).toEqual(['b', 'd']);
	state = reduceSelection(state, {type: 'shiftClick', itemId: 'c', orderedIds: ['a', 'b', 'c', 'd']});
	expect(selectedIds(state)).toEqual(['b', 'c']);
	expect(selectedIds(reduceSelection(state, {type: 'directoryChanged'}))).toEqual([]);
});

test('ctrl+a uses direct children scope and tracks only exclusions', () => {
	let state = reduceSelection(createSelectionState(), {type: 'selectAll', parentItemId: 'root', filter: 'unmarked'});
	state = reduceSelection(state, {type: 'ctrlClick', itemId: 'visible-9'});
	expect(toWireSelection(state)).toEqual({DirectChildren: {parent_item_id: 'root', filter: 'unmarked', excluded_item_ids: ['visible-9']}});
});

test('shrinking lasso removes cards no longer intersected and ctrl lasso unions baseline', () => {
	const baseline = new Set(['fixed']);
	expect(computeLassoSelection(new Set(), new Set(['a', 'b', 'c']), false)).toEqual(new Set(['a', 'b', 'c']));
	expect(computeLassoSelection(new Set(), new Set(['b']), false)).toEqual(new Set(['b']));
	expect(computeLassoSelection(baseline, new Set(['b']), true)).toEqual(new Set(['fixed', 'b']));
});
```

- [ ] **Step 2: Run RED selection tests**

Run: `bun test packages/interface/src/routes/organize/__tests__/selection.test.ts packages/interface/src/routes/organize/__tests__/lasso.test.ts`

Expected: FAIL because reducers/adapters are absent.

- [ ] **Step 3: Implement selection/lasso contracts**

```ts
export type OrganizeSelectionState =
	| {kind: 'items'; itemIds: Set<string>; focusId: string | null; anchorId: string | null}
	| {kind: 'directChildren'; parentItemId: string; filter: OrganizeItemFilter; excludedItemIds: Set<string>; focusId: string | null; anchorId: string | null};

export function createSelectionState(): OrganizeSelectionState;
export function reduceSelection(state: OrganizeSelectionState, event: OrganizeSelectionEvent): OrganizeSelectionState;
export function toWireSelection(state: OrganizeSelectionState): OrganizeSelectionInput;
export function computeLassoSelection(pointerDownSelection: Set<string>, currentIntersections: Set<string>, ctrlKey: boolean): Set<string>;
export function intersectRenderedCards(rect: DOMRect, cards: Iterable<HTMLElement>): Set<string>;
export function edgeScrollVelocity(pointerY: number, viewport: DOMRect, edgeSize?: number, maxVelocity?: number): number;
```

Blank plain click dispatches clear; blank Ctrl-click leaves selection intact. Every pointer move recomputes from mounted DOM geometry and the captured baseline.

- [ ] **Step 4: Add RED DOM virtualization and card tests**

```tsx
test('10000 child fixture renders fewer than 300 item cards', async () => {
	const items = makeOrganizeItems(10_000);
	const view = render(<OrganizeVirtualGrid items={items} viewportHeight={900} columnWidth={180} overscanRows={2} onLoadMore={() => Promise.resolve()} />);
	expect(view.container.querySelectorAll('[data-organize-item-id]').length).toBeLessThan(300);
});

test('scrolling preserves decision identity by item uuid', async () => {
	const items = makeOrganizeItems(10_000, {decisionAt: 9000});
	const view = render(<OrganizeVirtualList items={items} viewportHeight={800} overscanRows={2} onLoadMore={() => Promise.resolve()} />);
	view.getByTestId('organize-scroll').scrollTop = 9_000 * 44;
	fireEvent.scroll(view.getByTestId('organize-scroll'));
	await waitFor(() => expect(view.container.querySelector('[data-organize-item-id="item-9000"] [data-decision="discard"]')).not.toBeNull());
});
```

- [ ] **Step 5: Install the scoped DOM test runtime and run RED**

Add `@testing-library/react` and `happy-dom` as interface dev dependencies. `setup-dom.ts` creates a `Window`, installs `window`, `document`, `HTMLElement`, `DOMRect`, `ResizeObserver`, `requestAnimationFrame`, and cleans `document.body` after each test.

Run: `bun test --preload packages/interface/src/test/setup-dom.ts packages/interface/src/routes/organize/__tests__/virtualization.test.tsx packages/interface/src/routes/organize/__tests__/taskPresentation.test.tsx`

Expected: FAIL because virtual components/cards are absent.

- [ ] **Step 6: Implement list/grid virtualization and thumbnail identity**

Both views may copy the established repository `useVirtualizer` setup pattern, but their sole item source is paged `useLibraryQuery` calls to `organize.children` with a limit of 200. They must not adapt Explorer entries, old organize local state, or the global selection provider. Load the next page when the last virtual row enters two-row overscan. Grid virtualizes rows, not 10,000 individual absolute elements. Export:

```ts
export function gridColumnCount(width: number, minimumCardWidth: number, gap: number): number;
export function virtualRowCount(itemCount: number, columnCount: number): number;
export function createThumbnailCacheKey(path: string, sizeBytes: number, modifiedAt: string, isDirectory: boolean): string;
```

The cache key is `${isDirectory ? 'dir' : 'file'}:${normalizedPath}@${sizeBytes}@${modifiedAt}`. `useOrganizeThumbnail` is called only inside mounted cards, so unrendered pages do not load thumbnails.

- [ ] **Step 7: Run GREEN selection/virtualization tests**

Run: `bun test packages/interface/src/routes/organize/__tests__/selection.test.ts packages/interface/src/routes/organize/__tests__/lasso.test.ts`

Run: `bun test --preload packages/interface/src/test/setup-dom.ts packages/interface/src/routes/organize/__tests__/virtualization.test.tsx packages/interface/src/routes/organize/__tests__/taskPresentation.test.tsx`

Expected: PASS, including the under-300 DOM bound and UUID decision identity after scroll.

Run: `bun run --filter @sd/interface typecheck`

Run: `bun run --filter @sd/interface lint`

Expected: both commands PASS without importing the global Explorer selection implementation.

- [ ] **Step 8: Commit ORG-FE-02**

```bash
git add packages/interface/package.json bun.lock packages/interface/src/test/setup-dom.ts packages/interface/src/routes/organize
git commit -m "feat(organize): virtualize task review and selection"
```

## ORG-FE-03: Decision Bar, Progress, Override Dialogs, and Move Picker

**Depends on:** ORG-FE-02, ORG-TS-01

**Files:**
- Create: `packages/interface/src/routes/organize/{OrganizeDecisionBar.tsx,OrganizeConflictDialog.tsx,OrganizeMovePicker.tsx}`
- Modify: `packages/interface/src/routes/organize/{OrganizeTaskPage.tsx,OrganizeItemCard.tsx,useOrganizeTask.ts,OrganizeFilters.tsx}`
- Test: `packages/interface/src/routes/organize/__tests__/{decisionFlow.test.ts,movePicker.test.ts,taskPresentation.test.tsx}`

**Input/output and failure contract:** Decision controls always serialize the complete selection scope. Confirmation text uses backend counts. `ConfirmationRequired` does not alter local query data. `StaleRevision` clears selection, refetches detail/children, and asks the user to retry. Move picker records a Move decision only.

- [ ] **Step 1: Add RED decision outcome and progress presentation tests**

```ts
test('decision request carries complete direct-children scope', () => {
	const input = buildSetDecisionInput('task', 8, directChildrenSelection('parent', 'unmarked', ['skip']), 'Discard');
	expect(input).toEqual({task_id: 'task', expected_revision: 8, selection: {DirectChildren: {parent_item_id: 'parent', filter: 'unmarked', excluded_item_ids: ['skip']}}, decision: 'Discard', confirm_descendant_override: false, confirm_ancestor_split: false});
});

test('confirmation copy is entirely backend-count driven', () => {
	const model = conflictDialogModel({ConfirmationRequired: {conflict_kind: 'descendant_override', keep_units: 4, discard_units: 2, move_units: 3, unmarked_units: 7, affected_bytes: 2048, conflicting_roots: ['a']}});
	expect(model).toMatchObject({keepUnits: 4, moveUnits: 3, affectedBytes: 2048, destructive: true});
});

test('directory progress keeps processed and categories exact', () => {
	const segments = progressSegments({total_units: 10, processed_units: 7, keep_units: 2, discard_units: 3, move_units: 2, unmarked_units: 3});
	expect(segments.map((segment) => segment.fraction)).toEqual([0.2, 0.3, 0.2, 0.3]);
});
```

- [ ] **Step 2: Run RED decision tests**

Run: `bun test packages/interface/src/routes/organize/__tests__/decisionFlow.test.ts packages/interface/src/routes/organize/__tests__/taskPresentation.test.tsx`

Expected: FAIL because request/outcome and progress presentation helpers are absent.

- [ ] **Step 3: Implement typed decision flow and dialogs**

```ts
export function buildSetDecisionInput(taskId: string, revision: number, selection: OrganizeSelectionState, decision: OrganizeDecisionInput | null): OrganizeSetDecisionInput;
export function conflictDialogModel(outcome: Extract<OrganizeDecisionOutcome, {ConfirmationRequired: unknown}>): ConflictDialogModel;
export function progressSegments(progress: OrganizeProgressSummary): Array<{kind: 'keep' | 'discard' | 'move' | 'unmarked'; fraction: number}>;
```

On `Applied`, invalidate `organize.get`, every `organize.children` page for the task, `organize.list`, and `organize.commit_plan`. On `InheritedNoOp`, retain data and show the ancestor source. On confirmation, resubmit the same immutable input with only the matching confirmation boolean set. Enter confirms only from the focused destructive button; Escape/outside cancel and do not call mutate.

- [ ] **Step 4: Add RED Move picker ordering test**

```ts
test('orders recent, locations, pinned physical paths, then browse', () => {
	const rows = buildMoveDestinationRows({
		recent: [sdPath('C:/Recent')],
		locations: [{id: 'loc', name: 'Photos', sd_path: sdPath('C:/Photos')}],
		pinned: [{id: 'pin', name: 'Archive', sdPath: sdPath('D:/Archive')}, {id: 'virtual', name: 'Ignored', sdPath: {Virtual: 'recent'}}],
	});
	expect(rows.map((row) => row.kind)).toEqual(['recent', 'location', 'pinned', 'browse']);
});
```

- [ ] **Step 5: Run RED Move picker test**

Run: `bun test packages/interface/src/routes/organize/__tests__/movePicker.test.ts`

Expected: FAIL because destination normalization/ordering is absent.

- [ ] **Step 6: Implement Move picker without a new persistence table**

```ts
export function buildMoveDestinationRows(input: {recent: SdPath[]; locations: Location[]; pinned: PinnedPathCandidate[]}): MoveDestinationRow[];
```

`recent` comes from distinct current/applied Move decision destinations returned in task detail, ordered by item `updated_at`, limited to five. Use existing location and space layout queries. Exclude non-physical pinned items. Native browse converts the selected Windows string to `{Physical: {device_slug: task.root_sd_path.Physical.device_slug, path}}`. Choosing a row closes the picker and submits `organize.set_decision`; it never opens `FileOperationModal`.

- [ ] **Step 7: Run GREEN decision/move/type tests**

Run: `bun test packages/interface/src/routes/organize/__tests__/decisionFlow.test.ts packages/interface/src/routes/organize/__tests__/movePicker.test.ts packages/interface/src/routes/organize/__tests__/taskPresentation.test.tsx`

Run: `bun run --filter @sd/interface typecheck`

Run: `bun run --filter @sd/interface lint`

Expected: both commands PASS with no local backend DTO definitions and no `as any` in `packages/interface/src/routes/organize`.

- [ ] **Step 8: Commit ORG-FE-03**

```bash
git add packages/interface/src/routes/organize
git commit -m "feat(organize): add subtree decisions and move picker"
```

## ORG-PREV-02: Shared Quick Preview Contact Sheet and Task Preview Pane

**Depends on:** ORG-PREV-01, ORG-TS-01, ORG-FE-02

**Files:**
- Modify: `packages/interface/src/components/QuickPreview/{ContentRenderer.tsx,DirectoryPreview.tsx,index.ts}`
- Create: `packages/interface/src/components/QuickPreview/PreviewSequence.tsx`
- Create test: `packages/interface/src/components/QuickPreview/__tests__/previewSequence.test.tsx`
- Create: `packages/interface/src/routes/organize/OrganizePreviewPane.tsx`
- Modify: `packages/interface/src/routes/organize/OrganizeTaskPage.tsx`

**Input/output and failure contract:** Focused files use their exact existing renderer. Directories use the bounded query. Selection alone never autoplays video. Query errors retain the focused item and render a retry action. Empty media uses a direct-child listing with limit 200 and virtualization.

- [ ] **Step 1: Add RED preview presentation tests**

```tsx
test('multiple samples render contact sheet and video tiles remain paused', () => {
	const view = render(<PreviewSequence output={mixedPreviewOutput(6)} onOpen={() => undefined} />);
	expect(view.container.querySelectorAll('[data-preview-sample]').length).toBe(6);
	expect(view.container.querySelector('video[autoplay]')).toBeNull();
});

test('one sample renders shared content and arrows move sequence focus', () => {
	const view = render(<PreviewSequence output={imagePreviewOutput(1)} onOpen={() => undefined} />);
	expect(view.container.querySelector('[data-shared-content-renderer]')).not.toBeNull();
	const multi = render(<PreviewSequence output={imagePreviewOutput(3)} onOpen={() => undefined} />);
	fireEvent.keyDown(multi.container, {key: 'ArrowRight'});
	expect(multi.container.querySelector('[data-preview-index="1"][data-focused="true"]')).not.toBeNull();
});

test('sampled result exposes budget status and empty media requests bounded children', () => {
	expect(previewPresentationModel({...imagePreviewOutput(2), candidate_budget_exhausted: true}).sampled).toBe(true);
	expect(buildFallbackChildrenInput(sdPath('C:/Empty'))).toEqual({path: sdPath('C:/Empty'), limit: 200, include_hidden: true, sort_by: 'name', folders_first: true});
});
```

- [ ] **Step 2: Run RED preview UI tests**

Run: `bun test --preload packages/interface/src/test/setup-dom.ts packages/interface/src/components/QuickPreview/__tests__/previewSequence.test.tsx`

Expected: FAIL because shared sequence components and helpers do not exist.

- [ ] **Step 3: Refactor Quick Preview around the generated query**

Export `ContentRenderer` and add:

```tsx
export interface DirectoryPreviewContext { organize?: PreviewSequenceContext }
export function DirectoryPreview({file, context}: {file: File; context?: DirectoryPreviewContext}): JSX.Element;
export function PreviewSequence({output, onOpen}: {output: PreviewSequenceOutput; onOpen: (file: File) => void}): JSX.Element;
export function previewPresentationModel(output: PreviewSequenceOutput): PreviewPresentationModel;
export function buildFallbackChildrenInput(path: SdPath): DirectoryListingInput;
```

Use `useLibraryQuery({type: 'files.preview_sequence', input: {directory: file.sd_path, organize: context?.organize ?? null, limit: 12}})`. A single image/video delegates to `ContentRenderer`. Multiple samples render posters/contact sheet. Opening a sample delegates to the existing Quick Preview controller/state. Video playback uses existing `VideoPlayer`, so `sd-video-volume` and `sd-video-muted` behavior remains unchanged.

- [ ] **Step 4: Mount the task right pane with item focus only**

```tsx
export function OrganizePreviewPane({item, taskId}: {item: OrganizeItemView | null; taskId: string}): JSX.Element {
	if (!item) return <EmptyPreview />;
	return <ContentRenderer file={item.file} directoryPreviewContext={item.item_kind === 'directory' ? {organize: {task_id: taskId, item_id: item.item_id}} : undefined} />;
}
```

The selection reducer's `focusId` chooses the preview item. Multi-selection does not synthesize a combined preview.

- [ ] **Step 5: Run GREEN preview and type tests**

Run: `bun test --preload packages/interface/src/test/setup-dom.ts packages/interface/src/components/QuickPreview/__tests__/previewSequence.test.tsx`

Run: `bun run --filter @sd/interface typecheck`

Expected: PASS for contact sheet, one-sample renderer, paused video, arrows, sampled indicator, bounded no-media list, and organize manifest context.

- [ ] **Step 6: Commit ORG-PREV-02**

```bash
git add packages/interface/src/components/QuickPreview packages/interface/src/routes/organize/OrganizePreviewPane.tsx packages/interface/src/routes/organize/OrganizeTaskPage.tsx
git commit -m "feat(preview): share bounded directory sequences"
```

## ORG-FE-04: Commit Review, Drift Review, Lifecycle, and Legacy Import

**Depends on:** ORG-BE-05, ORG-FE-03, ORG-PREV-02

**Files:**
- Create: `packages/interface/src/routes/organize/{OrganizeCommitDialog.tsx,OrganizeChangesPanel.tsx,OrganizeLifecycleDialogs.tsx}`
- Create: `packages/interface/src/routes/organize/legacy/{types.ts,importLegacy.ts,LegacyImportBanner.tsx}`
- Modify: `packages/interface/src/routes/organize/{OrganizeTasksPage.tsx,OrganizeTaskPage.tsx,OrganizeHeader.tsx,useOrganizeTask.ts}`
- Create: `apps/tauri/src-tauri/src/legacy_organize.rs`
- Modify: `apps/tauri/src-tauri/src/main.rs`, `apps/tauri/src/platform.ts`, `packages/interface/src/contexts/PlatformContext.tsx`
- Test: `packages/interface/src/routes/organize/__tests__/{commitFlow.test.ts,lifecycle.test.ts}`
- Test: colocated Rust tests in `apps/tauri/src-tauri/src/legacy_organize.rs`

**Input/output and failure contract:** Review always uses the current revision. Permanent delete confirmation is mandatory and conflict policy defaults to `AutoModifyName`. Change acceptance and legacy import archive only after all required backend transactions succeed. Completed tasks are read-only until reopened.

- [ ] **Step 1: Add RED commit/lifecycle frontend tests**

```ts
test('commit dialog renders compact plan and requires explicit permanent confirmation', () => {
	const model = commitDialogModel(commitPlanFixture());
	expect(model.discardRootCount).toBe(2);
	expect(model.moveGroupCount).toBe(1);
	expect(model.recycleBinWarning).toBe(true);
	expect(buildCommitInput('task', 12, model, false)).toMatchObject({permanent_delete_confirmed: false, move_conflict_policy: 'AutoModifyName', allow_current_subtree_drift: false});
});

test('stale commit and decision outcomes refetch instead of changing cache', async () => {
	const calls: string[] = [];
	await handleOrganizeOutcome({StaleRevision: {current_revision: 13}}, {refetch: async () => { calls.push('refetch'); }, mutateCache: () => { calls.push('mutate'); }});
	expect(calls).toEqual(['refetch']);
});

test('completed task is read only and reopen enables unapplied decisions', () => {
	expect(taskCapabilities(taskWithStatus('completed'))).toMatchObject({decide: false, commit: false, reopen: true});
	expect(taskCapabilities(taskWithStatus('active'))).toMatchObject({decide: true, commit: true, reopen: false});
});
```

- [ ] **Step 2: Run RED commit/lifecycle tests**

Run: `bun test packages/interface/src/routes/organize/__tests__/commitFlow.test.ts packages/interface/src/routes/organize/__tests__/lifecycle.test.ts`

Expected: FAIL because commit review, drift review, and lifecycle models are absent.

- [ ] **Step 3: Implement commit, drift, and lifecycle surfaces**

```ts
export function commitDialogModel(plan: OrganizeCommitPlanOutput): CommitDialogModel;
export function buildCommitInput(taskId: string, revision: number, model: CommitDialogModel, permanentDeleteConfirmed: boolean): OrganizeCommitInput;
export function taskCapabilities(task: OrganizeTaskSummary): {decide: boolean; scan: boolean; commit: boolean; finish: boolean; reopen: boolean; deleteRecord: boolean};
export async function handleOrganizeOutcome(outcome: {StaleRevision: {current_revision: number}}, effects: {refetch: () => Promise<unknown>; mutateCache: () => void}): Promise<void>;
```

The commit button first queries `organize.commit_plan`. Disable execution for `can_commit = false`, active change scan, or non-active status. Drift override requires a separate checked warning that current unreviewed descendants will be physically moved/deleted. Poll/invalidate task/list data through existing job updates until the parent job settles. Finish checks pending/running/failed operation counts; unmarked units invoke the second confirmation. Delete task record warns that files are untouched and is disabled while committing.

- [ ] **Step 4: Add RED retained legacy boundary tests**

```rust
#[tokio::test]
async fn lists_parses_and_archives_valid_legacy_json_without_write_command() {
	let root = legacy_fixture_root().await;
	write_legacy(&root, "dir-a", r#"{"version":1,"directoryPath":"C:/Photos","updatedAt":"2026-06-05T15:00:00Z","items":{"id:1":{"itemId":"1","path":"C:/Photos/a.jpg","name":"a.jpg","kind":"File","decision":"keep","updatedAt":"2026-06-05T15:00:00Z"}}}"#).await;
	let records = list_legacy_state_files(&root).await.unwrap();
	assert_eq!(records.len(), 1);
	let parsed = read_legacy_state_file(&root, &records[0].key).await.unwrap();
	assert_eq!(parsed.directory_path, "C:/Photos");
	archive_legacy_state_file(&root, &records[0].key).await.unwrap();
	assert!(root.join("organize/v1/dir-a.json.migrated").exists());
}
```

- [ ] **Step 5: Run RED Tauri legacy tests**

Run: `cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml legacy_organize::tests -- --nocapture`

Expected: FAIL because retained list/read/archive commands do not exist.

- [ ] **Step 6: Implement retained Tauri DTO and import orchestrator**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyOrganizeState { pub version: u32, pub directory_path: String, pub updated_at: String, pub items: BTreeMap<String, LegacyOrganizeItem> }

#[tauri::command]
pub async fn list_legacy_organize_states() -> Result<Vec<LegacyOrganizeStateSummary>, String>;
#[tauri::command]
pub async fn read_legacy_organize_state(key: String) -> Result<LegacyOrganizeState, String>;
#[tauri::command]
pub async fn archive_legacy_organize_state(key: String) -> Result<(), String>;
```

```ts
export async function importLegacyState(record: LegacyOrganizeState, api: LegacyImportApi): Promise<LegacyImportResult>;
```

The TypeScript orchestrator creates one task per non-overlapping `directoryPath`, waits until snapshot status is active, maps legacy item paths by normalized physical path against task children/descendants through paged reads, sends Keep/Discard through `organize.set_decision`, reports missing paths, and invokes archive only when every mappable decision applied. Move is never inferred. A failed create, snapshot, decision, or archive leaves the JSON unrenamed and reports the exact record/path.

- [ ] **Step 7: Run GREEN commit/lifecycle/legacy tests**

Run: `bun test packages/interface/src/routes/organize/__tests__/commitFlow.test.ts packages/interface/src/routes/organize/__tests__/lifecycle.test.ts`

Run: `cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml legacy_organize::tests -- --nocapture`

Run: `bun run --filter @sd/interface typecheck`

Expected: PASS for commit confirmation, stale refetch, drift acceptance, completed/reopen capability, metadata delete, and archive-after-success import behavior.

- [ ] **Step 8: Commit ORG-FE-04**

```bash
git add packages/interface/src/routes/organize apps/tauri/src-tauri/src/legacy_organize.rs apps/tauri/src-tauri/src/main.rs apps/tauri/src/platform.ts packages/interface/src/contexts/PlatformContext.tsx
git commit -m "feat(organize): add commit review and task recovery"
```

## ORG-TS-02: Remove Old Type Escapes, Retire ViewMode/JSON Writes, and Verify Generated Drift

**Depends on:** ORG-FE-04

**Files:**
- Delete: `apps/tauri/src-tauri/src/organize.rs`
- Delete: `packages/interface/src/routes/explorer/organize/` in full
- Delete: `packages/interface/src/organizeLayoutSizing.ts`, `packages/interface/src/__tests__/organizeLayoutSizing.test.ts`, `packages/interface/src/__tests__/organizeLayoutIntegration.test.ts`
- Modify: `packages/interface/src/routes/explorer/{context.tsx,ViewModeMenu.tsx}`
- Modify: `packages/interface/src/routes/explorer/panes/ExplorerPaneBody.tsx`
- Modify: `packages/interface/src/routes/explorer/views/{SearchView/SearchView.tsx,RecentsView/RecentsView.tsx}`
- Modify: `packages/ts-client/src/stores/viewPreferences.ts`
- Modify: `packages/interface/src/{ShellLayout.tsx,components/Inspector/Inspector.tsx,components/Inspector/variants/FileInspector.tsx}`
- Modify localization/help files listed in File Structure
- Regenerate: `packages/ts-client/src/generated/types.ts` and existing i18n generated output

**Input/output and failure contract:** No active code may call old load/save/delete organize JSON commands or select `viewMode = organize`. The retained Tauri boundary exposes only list/read/archive. New route code imports generated backend DTOs and contains no unsafe response casts.

- [ ] **Step 1: Add RED static retirement/type-safety test**

```ts
import {describe, expect, test} from 'bun:test';
import {readdirSync, readFileSync, statSync} from 'node:fs';
import {join} from 'node:path';

function sourceFiles(root: string): string[] {
	return readdirSync(root).flatMap((name) => {
		const path = join(root, name);
		return statSync(path).isDirectory() ? sourceFiles(path) : [path];
	}).filter((path) => /\.(ts|tsx)$/.test(path));
}

test('new organize route has no backend DTO copies or unsafe casts', () => {
	const source = sourceFiles('packages/interface/src/routes/organize').map((path) => readFileSync(path, 'utf8')).join('\n');
	expect(source).not.toMatch(/\bas\s+any\b/);
	expect(source).not.toMatch(/interface\s+Organize(TaskSummary|ItemView|CommitPlanOutput|SelectionInput)\b/);
});

test('retired view and active json commands are absent', () => {
	const roots = ['packages/interface/src', 'apps/tauri/src', 'apps/tauri/src-tauri/src'];
	const source = roots.flatMap(sourceFiles).map((path) => readFileSync(path, 'utf8')).join('\n');
	expect(source).not.toContain("viewMode === 'organize'");
	expect(source).not.toContain('save_organize_state');
	expect(source).not.toContain('delete_organize_state');
});
```

- [ ] **Step 2: Run RED retirement test**

Run: `bun test packages/interface/src/routes/organize/__tests__/contractHygiene.test.ts`

Expected: FAIL while old ViewMode, inspector wiring, and JSON write commands remain.

- [ ] **Step 3: Remove retired code and update copy/help**

Delete the old route directory only after ORG-PREV-02 and ORG-FE-04 are green. Remove `organize` from every view-mode union/menu/switch/fallback, remove Shell/Inspector organize props, and retain shared Quick Preview code in its new home. Remove old active Tauri commands and platform methods. Update English/Chinese task, warning, change, migration, and preview labels. Update help content to Arrow keys and focused-video Space behavior.

- [ ] **Step 4: Regenerate TypeScript and i18n types**

Run: `cargo run --bin generate_typescript_types`

Run: `bun run --filter @sd/interface generate:i18n-types`

Expected: generated outputs change only to reflect current registered Rust DTOs and locale keys.

- [ ] **Step 5: Run GREEN static/type/drift checks**

Run: `bun test packages/interface/src/routes/organize/__tests__/contractHygiene.test.ts packages/interface/src/Settings/pages/__tests__/helpSettingsContent.test.ts`

Run: `bun run --filter @sd/interface typecheck`

Run: `bash scripts/check-ts-types.sh`

Expected: PASS; no old write command, ViewMode entry, local backend DTO, generated drift, or unsafe cast exists in the new module.

- [ ] **Step 6: Commit ORG-TS-02**

```bash
git add -A apps/tauri/src-tauri/src/organize.rs packages/interface/src/routes/explorer/organize packages/interface/src/organizeLayoutSizing.ts packages/interface/src/__tests__/organizeLayoutSizing.test.ts packages/interface/src/__tests__/organizeLayoutIntegration.test.ts packages/interface/src/routes/explorer packages/interface/src/components/TabManager/TabManagerContext.tsx packages/ts-client/src/stores/viewPreferences.ts packages/interface/src/ShellLayout.tsx packages/interface/src/components/Inspector packages/interface/src/locales packages/interface/src/Settings packages/ts-client/src/generated/types.ts apps/tauri/src apps/tauri/src-tauri/src/main.rs packages/interface/src/contexts/PlatformContext.tsx packages/interface/src/routes/organize/__tests__/contractHygiene.test.ts
git commit -m "refactor(organize): retire per-directory organize view"
```

## ORG-INT-01: Full Contract, Windows Filesystem, and Real Tauri Flow

**Depends on:** ORG-TS-02

**Files:**
- Extend: `core/tests/organize_task_flow.rs`
- Replace Organize-specific sections: `tests/webdriver/test_real_tauri_app.py`
- Modify only if factual test wiring requires it: `tests/webdriver/README.md`

**Input/output and failure contract:** The harness owns its temporary directory and records created task UUIDs. Cleanup deletes only those task metadata rows and process-owned temporary files. Drift/failure assertions require zero unexpected deletion.

- [ ] **Step 1: Add the RED vertical WebDriver scenario**

```python
def test_recursive_organize_task_flow(driver, origin):
    with RecursiveOrganizeFixture(driver) as fixture:
        fixture.create_nested_albums_images_video_move_and_delete_targets()
        fixture.open_explorer_root(origin)
        task_id = fixture.create_task_from_path_bar()
        fixture.wait_for_task_status(task_id, "active")
        fixture.open_nested_directory("Albums/2026")
        fixture.keep_child("preserve.jpg")
        fixture.assert_parent_progress(keep_units=1)
        fixture.assert_plain_and_ctrl_lasso_contract()
        fixture.keep_descendant_then_cancel_and_confirm_parent_discard("conflict-dir")
        fixture.move_item_via_native_destination("move-me.jpg", fixture.move_destination)
        fixture.reload_and_assert_decisions_progress(task_id)
        fixture.execute_and_wait_for_organize_commit(task_id)
        fixture.assert_disk_state(moved=["move-me.jpg"], deleted=["delete-me.jpg", "conflict-dir"], preserved=["preserve.jpg"])
        fixture.finish_with_unmarked_confirmation(task_id)
        fixture.assert_read_only(task_id)
        fixture.reopen(task_id)
        fixture.assert_decisions_enabled(task_id)
```

Add a second focused branch in the same test that introduces drift before execution, asserts the commit job performs no move/delete, accepts or explicitly overrides drift, and then retries.

- [ ] **Step 2: Run RED WebDriver flow**

Run: `python tests/webdriver/test_real_tauri_app.py`

Expected: FAIL until selectors/helpers follow `/organize/:taskId`, backend job status, new confirmations, and SQLite task cleanup rather than old JSON commands.

- [ ] **Step 3: Replace old harness assumptions**

Remove seeded `viewMode: organize`, FNV JSON keys, and direct load/save/delete command checks. Drive the real path-bar/context-menu entry. Use stable `data-testid` values from new components for task status, item UUID, progress, lasso surface, dialogs, and commit state. Capture only task UUIDs created by the fixture and delete those records through `organize.delete_task` during cleanup.

- [ ] **Step 4: Run all fresh automated verification**

Run: `cargo fmt --check`

Run: `cargo test -p sd-core organize`

Run: `cargo test -p sd-core --test organize_repository`

Run: `cargo test -p sd-core --test organize_task_flow`

Run: `cargo test -p sd-core --test preview_sequence`

Run: `cargo test -p sd-core --test organize_wire_contract`

Run: `cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml legacy_organize::tests`

Run: `cargo run --bin generate_typescript_types`

Run: `bash scripts/check-ts-types.sh`

Run: `bun test packages/ts-client/src/__tests__/organizeContract.test.ts`

Run: `bun test packages/interface/src/routes/organize`

Run: `bun test --preload packages/interface/src/test/setup-dom.ts packages/interface/src/routes/organize/__tests__/virtualization.test.tsx packages/interface/src/routes/organize/__tests__/taskPresentation.test.tsx packages/interface/src/components/QuickPreview/__tests__/previewSequence.test.tsx`

Run: `bun run --filter @sd/interface typecheck`

Expected: every command exits 0 with no test failure, generated drift, TypeScript error, or format diff.

- [ ] **Step 5: Run the real Windows Tauri flow**

Run: `python tests/webdriver/test_real_tauri_app.py`

Expected: PASS for recursive creation, navigation, parent progress, click/Ctrl/Shift/lasso, Discard override, Move picker, restart persistence, shared preview, preflight safety, move-before-delete disk state, partial/drift recovery, finish/read-only, and reopen.

- [ ] **Step 6: Commit ORG-INT-01**

```bash
git add core/tests/organize_task_flow.rs tests/webdriver/test_real_tauri_app.py tests/webdriver/README.md
git commit -m "test(organize): verify recursive task workflow"
```

## Plan Self-Review

### Spec coverage

- PASS: Fixed recursive snapshot, metadata-only traversal, Windows current-device identity, path normalization, overlap, volume-root guard, reparse leaves, unreadable issue units, and fixed denominator map to ORG-BE-01 through ORG-BE-03.
- PASS: Keep/Discard/Move/Clear, Unmarked, exact progress categories, same-decision compression, mixed descendant confirmation, ancestor split, DirectChildren normalization, applied immutability, and failed-operation retry map to ORG-BE-01, ORG-BE-02, ORG-BE-04, and ORG-FE-03.
- PASS: Change scan, pending additions outside the denominator, missing/changed acceptance, destructive inherited-addition confirmation, finish/reopen/delete record, scan retry, and completed read-only behavior map to ORG-BE-04 and ORG-FE-04.
- PASS: Compact move/delete roots, unsafe topology, global no-side-effect preflight, current-subtree drift override, move-before-delete, confirmed permanent delete, child-job waits, partial settlement, cancellation, checkpoint, restart reconciliation, and task return to active map to ORG-BE-05 and ORG-FE-04.
- PASS: Paged direct children, stable cursor tie-breakers, list/grid virtualization, fewer than 300 cards for 10,000 items, rendered-only thumbnails, path/size/last-write cache identity, transient UUID selection, plain/Ctrl/Shift/Ctrl+A/lasso and edge scroll map to ORG-BE-02, ORG-FE-02, and ORG-FE-03.
- PASS: Bounded live/manifest preview, branch sampling, candidate budgets, paused videos, one-sample renderer, keyboard sequence, persisted VideoPlayer preferences, and bounded no-media fallback map to ORG-PREV-01 and ORG-PREV-02.
- PASS: Task routes, sidebar ordering/count, path-bar/context-menu create/open, tab-state restore, Move destinations, exact confirmation focus behavior, generated DTO use, stale refetch, legacy import/archive, old ViewMode/JSON retirement, localization/help, and real Tauri flow map to ORG-FE-01 through ORG-FE-04, ORG-TS-01/02, and ORG-INT-01.
- PASS: No non-Windows scanning/execution, sync model, third persistence table, workflow framework, cloud continuation, whole-file hash, automatic completion, recycle-bin delete, or unrelated refactor is planned.

### Placeholder scan

- PASS: The plan contains no deferred-work markers, undefined follow-up references, copy-by-reference instructions, or requests for unspecified tests/error handling.
- PASS: Every task has exact files, dependencies, contract/failure semantics, a RED command with expected failure, concrete signatures or test code, a GREEN command with expected success, and an explicit commit command.

### Type consistency

- PASS: `OrganizeTaskStatus`, `OrganizeItemKind`, `OrganizeDecisionInput`, `OrganizeSelectionInput`, `OrganizeItemFilter`, `OrganizeItemSort`, `SortDirection`, `OrganizeProgressSummary`, `OrganizeTaskSummary`, `OrganizeItemView`, decision/change/commit/lifecycle outcomes, and preview DTO names are defined once in Rust and reused unchanged in later Rust and TypeScript steps.
- PASS: Wire names are fixed as `organize.list`, `organize.get`, `organize.children`, `organize.resolve_root`, `organize.commit_plan`, `files.preview_sequence`, `organize.create`, `organize.set_decision`, `organize.scan_changes`, `organize.accept_changes`, `organize.commit`, `organize.retry_snapshot`, `organize.finish`, `organize.reopen`, and `organize.delete_task`.
- PASS: `FileConflictResolution` is reused from the existing generated file-copy contract. `JobReceipt`, `JobId`, `SdPath`, `File`, `Location`, and existing space item types are imported from existing/generated sources.
- PASS: Revision is signed `i64` in Rust and generated as TypeScript `number`; counts/bytes are `u64` and generated under the repository's configured Specta number behavior.
- PASS: The task item UUID, not `File.id` from live index enrichment or DOM position, remains the selection, focus, cursor tie-breaker, and operation-settlement identity.

### Repository mapping notes for the execution coordinator

- The current job context cannot spawn child jobs directly, so ORG-BE-05 intentionally uses `ctx.library().jobs().dispatch`, `JobManager::get_job`/`get_job_info`, and `JobHandle::wait`.
- The current old Organize implementation has reusable thumbnail and preview ideas but also unsafe casts and unbounded queries. Move only the cache behavior and shared renderer responsibility, then delete the old directory after replacement tests pass.
- `packages/interface` currently has no DOM test runtime. ORG-FE-02 adds narrowly scoped test-only dependencies and uses an explicit Bun preload rather than changing every repository test.
- The generated type file is `packages/ts-client/src/generated/types.ts`, which matches the current generator and root guidance despite older documentation mentioning `generated.ts`.
- The exact Space layout generated output shape must be confirmed while implementing ORG-FE-03. The plan requires using its generated type or existing typed hook, never reproducing it or casting it.
- SQLite partial-index syntax and SeaORM's generated index names must be checked against the repository's pinned SeaORM version in ORG-BE-02. The semantic constraints and asserted stable explicit names must not change.
