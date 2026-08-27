# Recursive Organize Tasks Execution Design

> **Date:** 2026-08-27
>
> **Status:** Approved direction, pending written-spec review
>
> **Primary platform:** Windows desktop/Tauri
>
> **Supersedes:** The scope, persistence, entry-point, and preview decisions in `docs/superpowers/specs/2026-06-05-organize-view-design.md`

## 1. Authority and purpose

This document is the execution contract for replacing the current per-directory Organize View with recursive Organize Tasks. When this document conflicts with the 2026-06-05 Organize View design, this document wins. Existing implementation that still matches this contract should be reused.

The design is complete when an implementation agent can answer the following questions from this document without redefining product behavior:

- Where does the user start and resume an organize task?
- Which files belong to a task, and when can that membership change?
- What do Keep, Discard, Move, and Unmarked mean for files and directories?
- How is directory progress calculated?
- How do parent and descendant decisions interact?
- What is persisted, and which component owns it?
- How are move and permanent-delete operations planned and settled?
- What happens when the filesystem changes after the snapshot?
- Which automatic tests prove each important rule?
- What exact conditions define Done?

The implementation plan may adjust file placement when repository facts require it, but it must not change the business semantics, wire contracts, safety conditions, or Done criteria without first revising this specification.

## 2. Goal

Build a Windows-first recursive organize workflow in which a user:

1. Creates a task for one physical directory.
2. Receives a fixed recursive snapshot of everything below that root.
3. Navigates the snapshot at any depth without rescanning task state per directory.
4. Previews files and representative media from directories quickly.
5. Marks files or whole subtrees as Keep, Discard, or Move.
6. Sees recursive progress on every directory.
7. Reviews one compact execution plan.
8. Permanently deletes Discard roots and moves Move roots through the existing file-operation jobs.
9. Keeps failed operations in the task for correction or retry.
10. Explicitly decides when the task is finished.

Keep is a reviewed marker. It has no filesystem side effect. Discard means permanent deletion and never uses the recycle bin. Move means move the selected file or complete subtree to one selected destination.

## 3. Non-goals

The first implementation does not include:

- Functional scanning or execution support outside Windows. Non-Windows builds return an explicit unsupported-platform result.
- Cloud sync, peer sync, or cross-device continuation of organize tasks.
- Multi-user task ownership or collaboration.
- A general workflow engine, event-sourcing system, plugin API, or reusable task framework.
- Automatic content hashing of the entire snapshot.
- Duplicate detection redesign. Existing size-first duplicate candidate grouping remains unchanged.
- Recycle-bin deletion, secure overwrite, or restoration after permanent deletion.
- Automatic ingestion of new files after task creation.
- Automatic completion when progress reaches 100 percent.
- Following Windows junctions, directory symlinks, or other reparse points.
- Search, semantic clustering, or AI-assisted decisions inside organize tasks.
- A complete audit history or multi-step undo system. Before execution, users correct mistakes through Clear and new decisions.

## 4. Existing repository facts and reuse decisions

The implementation must preserve and reuse these existing capabilities:

- `core/src/ops/files/delete` already dispatches confirmed permanent recursive deletion through `DeleteJob`.
- `core/src/ops/files/copy` already supports moving sources with `move_files: true` and existing conflict policies.
- `core/src/infra/job` already persists jobs, reports progress, exposes `JobHandle::wait`, and allows a job to dispatch another job through the library job manager.
- `core/src/infra/db/entities/entry_closure.rs` can accelerate indexed-path lookups, but it is not the task manifest because the task snapshot must stay stable while the live index changes.
- `packages/interface` already depends on `@tanstack/react-virtual` and uses it in Grid, List, Media, Masonry, and other large collections.
- `packages/interface/src/components/QuickPreview` already renders images and videos and persists video volume/mute preferences.
- `PlatformContext.openDirectoryPickerDialog` already provides an arbitrary local destination picker.
- Locations, Volumes, and pinned Paths already expose reusable move destinations.

The following current decisions are retired:

- Organize is no longer an Explorer `ViewMode`.
- A task is not stored as one JSON file per visited directory.
- Decisions are not limited to direct children of whichever directory is currently open.
- Directory preview does not request up to 10,000 recursive media records.
- Decision persistence is not delayed until every fifth interaction.
- The custom Organize lasso implementation is not retained.

The old per-directory write adapter is retired after the migration path works. A read/archive-only legacy import boundary remains for this version so an installation with old files can still migrate after upgrading.

## 5. End-to-end business flows

### 5.1 Create a task

1. The user opens a physical Windows directory in Explorer or selects a physical directory card.
2. The user chooses **Organize this folder** from the path-bar action or directory context menu.
3. The frontend calls `action:organize.create.input` with the directory `SdPath` and an optional display name.
4. Core canonicalizes the Windows path, verifies that it exists and is a directory, rejects an unsupported or overlapping active task, creates an `organize_tasks` row in `scanning`, and dispatches `OrganizeSnapshotJob`.
5. The UI navigates immediately to `/organize/<task-id>` and shows scan progress.
6. The snapshot job performs metadata-only traversal. It does not hash content, decode media, or generate thumbnails.
7. The job persists the root and descendants in `organize_task_items`, computes stable tree intervals and unit counts, then transitions the task to `active`.
8. A root-level scan failure moves the task to `failed`. Unreadable descendants become visible issue items and leave the task usable.

The default task name is the root folder name. If the root is a volume root, use the volume display name followed by `Organize`.

### 5.2 Resume and navigate

1. Active, scanning, committing, and failed tasks appear in a dedicated **Organize Tasks** sidebar group.
2. `/organize` lists all task records for the current library. `/organize/<task-id>` opens one task.
3. The task route queries only the current snapshot directory's direct children, in pages of at most 200 records.
4. Double-clicking a directory changes the current task item and breadcrumb. It does not create or load another persistence file.
5. Returning to a task restores its current directory, layout, filter, and scroll position through existing tab state. Selection is transient and clears on directory navigation.

