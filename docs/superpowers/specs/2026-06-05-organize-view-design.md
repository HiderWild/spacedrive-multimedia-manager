# Explorer Organize View Design Spec

> **Date:** 2026-06-05
> **Status:** Approved
> **Scope:** `packages/interface` Explorer UI + platform persistence adapter + existing delete/media backend ops

## Goal

Add the first "Organize View" to Spacedrive Explorer so a user can review the direct children of a directory, preview media quickly, mark each direct child as keep or discard, and permanently delete all discard-marked direct children from a dedicated action in the Discard tab after explicit confirmation.

The organizing workflow is optimized for reviewing:

- Direct child directories
- Direct child media files
- Direct child non-media files that still need keep/discard decisions

Recursive traversal is used only to power preview for a selected directory. It does not change the decision scope, which remains limited to the current directory's direct children.

## Confirmed Product Decisions

- Organize View is a new Explorer `viewMode`.
- The layout is three columns:
  - Left narrow results pane
  - Center primary file pane
  - Right preview pane
- The left pane has two tabs:
  - `Keep`
  - `Discard`
- The center pane supports two layouts:
  - `list`
  - `grid` / tiled thumbnails
- The right pane auto-previews the selected item.
- Decisions apply only to direct children of the current directory.
- Directories are decided as whole units.
- Recursive traversal is preview-only.
- Decision state persists under the current user's `.spacedrive` directory, mapped by the current directory path.
- The persistence record for a directory is created only after the first keep/discard action.
- Keep/discard items remain visible in the center pane, but are greyed out and badged.
- The Discard tab contains an `立即删除` button.
- `立即删除` permanently deletes all discard-marked direct children after a confirmation modal.
- Deletion does not use the recycle bin / trash.
- Confirmation modal behavior:
  - `Enter` confirms delete
  - `Esc` cancels and closes
  - click outside / lose focus cancels and closes
  - cancel preserves the pre-modal organize state

## Non-Goals for Phase 1

- No cross-directory batch organizer
- No auto-processing of descendants as separate decisions
- No soft-delete / trash integration
- No bulk multi-directory queue
- No advanced keyboard workflow beyond the confirmation modal requirements
- No background whole-tree indexing job specifically for organize mode
- No new backend delete operation unless existing `files.delete` proves insufficient

## Existing Code and Reuse Strategy

The current codebase already contains several useful pieces that this feature should reuse rather than replace.

### Explorer shell

The organize mode should plug into the existing Explorer routing and navigation stack:

- `packages/interface/src/routes/explorer/context.tsx`
- `packages/interface/src/routes/explorer/ExplorerView.tsx`
- `packages/interface/src/routes/explorer/panes/ExplorerPaneBody.tsx`
- `packages/interface/src/routes/explorer/ViewModeMenu.tsx`

### Existing file loading

Direct children of the current directory should continue using the standard Explorer directory listing path via:

- `useExplorerFiles()`
- `files.directory_listing`

### Existing recursive media query

Directory preview should prefer the existing recursive media query instead of implementing custom frontend directory walking:

- `files.media_listing`
- `include_descendants: true`

This already exists in:

- `packages/interface/src/routes/explorer/views/MediaView/MediaView.tsx`
- `core/src/ops/files/query/media_listing.rs`

### Existing delete operation

Permanent recursive deletion already exists in the current frontend/backend flow:

- `packages/interface/src/routes/explorer/hooks/useDeleteFiles.ts`
- `files.delete`
- `permanent: true`
- `recursive: true`

Organize View should reuse the same backend mutation path, but replace the current native `confirm()` flow with a dedicated organize-specific confirmation dialog.

### Existing dialog pattern

The confirmation modal should follow the same dialog creation pattern already used elsewhere:

- `dialogManager.create(...)`
- `useDialog(...)`
- `Dialog` from `@spacedrive/primitives`

## Architecture

The feature should be split into focused units with clear boundaries.

### 1. `OrganizeView`

**Responsibility:**
Own the three-column compose-and-render layer for organize mode.

**Should do:**

- Render left results pane, center file pane, right preview pane
- Read Explorer context and current directory
- Bind together organize state, preview state, and action handlers
- Select between center `list` and `grid` sub-layouts