### 5.3 Review and decide

1. The user selects one or more visible items.
2. Keep, Discard, Move, or Clear applies to the complete selection, not only the focused card.
3. A decision on a file covers one decision unit.
4. A decision on a directory covers its complete snapshotted subtree.
5. Core validates parent/descendant conflicts in one transaction and either applies the full batch or returns a confirmation requirement without changing data.
6. Successful mutation increments the task revision and returns the updated directory and task summaries.

### 5.4 Preview

1. Selecting a file shows the existing native Quick Preview renderer.
2. Selecting a directory shows a bounded representative contact sheet generated by the shared preview-sequence service.
3. The user can move through representative images and videos with the keyboard and open any one in the existing full Quick Preview.
4. Video does not autoplay merely because a directory or card becomes selected.

### 5.5 Move

1. The user chooses Move for the selection.
2. The destination picker shows recent destinations, current-library Locations, pinned physical Paths, and **Choose another folder**.
3. Choosing another folder uses `openDirectoryPickerDialog`.
4. The selected destination is stored with the decision. No move occurs until the user executes the task plan.
5. The UI displays the destination on moved-marked items and in the Move filter.

### 5.6 Execute pending operations

1. The user chooses **Execute organize actions**.
2. The frontend queries `organize.commit_plan` using the current task revision.
3. The review dialog shows compact move groups, compact permanent-delete roots, estimated bytes affected, unmarked units, failed items, and detected filesystem drift.
4. The user explicitly confirms permanent deletion and chooses a move conflict policy. The default policy is `AutoModifyName`.
5. `organize.commit` atomically verifies the revision, locks decision mutation by moving the task to `committing`, and dispatches `OrganizeCommitJob`.
6. The job preflights every physical operation root before producing any filesystem side effect.
7. If preflight detects unaccepted additions, changed targets, or unsafe paths, the job returns the task to `active` and performs no move or delete.
8. If preflight passes, all move groups execute first. Permanent deletes execute second.
9. Successful targets become applied and immutable. Failed targets remain decided with `failed` operation state and a user-visible error.
10. The task returns to `active`. Execution does not automatically finish the task.

### 5.7 Finish or reopen

1. **Finish task** is available only when no operation is queued, running, or failed.
2. If unmarked units remain, finishing requires a second confirmation that reports their count.
3. Finishing changes the task to `completed`; it does not perform filesystem actions.
4. A completed task is read-only and remains available in the task list.
5. **Reopen task** changes it back to `active`. Applied operations remain immutable.
6. **Delete task record** deletes only organize metadata. It is disabled while committing and never deletes user files.

## 6. Decision and progress semantics

### 6.1 Decision units

Progress must not double-count a non-empty directory and all of its children. The snapshot therefore assigns units as follows:

- A file is one unit.
- An empty directory is one unit.
- A reparse point is one unit and is never traversed.
- An unreadable entry or unreadable directory is one issue unit.
- A non-empty readable directory has a unit count equal to the sum of its children's unit counts.

The task root's `unit_count` is the stable denominator for task progress. A directory's `unit_count` is the denominator for its progress bar.

### 6.2 Sparse subtree decisions

Only explicit decision roots are stored. A directory decision applies to every unit in its interval. Descendants inherit the closest explicit ancestor decision.

The core invariant is:

> No included task item with an explicit decision may have another explicit decision ancestor.

This prevents contradictory execution plans and makes progress additive without double-counting.

For a directory with no explicit decision on itself or an ancestor:

```text
processed_units = sum(unit_count of explicit decision roots in the interval)
unmarked_units  = total_units - processed_units
progress        = processed_units / total_units
```

If the directory itself or its closest explicit ancestor has a decision, `processed_units = total_units` and the complete total belongs to that effective decision category.

Keep, Discard, and Move counts use the same sum grouped by decision kind. A failed Discard or Move remains processed because the user made a decision; execution failure is shown separately.

### 6.3 Parent and descendant behavior

The same rule applies to Keep, Discard, and Move. For Move, “same decision” also requires the same normalized destination.

| Existing state | Requested state | Required behavior |
| --- | --- | --- |
| No explicit ancestor or descendant | Any decision | Apply directly. |
| Descendant decisions are all the requested decision | Same decision on directory | Silently remove redundant descendant roots and store one directory decision. |
| Any descendant has a different decision | Decision on directory | Return a confirmation summary. On confirmation, clear descendant decisions and store the directory decision. |
| Closest explicit ancestor already has the same decision | Same decision on descendant | No-op and report that the item inherits the decision. |
| Closest explicit ancestor has a different decision | Different decision on descendant | Return an ancestor-conflict confirmation. On confirmation, clear the ancestor decision, leave the rest of that former subtree unmarked, and apply the requested child decision. |
| Applied operation covers the item | Any mutation | Reject as immutable. |

For the user's primary Discard case:

- A directory whose decided descendants are all Discard collapses immediately into one Discard root.
- A directory with any Keep or Move descendant requires confirmation that reports Keep units, Move units, and affected bytes.
- Confirming means “overwrite all existing organization under this directory and permanently discard the complete current snapshot subtree.”
- Unmarked descendants are not a conflict. They become Discard through inheritance.

Clear removes an explicit decision root and leaves its former subtree unmarked. Clearing an inherited item requires ancestor-split confirmation; on confirmation Core clears the covering ancestor decision and leaves that former subtree unmarked. There is no hidden restoration of descendant decisions that an earlier confirmed override removed.

### 6.4 Batch selection normalization

Before conflict evaluation, Core sorts selected items by tree order and drops selected descendants whose selected ancestor is already in the batch. The returned affected counts reflect the normalized roots.

This normalization is mandatory even though the normal directory view presents direct children, because filtered views and future restored selections can contain nested items.

## 7. Task and item lifecycle

### 7.1 Task states