**Should not do:**

- Read/write `.spacedrive` files directly
- Implement recursive media search itself
- Own delete mutation details

### 2. `organize-state`

**Responsibility:**
Own decision state for the current directory's direct children.

**Should do:**

- Load decision state for the current directory
- Store `keep`, `discard`, `undecided`
- Expose commands:
  - `markKeep(item)`
  - `markDiscard(item)`
  - `clearDecision(item)`
- Project decision subsets for left-pane tabs
- Write changes through a persistence adapter

**Should not do:**

- Render preview
- Run delete operations
- Know about video/image tab logic

### 3. `organize-persistence`

**Responsibility:**
Persist and restore organize decision files under `.spacedrive`.

**Should do:**

- Map current directory identity to a stable persistence key
- Load/save JSON for one organized directory
- Hide platform-specific filesystem access behind an adapter

**Should not do:**

- Depend directly on React components
- Hold transient UI state

### 4. `organize-preview`

**Responsibility:**
Resolve the right preview content for the current selection.

**Should do:**

- Preview single selected video/image files directly
- For a selected directory, fetch recursive media candidates using `files.media_listing`
- Determine which preview tabs are available
- Maintain preview loading/error state
- Persist and restore user volume/mute state

**Should not do:**

- Change keep/discard decisions
- Delete files

### 5. `organize-delete-dialog`

**Responsibility:**
Handle explicit permanent deletion of all discard-marked direct children.

**Should do:**

- Open from the Discard tab's `立即删除` button
- Show count and clear permanence warning
- Handle `Enter`, `Esc`, and outside-click cancellation rules
- On confirm, delete all current discard-marked direct children with permanent recursive delete
- On success, remove deleted items from both view state and persisted decision state

**Should not do:**

- Decide which items are discard-marked
- Traverse descendants for separate decisions

## Platform Boundary

The Explorer UI lives in `packages/interface`, but the persistence location requirement is explicitly the user's `.spacedrive` directory.

To keep coupling low, the UI must not hardcode a filesystem path or direct host API calls in random view components.

Instead, introduce a small persistence interface at the platform boundary.

### Proposed interface

```ts
interface OrganizePersistenceAdapter {
  loadDirectoryState(directoryKey: string): Promise<OrganizeDirectoryState | null>;
  saveDirectoryState(directoryKey: string, state: OrganizeDirectoryState): Promise<void>;
}
```

### Phase 1 assumption

Initial support should target platforms that can write to the local user profile directory, such as the desktop/Tauri app.

Unsupported environments may:

- hide the organize view mode, or
- show it as unavailable until a persistence adapter exists

The spec does not require a web filesystem implementation.

## State Model

Split runtime UI state from persisted decision state.

### `OrganizeSessionState`

Transient, in-memory state for the active organize screen.

```ts
type OrganizeSessionState = {
  currentDirectoryKey: string;
  selectedItemId: string | null;
  leftTab: 'keep' | 'discard';
  centerLayout: 'list' | 'grid';
  previewTab: 'video' | 'image' | 'list';
  previewStatus: 'idle' | 'loading' | 'ready' | 'error';
  deleteDialogOpen: boolean;
};
```

### `OrganizeDirectoryState`

Persisted per organized directory.

```ts
type OrganizeDecision = 'keep' | 'discard';

type OrganizeItemRecord = {
  itemId: string | null;
  path: string | null;
  name: string;
  kind: 'File' | 'Directory';
  decision: OrganizeDecision;
  updatedAt: string;
};

type OrganizeDirectoryState = {
  version: 1;
  directoryPath: string;
  updatedAt: string;
  items: Record<string, OrganizeItemRecord>;
};
```

## Persistence Layout

Store one JSON file per organized directory.

```text
.spacedrive/
  organize/
    v1/
      <directory-key>.json
```

### Keying rules

The filename must not be the raw physical path.

Use a stable derived key from a normalized directory identity to avoid:

- invalid filename characters
- very long paths
- path separator differences
- future migration issues

### Matching rules

When mapping current direct children back to saved records, match in this order:

1. `file.id` when stable and available
2. normalized `sd_path`
3. `kind + name` fallback only if needed

The persisted record should store both identity and display metadata so the UI can recover gracefully if one identity signal changes.

### File creation rules

- No file is created when a directory is first opened in organize mode.
- The file is created after the first keep/discard decision.
- Clearing the final remaining decision may leave an empty state file in place.

Leaving an empty file is preferred over deleting it because it preserves the fact that this directory has been organized before.

## View Layout

### Left pane

Fixed narrow list view with two tabs:

- `Keep`
- `Discard`

The pane only shows decided items.

#### Discard tab action

The Discard tab contains a primary action button:

- `立即删除`

This is a button, not a decorative label.

Its action scope is:

- all currently discard-marked direct children of the current directory

### Center pane

Primary file browsing area showing only direct children of the current directory.

Supported layouts:

- `list`
- `grid`

Visual decision rules:

- `undecided`: normal appearance
- `keep`: greyed out + green check badge at bottom-right
- `discard`: greyed out + red X badge at bottom-right

Decided items remain visible and selectable. They are not filtered out of the main pane.

### Right pane

Preview area for the currently selected direct child.

## Preview Behavior

### Single file selected

- If the file is a video:
  - start preview automatically
  - start muted by default
  - apply the user's last saved volume/mute state
- If the file is an image:
  - show image preview directly

### Directory selected

When the selected direct child is a directory:

- request recursive media via `files.media_listing` with `include_descendants: true`
- identify whether video exists
- identify whether image exists
- always allow the list preview tab

### Preview tabs for directory selection

Supported tabs:

- `video`
- `image`
- `list`

Default priority:

1. `video`
2. `image`
3. `list`

### Tab enable/disable rules

- If matching video exists, `video` tab is enabled
- If matching image exists, `image` tab is enabled
- `list` tab is always enabled for directories
- If a media type is missing:
  - its tab is greyed out
  - hover tooltip explains:
    - `未在该目录下找到视频`
    - `未在该目录下找到图像`
- If both video and image are absent:
  - do not show the media tabs at all
  - show only the list preview view

### Volume persistence

Volume and mute state are user-level preview preferences, not per-directory state.

They should persist across item changes in organize mode so that the next playable video preview uses the same setting.

## Decision Workflow

### Keep

When the user marks a direct child as keep:

- update in-memory organize state
- persist to `.spacedrive`
- show the item in the left `Keep` tab
- grey out the item in the center pane
- add green badge in the center pane

### Discard

When the user marks a direct child as discard:

- update in-memory organize state
- persist to `.spacedrive`
- show the item in the left `Discard` tab
- grey out the item in the center pane
- add red badge in the center pane

### Undo

When the user clears a decision:

- remove that item's record from state
- remove it from the relevant left-pane tab
- restore its undecided appearance in the center pane
- write updated directory state back to persistence

## Permanent Delete Flow

The Discard tab's `立即删除` button opens a dedicated confirmation modal.

### Modal content

The dialog should clearly communicate:

- how many items will be deleted
- that deletion is permanent
- that deletion will not use the recycle bin / trash
- that directories will be deleted recursively

### Modal actions

- Confirm button: execute permanent delete
- Cancel button: close and abort

### Keyboard and focus behavior

- `Enter`: confirm immediately
- `Esc`: close and cancel
- click outside / lose focus: close and cancel
- closing the modal by cancel paths must preserve:
  - existing keep/discard marks
  - current selection
  - active tab
  - center layout
  - preview state

### Delete execution

On confirm:

- snapshot the current discard-marked direct children
- call existing delete mutation path using:
  - `files.delete`
  - `permanent: true`
  - `recursive: true`
- on success:
  - remove deleted entries from the center pane
  - remove them from the left Discard tab
  - remove their persisted discard records
- show success feedback

### Failure behavior

Preferred behavior for phase 1:

- attempt the full batch through the existing mutation pathway
- if the delete request fails as a whole:
  - keep all discard state intact
  - keep all items visible
  - show error feedback
- if partial success granularity is later exposed by backend responses, it can be layered in later

This keeps phase 1 simple and consistent with the current existing delete abstraction.

## Error Handling

### Persistence read failure

- do not block the view from opening
- fall back to empty organize state
- surface lightweight error feedback

### Persistence write failure