| State | Allowed transitions | User-visible meaning |
| --- | --- | --- |
| `scanning` | `active`, `failed` | The recursive snapshot is being created. Decisions and commit are disabled. |
| `active` | `committing`, `completed` | The user may navigate, decide, scan for changes, and execute. A change-scan failure leaves the task active and stores the error. |
| `committing` | `active` | Decision mutation and finishing are locked while the commit job preflights and executes. |
| `completed` | `active` | Read-only finished record. Reopen is allowed. |
| `failed` | `scanning` or task deletion | Root scan failed. The task exposes retry and the stored error. |

A partial commit failure does not put the whole task in `failed`; it returns to `active` with failed item operations.

### 7.2 Item operation states

| State | Meaning |
| --- | --- |
| `none` | Unmarked or Keep. No physical action is pending. |
| `pending` | Discard or Move is decided but not running. |
| `running` | The current commit job is operating on this explicit decision root. |
| `applied` | The physical action settled successfully. The decision is immutable. |
| `failed` | The action did not settle. The decision remains and may be changed or retried. |

Only explicit decision roots hold an operation state. Descendants derive the effective state from the closest explicit ancestor.

## 8. Persistence model

Organize Tasks are local records in the current library SQLite database. They do not implement `Syncable` and are not included in sync backfill.

### 8.1 `organize_tasks`

Required fields:

| Field | Contract |
| --- | --- |
| `id` | UUID primary identity exposed over the wire. |
| `name` | Non-empty display name. Defaults from the root directory. |
| `root_path` | Canonical display-form Windows path without removable `\\?\` drive/UNC prefix. |
| `root_path_key` | Case-insensitive normalized Windows path key used for overlap checks. |
| `device_slug` | Current physical-device slug used to reconstruct `SdPath`. |
| `volume_id` | Optional local Volume relation when resolution succeeds. |
| `root_entry_uuid` | Optional indexed Entry identity. It is an accelerator, not membership authority. |
| `status` | Task state from section 7.1. |
| `revision` | Monotonic integer incremented by decisions, detected or accepted scope changes, operation settlement, and finish/reopen lifecycle changes. |
| `snapshot_version` | Starts at `1`; identifies manifest schema, not application release. |
| `total_entries` | Included manifest rows including the root. |
| `total_units` | Root decision-unit denominator. |
| `total_bytes` | Sum of included file sizes. |
| `scan_issue_count` | Unreadable or metadata-failed snapshot entries. |
| `pending_addition_count` | Discovered additions not yet included in the fixed snapshot. |
| `scan_job_id` | Current snapshot/change-scan job ID or null. |
| `commit_job_id` | Current commit job ID or null. |
| `last_error` | Last task-level failure message or null. |
| timestamps | `created_at`, `updated_at`, and nullable `completed_at`. |

At most one non-completed task may overlap another non-completed task root. “Overlap” means equal roots, one root is an ancestor of the other, or vice versa under normalized Windows path comparison. Creating an overlapping task returns the existing task identity so the UI can open it.

### 8.2 `organize_task_items`

Required fields:

| Field | Contract |
| --- | --- |
| `id` | Local integer primary key used by parent joins. |
| `uuid` | Stable UUID exposed to the frontend. |
| `task_id` | Foreign key to `organize_tasks`, cascading on task-record deletion. |
| `parent_id` | Nullable self-reference; null only for the task root. |
| `entry_uuid` | Optional indexed Entry UUID for metadata enrichment. |
| `relative_path` | Display-form path relative to the task root; root uses an empty string. |
| `relative_path_key` | Case-insensitive normalized relative key, unique per task. |
| `name`, `extension` | Snapshot display metadata. Extension is lowercase without a leading dot. |
| `kind` | `file`, `directory`, `reparse_point`, or `unreadable`. |
| `size_bytes` | File size; zero for containers and unresolved entries. |
| `aggregate_size_bytes` | Recursive file-byte sum for the row's subtree. |
| `modified_at_100ns` | Windows last-write time at snapshot precision when available. |
| `metadata_signature` | BLAKE3-128 of kind, size, last-write time, extension, and normalized relative path. It is a change token, not a content identity. |
| `tree_start`, `tree_end` | Inclusive depth-first interval. A is an ancestor of B when B's start lies inside A's interval. |
| `unit_count` | Decision units covered by this row. |
| `membership_state` | `included` or `pending_addition`. Pending additions do not affect the denominator. |
| `external_state` | `present`, `changed`, `missing`, or `unreadable`. |
| `decision_kind` | Null, `keep`, `discard`, or `move`. |
| `move_destination` | Serialized `SdPath`; non-null only for Move. |
| `operation_state` | State from section 7.2. |
| `last_error` | Item-operation or scan error text. |
| `applied_at` | Nullable successful-operation timestamp. |
| timestamps | `created_at` and `updated_at`. |

Required indexes and constraints:

- Unique `(task_id, uuid)`.
- Unique `(task_id, relative_path_key)`.
- Unique `(task_id, tree_start)` for included items.
- Index `(task_id, parent_id, name)` for child paging.
- Index `(task_id, decision_kind, tree_start)` for progress and planning.
- Index `(task_id, membership_state, external_state)` for change review.
- `move_destination` is non-null if and only if `decision_kind = move`.
- Null and Keep decisions use `operation_state = none`; only Discard and Move may use pending, running, applied, or failed.
- Applied rows cannot be updated by the decision repository.
- Decision transactions must preserve the no-nested-explicit-decisions invariant.

No third persistence table is introduced for history, cached aggregates, or events.

## 9. Snapshot, path, and change-detection algorithms

### 9.1 Windows path rules

Creation accepts only an `SdPath::Physical` that resolves to the current Windows device and an existing directory.

Canonicalization must:

- Resolve `.` and `..`.
- Normalize separators to `\` for keys.
- Remove `\\?\C:\` and `\\?\UNC\` display prefixes using the repository's existing prefix helper where valid.
- Trim trailing separators except on a volume root.
- Compare keys case-insensitively.
- Preserve the original case in display paths.

A task may target a volume root, but Discard or Move on the root item itself is forbidden when the root is a mounted volume root. Its children remain actionable.

### 9.2 Metadata-only snapshot

`OrganizeSnapshotJob` runs blocking Windows filesystem enumeration inside `spawn_blocking` and uses an explicit stack rather than recursive Rust calls.

Traversal rules:

- Include normal, hidden, and system files because the task promises the complete directory contents.
- Use `symlink_metadata` semantics.
- Record a junction, directory symlink, or other reparse point as one leaf item and never follow it.
- Insert an unreadable directory or entry as one issue item instead of silently omitting it.
- Never decode media, create thumbnails, or calculate a whole-file hash.
- Persist rows in batches so progress is visible and memory does not scale with the complete tree.
- On directory exit, calculate `tree_end`, `unit_count`, and `aggregate_size_bytes` bottom-up.
- On root-level failure, remove partial included rows before marking the task failed.

### 9.3 Cheap metadata signature

The user's size-plus-extension observation is used as a prefilter and change token, not as proof that two media files are identical.

For organize tasks:

- `kind + size + modified_at_100ns + extension + normalized relative path` forms the metadata signature.
- Matching signatures avoid content hashing during ordinary reopen and commit preflight.
- A mismatching signature marks the item changed and requires user review or explicit current-subtree override.
- No whole-file hash is calculated merely to delete or move a path the user explicitly chose.

For adjacent existing systems:

- The Organize thumbnail cache key must add last-write time to its current path-and-size key so same-size replacement does not reuse stale media.
- Existing duplicate detection continues grouping by size before expensive content hashing. This feature does not alter duplicate correctness rules.

Residual risk is explicit: a replacement with the same path, type, size, timestamp, and extension is not detected. This is acceptable for this personal utility and is safer than treating size and extension as a content identity in database relationships.

### 9.4 Manual scope refresh

`OrganizeChangeScanJob` compares the live root against the included manifest using normalized path keys and metadata signatures.

- New paths are inserted as `pending_addition` and do not change progress.
- Missing included paths are marked `missing` but remain in the denominator until accepted.
- Signature mismatches are marked `changed`.
- Applied Discard and Move intervals are historical and are excluded from live missing/change detection.
- The task page reports all three categories.

Only one change scan may run for a task. Decisions remain available while it runs, but commit and Finish are disabled until the scan settles. Persisting scan results increments the task revision so a commit plan prepared before those results becomes stale.

`organize.accept_changes` applies an explicit user choice:

- Include selected additions.
- Remove selected missing entries from the manifest.
- Refresh selected changed metadata and clear their unapplied decisions by default.
- Preserve a changed item's decision only when the user explicitly confirms that choice.
- When an accepted addition lies under an explicit ancestor decision, report the inherited decision before mutation and require confirmation if that decision is Discard or Move.

Accepting changes rebuilds tree intervals and unit counts in one transaction and increments the task revision.

## 10. Backend responsibility and wire contracts

All public Rust input/output types derive `specta::Type`. After backend type changes, the implementation regenerates and commits `packages/ts-client/src/generated/types.ts`. The frontend must use generated types and may not recreate DTOs or cast responses to `any`.

### 10.1 Core module boundaries

Create `core/src/ops/organize/` with these responsibilities:

- `model.rs`: wire enums and task/item summaries.
- `path.rs`: Windows canonicalization, normalized keys, overlap, ancestry, and destination safety.
- `tree.rs`: pure tree intervals, unit counts, decision conflict resolution, progress reduction, and operation compaction.
- `repository.rs`: all SQLite reads and transactions for the two organize tables.
- `snapshot/`: snapshot and change-scan jobs; Windows implementation plus explicit unsupported-platform stub.
- `create/`: task creation action.
- `query/`: task list, task detail, paged children, and commit-plan queries.
- `decision/`: set/clear decision action and confirmation outcomes.
- `commit/`: commit action, preflight, and `OrganizeCommitJob`.
- `lifecycle/`: retry scan, accept changes, finish, reopen, and delete-task-record actions.

Business rules live in `tree.rs` and repository transactions, not React hooks.

### 10.2 Public decision and selection types

The wire shape is a tagged enum equivalent to:

```rust
enum OrganizeDecisionInput {
    Keep,
    Discard,
    Move { destination: SdPath },
}

enum OrganizeSelectionInput {
    Items { item_ids: Vec<Uuid> },
    DirectChildren {
        parent_item_id: Uuid,
        filter: OrganizeItemFilter,
        excluded_item_ids: Vec<Uuid>,
    },
}
```

`DirectChildren` supports Ctrl+A without loading thousands of UUIDs. The backend resolves it against `expected_revision`, so a concurrent decision change produces a stale-revision result instead of silently changing the selection scope.

### 10.3 Required queries

#### `organize.list`

Input:

- Optional status filter.
- Cursor.
- Limit, clamped to 100.

Output:

- Task identity, name, root path, status, progress summary, issue counts, active job IDs, and next cursor.

#### `organize.get`

Input: task UUID.

Output:

- Complete task header data.
- Root item UUID.
- Current revision.
- Overall Keep, Discard, Move, Unmarked, failed-operation, changed, missing, and pending-addition counts.

#### `organize.children`

Input:

- Task UUID and parent item UUID.
- Cursor.
- Limit, clamped to 200.
- Sort: name, modified time, size, or progress.
- Direction.
- Filter: all, unmarked, keep, discard, move, failed, changed, or missing.

Output:

- `OrganizeItemView[]`, each containing a generated `File` representation plus explicit/effective decision, decision source, progress summary, move destination, external state, and operation state.
- Parent breadcrumb data.
- Next cursor.

Ordering must be stable by appending item UUID as a tie-breaker.

#### `organize.commit_plan`

Input: task UUID and expected revision.

Output:

- The same revision.
- Move groups by normalized destination.
- Compact Discard roots.
- Units and bytes per group.
- Keep and Unmarked counts.
- Pending additions, changed/missing targets, failed operations, and unsafe destination conflicts.
- `can_commit` boolean and blocking reasons.

The query has no filesystem side effects.

#### `files.preview_sequence`

Input:

- Physical directory `SdPath`.
- Optional organize task/item identity to reuse the fixed manifest.
- Maximum result count, clamped to 12.

Output:

- Representative generated `File` values.
- Image/video counts observed in the candidate budget.
- Whether the candidate budget was exhausted.

This query is a file-manager capability and is not owned by the task UI.

### 10.4 Required actions

#### `organize.create`

Input: root `SdPath` and optional name.

Output: task UUID, status, and snapshot `JobReceipt`.

Errors: unsupported platform/path kind, root missing, root not directory, permission failure, or overlapping active task. Overlap includes the existing task UUID.

#### `organize.set_decision`

Input:

- Task UUID.
- `OrganizeSelectionInput`.
- Decision or null for Clear.
- Expected revision.
- `confirm_descendant_override`.
- `confirm_ancestor_split`.

Output is one of:

- `Applied { revision, task_summary, affected_roots }`.
- `ConfirmationRequired { conflict_kind, keep_units, discard_units, move_units, unmarked_units, affected_bytes, conflicting_roots }`.
- `StaleRevision { current_revision }`.
- `InheritedNoOp { revision, ancestor_item_id }`.

A confirmation-required result performs no mutation.

#### `organize.scan_changes`

Input: task UUID and expected revision.

Output: change-scan `JobReceipt`. It is allowed only in `active`.

#### `organize.accept_changes`

Input: task UUID, expected revision, selected pending/missing/changed item UUIDs, and whether changed decisions should be preserved.

Output: new revision and rebuilt task summary, or the same no-mutation confirmation shape when an addition would inherit Discard or Move.

#### `organize.commit`

Input:

- Task UUID and expected revision.
- `permanent_delete_confirmed: true`.
- Move conflict policy: `AutoModifyName`, `Skip`, `Overwrite`, or `Abort`.
- `allow_current_subtree_drift`, default false.

Output: commit `JobReceipt`.

The action rejects false permanent confirmation, stale revision, non-active state, a blocked plan, or unsafe move topology.

#### Lifecycle actions

- `organize.retry_snapshot`: failed to scanning, returns a job receipt.
- `organize.finish`: accepts expected revision, changes active to completed, and returns confirmation requirement when unmarked units remain and the input has not confirmed them.
- `organize.reopen`: accepts expected revision and changes completed to active.
- `organize.delete_task`: accepts expected revision, deletes metadata only, and rejects committing tasks.

## 11. Operation planning, preflight, and settlement

### 11.1 Defensive compaction

The planner sorts explicit decision roots by `tree_start`. Even though the decision repository enforces non-overlap, the planner defensively drops any root covered by a previously accepted ancestor of the same physical action.

The resulting plan contains:

- No Keep targets.
- Move roots grouped by identical normalized destination and conflict policy.
- Discard roots with no descendant duplicate.

### 11.2 Destination validation

A Move destination must:

- Resolve to an existing local Windows directory.
- Not equal the source.
- Not lie inside the source subtree.
- Not lie inside a Discard subtree in the same plan.
- Not create a move cycle between planned source directories.

Unsafe topology blocks commit before a job is dispatched.

### 11.3 Preflight

`OrganizeCommitJob` completes preflight for all operation roots before starting the first move.

- File roots compare existence, kind, and metadata signature.
- Directory roots perform metadata-only membership comparison against their included snapshot interval.
- New descendants, changed included items, unreadable paths, and path replacement block execution when `allow_current_subtree_drift` is false.
- Missing explicit targets are settled as externally missing only after user acceptance; they are not silently considered successfully deleted or moved.

If any root fails preflight, the job performs no filesystem mutation, updates change states, returns the task to active, and reports the blocking paths.

When `allow_current_subtree_drift` is true, the final confirmation must explicitly state that current unreviewed descendants inside a directory Discard or Move will be included by the physical operation.

### 11.4 Execution order

1. Mark planned physical roots running.
2. Dispatch one existing move job per destination group with `move_files: true`.
3. Wait for each move job and reconcile every source by checking whether the source still exists.
4. Continue independent move groups even when one group fails.
5. Dispatch the existing confirmed permanent `DeleteJob` for remaining compact Discard roots.
6. Wait and reconcile every delete root by checking source existence.
7. Mark absent successful roots applied. Mark still-present or errored roots failed with a message.
8. Return the task to active and invalidate task/list queries.

Move runs before delete. Keep never dispatches a job.

The parent organize job is resumable and checkpoints between operation groups. Cancellation stops launching later groups but cannot pretend an already-dispatched child job was canceled. On resume, reconciliation occurs before dispatching another group.

## 12. User interface contract

### 12.1 Entry and routing

- Add `/organize` and `/organize/:taskId` under `ShellLayout`.
- Add **Organize this folder** to the current-directory path bar and directory context menu.
- When the path overlaps an active task, replace create with **Open organize task**.
- Add an **Organize Tasks** sidebar group below the space switcher and before user-defined Space groups. Show at most five non-completed tasks and an **All tasks** row.
- Remove Organize from `ViewModeMenu`, `ViewMode`, `ExplorerPaneBody`, search/recents fallbacks, and the old view-mode shortcut.

### 12.2 Task page layout

The task page keeps the useful three-area concept but changes each area's responsibility:

- Header: task name, root path, task status, overall segmented progress, scan-change action, execute action, and finish action.
- Left rail: current-task filters for All, Unmarked, Keep, Discard, Move, Failed, and Changes. It is not a separate source of truth.
- Center: current snapshot directory, breadcrumb, list/grid switch, virtualized direct children, selection actions, and load state.
- Right: shared native preview/inspector for the focused item.

A directory card shows:

- A segmented Keep/Discard/Move progress bar.
- Processed/total units.
- Failed or filesystem-change badge when present.
- An explicit or inherited decision badge when the whole subtree is covered.

### 12.3 Selection contract

Selection behavior matches Windows file-manager expectations:

- Plain click replaces the selection and focuses that item.
- Ctrl-click toggles one item without clearing the rest.
- Shift-click selects the range from the anchor in current stable sort order.
- Plain lasso replaces the previous selection.
- Ctrl-lasso unions the lasso result with the selection captured at pointer-down.
- Moving the lasso backward removes items that are no longer intersected; it does not permanently toggle every item ever crossed.
- Clicking blank space without Ctrl clears selection.
- Ctrl+A selects all direct children matching the current filter through `DirectChildren` selection scope.
- Keep, Discard, Move, and Clear apply to the complete selection scope.

Lasso operates on rendered virtual items and supports edge auto-scroll. As scrolling mounts new rows, intersections are recomputed from the pointer rectangle and current DOM geometry.

### 12.4 Virtualization contract

- Center list and grid always use `@tanstack/react-virtual`.
- Backend pages contain at most 200 children.
- Overscan defaults to two rows and may be tuned from measured evidence.
- A 10,000-child fixture must not render more than 300 item-card elements at a normal desktop viewport.
- Selection, focus, and decisions are keyed by task item UUID, never DOM position.
- Thumbnail loading is limited to rendered items plus overscan.

### 12.5 Move picker

The Move picker presents, in order:

1. Up to five destinations recently used by this organize task.
2. Current-library Locations.
3. Pinned physical Path space items.
4. **Choose another folder** using the native directory picker.

Selecting a destination records the decision and closes the picker. It does not open the generic file-operation modal and does not move immediately.

### 12.6 Confirmation dialogs

Directory override confirmation reports the exact conflicting decision units and affected bytes. Permanent execution confirmation reports compact roots, move groups, estimated deleted bytes, unmarked count, and the recycle-bin warning.

Enter confirms only when the destructive confirm button has focus. Escape and outside click cancel without mutation.

## 13. Shared preview sequence

The old directory preview loads an unbounded direct listing, while the old Organize preview requests large recursive media lists. Replace both with a shared bounded sequence.

### 13.1 Candidate budget

For a physical directory without a task manifest:

- Breadth-first inspect at most 128 directories and 4,096 entries.
- Collect at most 256 image/video candidates.
- Never follow reparse points.

For an organize item, query candidates from its fixed task interval instead of walking the live filesystem.

### 13.2 Representative selection

Return at most 12 media items using this deterministic algorithm:

1. Group candidates by the first descendant branch beneath the selected directory. Direct media uses its own group.
2. Sort each group by captured time when available, otherwise modified time descending, then normalized path.
3. Take one candidate from each group in round-robin order.
4. Take a second candidate from each group in the same order.
5. Fill remaining slots from the globally sorted candidates not yet used.
6. Cap videos at three when images exist, but include at least one video when any video exists.
7. If no images exist, videos may fill all slots. If no videos exist, images fill all slots.

This favors breadth across sibling albums while still giving useful samples for a directory whose media is all in one branch.

### 13.3 Presentation

- A file opens its exact renderer.
- A directory with multiple samples defaults to a contact sheet.
- A directory with one sample shows that sample directly.
- Video tiles show a poster and duration and remain paused.
- Opening a video uses the existing VideoPlayer and persisted volume/mute settings.
- Arrow keys move through the sample sequence; Space controls playback only when the video renderer has focus.
- If no media is found, show a virtualized bounded direct-child list and the scan-budget status.

## 14. Legacy JSON migration

Existing `organize/v1/*.json` decisions are user data and must not be silently discarded.

Migration flow:

1. Add a temporary Tauri command that lists and parses legacy state files without mutating them.
2. `/organize` shows one migration banner when valid legacy records exist.
3. Import creates one recursive task per legacy `directoryPath` unless it overlaps a task already created from another legacy root.
4. After each snapshot becomes active, map legacy records by normalized physical path and import Keep/Discard decisions through the normal decision repository.
5. Records whose paths no longer exist are reported as skipped.
6. Only after the task and all mappable decisions commit successfully is the old JSON renamed with a `.migrated` suffix.
7. Remove the active load/save/delete frontend adapter and legacy save command after migration and WebDriver tests pass. Retain only commands needed to list, read, and archive legacy files for this version.

Legacy import does not attempt to infer Move decisions or reconstruct historical progress.

## 15. Error and recovery contract

| Failure | Required result |
| --- | --- |
| Root is missing or not a directory | Creation fails; no task row remains. |
| Platform/path is unsupported | Typed unsupported result; UI hides creation on non-Windows but renders existing task metadata read-only. |
| Root scan fails | Task becomes failed, partial included rows are removed, Retry is available. |
| Descendant cannot be read | Store one issue item, continue scan, surface issue count. |
| Decision DB transaction fails | Roll back everything, preserve prior UI data, show retryable error. |
| Decision revision is stale | Make no mutation, refetch task and current children. |
| Parent/descendant conflict | Make no mutation until explicit confirmation. |
| Move destination is unsafe | Block commit and identify source/destination conflict. |
| Preflight finds drift | Perform no filesystem side effect unless the user explicitly allows current-subtree drift. |
| Move/delete partially fails | Settle successful roots; failed roots remain visible and retryable. |
| App closes during decision | The completed SQLite transaction is authoritative; no debounce window exists. |
| App/daemon stops during commit | Persisted parent job resumes or reconciliation restores pending/failed truth before another group runs. |
| Preview scan reaches budget | Show available samples and a “sampled” indicator; do not continue unbounded traversal. |

Errors shown to the user include the actionable path and operation, while logs include task ID, item ID, and child job ID through `tracing` or job context logging.

## 16. Test specification

Tests are organized so business rules fail at the lowest-cost layer that owns them.

### 16.1 Pure Rust unit tests

`tree.rs` uses table-driven tests over varied trees, including empty root, one file, empty directories, deep chains, wide sibling sets, and mixed file/directory trees.

Required cases:

- Unit counts do not double-count non-empty directories.
- Tree intervals satisfy parent containment and sibling non-overlap.
- Progress sums sparse non-overlapping decisions correctly.
- All-Discard descendants collapse into one parent Discard.
- Mixed Keep/Discard/Move descendants return the exact confirmation counts.
- Confirmed parent override removes descendant decisions atomically.
- Different child decision under an ancestor requires split confirmation.
- Confirmed ancestor split leaves siblings unmarked.
- Same inherited decision is a no-op.
- Applied operation roots reject mutation.
- Batch normalization drops selected descendants.
- Planner output contains no nested delete or move roots.
- Move-before-delete ordering is stable.
- Unsafe destination ancestry and move cycles are rejected.
- Fixed-seed generated trees preserve `processed <= total` and category sums equal processed.

`path.rs` tests drive-letter case, slash normalization, trailing separators, UNC paths, prefix stripping, sibling-prefix false positives (`C:\photo` versus `C:\photos`), and volume roots.

Preview sampling tests cover one branch, many branches, image-only, video-only, mixed media, candidate caps, deterministic output, and the three-video cap.

### 16.2 Repository and migration tests

Use an in-memory SQLite database with the real migration.

Required cases:

- Both tables and every required index/constraint are created.
- Task/item cascade deletion removes metadata only.
- Active-root overlap rejects equal, ancestor, and descendant paths but permits siblings and completed roots.
- Snapshot batches produce a queryable root and stable paged children.
- Cursor ordering does not duplicate or skip equal-sort items.
- Decision mutation is transactional and increments revision once.
- Stale revision makes no changes.
- A confirmation-required outcome makes no changes.
- DirectChildren selection respects filter and exclusions.
- Accepting additions changes the denominator only after acceptance.
- Missing-item removal and changed-item reset rebuild intervals and counts.
- Organize entities are absent from sync-model registration.

### 16.3 Windows filesystem integration tests

Use temporary directories owned by the test process.

Required cases:

- Snapshot includes nested, hidden, empty, and mixed-extension files.
- Reparse points are represented without traversal when the test environment permits creating them; otherwise the test records a platform skip rather than weakening production behavior.
- Metadata-only snapshot does not create thumbnail or embedding sidecars.
- Change scan finds added, removed, and metadata-changed paths without changing the denominator.
- Commit preflight with drift performs zero moves and zero deletes.
- One flow moves a file to a temporary destination, permanently deletes another file and a directory, preserves Keep, and verifies the resulting disk state.
- A deliberately failing target leaves that root failed while an independent target settles.

### 16.4 Wire and generated-type contract tests

- Every required query/action is present in the registry with its exact method name.
- Specta generation includes task, item, selection, decision outcome, commit plan, and preview sequence types.
- `scripts/check-ts-types.sh` reports no drift.
- Frontend calls use generated DTOs with no `as any` or locally duplicated backend shape in the new module.

### 16.5 Frontend white-box tests

Use reducer/hook tests for selection and presentation rules rather than relying only on rendered happy paths.

Required cases:

- Plain click replaces; Ctrl-click toggles; Shift-click ranges.
- Plain lasso replaces; Ctrl-lasso unions; shrinking lasso removes no-longer-intersected items.
- Decision actions receive the complete selection scope.
- Directory cards render partial segmented progress and inherited whole-subtree state.
- Confirmation dialog text uses backend-provided counts.
- Move picker orders recent destinations, Locations, pinned Paths, and native browse correctly.
- Stale-revision response refetches instead of applying optimistic state.
- A 10,000-item fixture renders fewer than 300 item cards and decisions remain keyed to the correct UUID after scrolling.
- Directory preview renders a contact sheet, does not autoplay video, and opens the shared Quick Preview renderer.
- Completed tasks are read-only; reopened tasks permit new unapplied decisions.

### 16.6 Vertical WebDriver flow

Replace the old ViewMode-specific assumptions in `tests/webdriver/test_real_tauri_app.py` with one real Windows task flow over a temporary directory:

1. Create nested albums, images, a small video fixture, a move destination, and delete targets.
2. Open Explorer at the root and create a task from the real entry point.
3. Wait for snapshot completion and open a nested directory.
4. Verify parent progress changes after a child Keep decision.
5. Verify plain lasso replaces selection and Ctrl-lasso adds to it.
6. Mark one directory Discard, encounter a conflicting Keep descendant, cancel once, then confirm override.
7. Mark a different item Move through the destination picker.
8. Reload the app and verify decisions and progress survive.
9. Execute, wait for the organize commit job, and assert the real moved/deleted/preserved disk state.
10. Verify failed or drifted execution performs no unexpected deletion.
11. Finish the task, verify read-only state, then reopen it.

The harness cleans its task rows and temporary files without touching user task records.

## 17. Execution slices

The implementation plan must expand these slices into test-first, frequently committed tasks. A slice is complete only when its stated vertical result works.

### Slice 1: Domain rules and local persistence

**Goal:** Establish the two-table model and pure, tested tree/decision/planning rules.

**Primary files:**

- Create the migration and two entities under `core/src/infra/db/`.
- Create `core/src/ops/organize/model.rs`, `path.rs`, `tree.rs`, and `repository.rs`.
- Register the module in `core/src/ops/mod.rs` and entities/migrations in their existing registries.

**Done:** In-memory DB tests and pure rule tests pass; no UI or filesystem execution is required.

### Slice 2: Create, snapshot, list, and reopen a task

**Goal:** Produce the first working vertical slice from an Explorer directory to a persisted recursive task visible after restart.

**Primary files:**

- Add `organize.create`, snapshot job, `organize.list`, `organize.get`, and `organize.children`.
- Add routes `/organize` and `/organize/:taskId`.
- Add the path-bar/context-menu entry and sidebar task group.
- Regenerate TypeScript types.

**Done:** A real Windows directory creates a metadata-only recursive task, paged children are visible at multiple depths, and the same task reopens after app restart.

### Slice 3: Virtualized review surface and correct selection

**Goal:** Make thousands of items navigable without DOM growth and make selection behave like Windows Explorer.

**Primary files:**

- Replace the current `OrganizeCenterPane` implementation with focused task-list/grid components under `packages/interface/src/routes/organize/`.
- Reuse `@tanstack/react-virtual` patterns from existing Grid/List views.
- Add a pure selection reducer and lasso adapter.

**Done:** The 10,000-item rendering bound and click/Ctrl/Shift/lasso contracts pass; actions receive the full selection.

### Slice 4: Decisions, recursive progress, and override confirmations

**Goal:** Persist Keep/Discard/Move subtree decisions and show correct directory/task progress.

**Primary files:**

- Add `organize.set_decision` and decision outcomes.
- Add task filters, decision bar, segmented directory progress, conflict dialogs, and bounded undo.

**Done:** Mixed descendant conflict, same-decision collapse, ancestor split, persistence, and recursive progress all pass backend and frontend tests.

### Slice 5: Native shared preview sequence

**Goal:** Make representative directory preview a reusable file-manager capability and use it in the task.

**Primary files:**

- Add the bounded preview-sequence backend module/query.
- Refactor `packages/interface/src/components/QuickPreview/DirectoryPreview.tsx` into a capped representative view.
- Remove Organize-only recursive 10,000-item preview logic while retaining existing image/video renderers and preferences.

**Done:** File preview, directory contact sheet, sampling limits, video pause behavior, keyboard sequence, and no-media fallback pass.

### Slice 6: Move selection and safe commit

**Goal:** Execute compact Move and permanent Discard plans with preflight and per-root settlement.

**Primary files:**

- Add commit-plan query, commit action/job, destination validation, and operation reconciliation.
- Add Move picker and execution review dialog.
- Reuse `FileCopyJob` and `DeleteJob`; do not duplicate their filesystem strategies.

**Done:** Moves run before deletes; parent Discard emits one recursive root; drift emits zero side effects; partial failures remain retryable; Keep emits no job.

### Slice 7: Change review, task lifecycle, and legacy import

**Goal:** Complete fixed-snapshot maintenance and retire per-directory JSON safely.

**Primary files:**

- Add change scan, accept changes, retry, finish, reopen, and delete-task-record actions.
- Add legacy-state listing/import UI and temporary Tauri command.
- Remove the active old Organize persistence adapter and write command only after successful migration coverage; retain the read/archive import boundary for this version.

**Done:** New files do not change progress before acceptance; accepted changes rebuild the manifest; legacy decisions import without loss through the retained read/archive boundary; completed/reopened behavior passes.

### Slice 8: Full verification and old-view retirement

**Goal:** Prove the approved flow and remove obsolete wiring.

**Primary files:**

- Update locales, generated i18n types, help shortcuts, WebDriver harness, and old Organize tests.
- Remove `organize` from Explorer ViewMode and delete code that has no remaining reusable responsibility.

**Done:** All verification commands and the vertical WebDriver flow pass, and no old path-key JSON persistence or view-mode entry remains active.

## 18. Verification commands

Implementation completion requires fresh successful output from:

```powershell
cargo fmt --check
cargo test -p sd-core organize
cargo test -p sd-core --test organize_task_flow
cargo run --bin generate_typescript_types
bun run --filter @sd/interface typecheck
bun test packages/interface/src/routes/organize packages/interface/src/components/QuickPreview
```

Type drift must also pass the repository check used by CI:

```bash
./scripts/check-ts-types.sh
```

At the final vertical milestone, launch the real Tauri app with WebView2 debugging and run:

```powershell
python tests/webdriver/test_real_tauri_app.py
```

If the implementation changes the exact package/test target name, the implementation plan must replace the command with the factual equivalent before execution. It may not omit the corresponding test layer.

## 19. Definition of Done

The feature is Done only when all of the following are true:

1. Organize is a task route and sidebar narrative, not an Explorer ViewMode.
2. A physical Windows directory can create one recursive fixed-snapshot task.
3. Active task roots cannot overlap silently.
4. The task reopens after app and daemon restart with identical scope, decisions, revision, and progress.
5. Users can navigate every snapshotted level without loading per-directory state files.
6. A directory displays correct recursive Keep, Discard, Move, and Unmarked progress.
7. Keep has no physical side effect.
8. Parent Discard with only Discard descendants collapses without prompting.
9. Parent Discard with Keep or Move descendants requires and honors explicit override confirmation.
10. Keep and Move obey the same same-decision collapse and mixed-decision conflict rule.
11. Selection follows the plain/Ctrl/Shift/lasso contract and bulk decisions affect the full selection.
12. A 10,000-child directory stays virtualized within the stated DOM bound.
13. File and directory preview share the native bounded preview sequence and do not issue unbounded recursive media queries.
14. Move destinations come from recent targets, Locations, pinned Paths, or the native picker.
15. Commit preflights all roots before side effects, moves before deletes, and dispatches only compact roots.
16. Discard uses existing confirmed permanent recursive deletion and never the recycle bin.
17. Filesystem drift blocks execution without side effects unless the user explicitly overrides current-subtree drift.
18. Successful operation roots settle as applied; failed roots remain visible and retryable.
19. New filesystem items do not change the denominator until explicitly accepted.
20. The metadata signature avoids full hashing while never being treated as content identity.
21. Existing JSON decisions have a tested import path before old persistence commands are removed.
22. Backend DTOs are generated into `@sd/ts-client`; the new frontend contains no duplicated backend types or `as any` escape.
23. Unit, repository, Windows filesystem, contract, frontend, and vertical WebDriver tests described in section 16 pass.
24. A user explicitly finishes the task; completion is not inferred from progress.
25. No cross-platform behavior, sync system, generic workflow engine, or unrelated refactor has been added.

When these conditions hold, design and implementation stop. Improvements outside them require a new approved scope.