- preserve in-memory decisions for the active session
- show clear error feedback that local organize state was not saved
- allow retry via the next decision mutation or explicit retry later

### Corrupted JSON state

- treat as corrupted local organize state
- do not crash the Explorer
- do not overwrite automatically on initial read
- allow reset/recovery behavior in implementation

### Preview failure

- if media lookup fails for a directory, keep list preview available
- show lightweight preview error state in the right pane

### Delete failure

- if permanent delete fails, do not silently mutate the decision state
- preserve the discard list and let the user retry

## File and Module Layout

Proposed additions under `packages/interface/src/routes/explorer/`:

```text
routes/explorer/
  organize/
    OrganizeView.tsx
    OrganizeLayout.tsx
    OrganizeLeftPane.tsx
    OrganizeCenterPane.tsx
    OrganizePreviewPane.tsx
    OrganizeDeleteDialog.tsx
    organizeState.ts
    organizePersistence.ts
    organizePreview.ts
    organizeTypes.ts
```

Potential platform adapter surface:

```text
packages/interface/src/contexts/
  PlatformContext.tsx             — extended with organize persistence adapter methods

apps/tauri/
  ...                             — platform implementation for .spacedrive organize state files
```

Exact file placement may vary, but the module boundaries should remain the same.

## Integration Points

### Explorer context and view mode

Add `organize` to the Explorer `ViewMode` union and wire it into:

- view mode menu
- explorer pane body switch
- persisted view preference loading where appropriate

### Current file source

The center pane should continue using direct children from existing Explorer listing logic so it remains consistent with sorting, filtering, and selection behavior.

### Selection model

Organize View should reuse the existing selection context for the active selected item, while decisions remain in organize-specific state.

### Delete hook reuse

The organize delete dialog should reuse the existing backend mutation path but should not reuse the current `confirm()` wrapper in `useDeleteFiles` unchanged.

Recommended direction:

- extract the mutation core from `useDeleteFiles`
- let Explorer legacy paths keep using `confirm()` for now
- let Organize View call the same mutation core behind a custom modal

This avoids duplicating delete mutation logic.

## Testing and Acceptance Criteria

### Organize state persistence

- opening an unorganized directory loads empty state
- first decision creates the state file
- re-entering the same directory restores saved keep/discard decisions
- different directories restore different saved states without leaking into each other

### Center pane rendering

- keep items are greyed out with green badge
- discard items are greyed out with red badge
- undo restores undecided appearance
- decided items remain previewable and selectable

### Left pane projection

- keep tab lists only keep-marked direct children
- discard tab lists only discard-marked direct children
- clicking a left-pane entry selects or reveals the corresponding center-pane item
- discard tab shows the `立即删除` button

### Preview behavior

- video file selection auto-plays preview muted
- saved mute/volume state is reused on the next video preview
- image file selection shows image preview
- directory selection uses recursive media lookup
- directory with only video enables video + list behavior
- directory with only image enables image + list behavior
- directory with both enables all relevant tabs
- directory with neither shows only list preview

### Delete confirmation modal

- clicking `立即删除` opens the confirmation modal
- `Enter` confirms delete
- `Esc` cancels and closes
- outside click / blur cancels and closes
- cancel does not change organize state

### Permanent delete execution

- confirmed delete calls the permanent recursive delete pathway
- deleted items disappear from the current directory listing after mutation success
- deleted items are removed from persisted discard state
- failed delete preserves discard state and shows error feedback

## Implementation Notes

- Keep comments/documentation additions minimal during implementation unless needed.
- Preserve type safety using existing generated `@sd/ts-client` types.
- Prefer reusing existing Explorer state, selection, and data hooks over parallel abstractions.
- Keep platform-specific filesystem persistence outside generic UI components.

## Recommended First Implementation Slice

1. Add `organize` view mode shell and three-column layout.
2. Implement per-directory organize state and local persistence adapter.
3. Render keep/discard state in center pane and projection in left tabs.
4. Implement right preview pane for file/directory preview using existing media query support.
5. Add Discard tab `立即删除` button and confirmation dialog.
6. Wire permanent recursive delete through the existing backend mutation path.
7. Add regression coverage for persistence, preview tab rules, and delete confirmation behavior.
