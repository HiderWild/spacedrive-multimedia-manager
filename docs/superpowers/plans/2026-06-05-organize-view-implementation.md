# Organize View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `organize` as a real Explorer `viewMode` with per-directory keep/discard persistence, a three-column organize UI, recursive media preview for selected directories, and permanent delete from the Discard tab after explicit confirmation.

**Architecture:** Keep organize logic under `packages/interface/src/routes/explorer/organize/`, expose persistence through `PlatformContext`, back it with two small Tauri commands, and reuse existing Explorer data hooks, selection state, `FileComponent.Thumb`, `VideoPlayer`, `files.directory_listing`, `files.media_listing`, `files.delete`, and `@spacedrive/primitives` dialog primitives.

**Tech Stack:** React 19, TypeScript, Tauri 2, Rust async commands, TanStack Query via `useNormalizedQuery`, Bun test, Cargo test.

> **Scope update (2026-06-06):** Current agent execution scope covers logic, persistence, Explorer wiring, preview resolution, and delete-flow code. UI debugging and unified manual UI verification are user-owned and out of current execution scope for this plan.

---

## File Map

- `packages/interface/src/contexts/PlatformContext.tsx`
  - Add `loadOrganizeState` and `saveOrganizeState`.
- `apps/tauri/src/platform.ts`
  - Bind the new Tauri organize commands.
- `apps/web/src/platform.ts`
  - Leave organize persistence unavailable.
- `apps/tauri/src-tauri/src/organize.rs`
  - Implement organize JSON read/write and tests.
- `apps/tauri/src-tauri/src/main.rs`
  - Register `load_organize_state` and `save_organize_state`.
- `apps/tauri/src-tauri/Cargo.toml`
  - Add `tempfile` for Rust tests.
- `packages/interface/src/routes/explorer/organize/organizeTypes.ts`
  - Shared types.
- `packages/interface/src/routes/explorer/organize/organizePersistence.ts`
  - Path normalization, directory-key generation, JSON serialization.
- `packages/interface/src/routes/explorer/organize/organizeState.ts`
  - Decision helpers, state hook, presentation selectors, delete cleanup.
- `packages/interface/src/routes/explorer/organize/organizeAvailability.ts`
  - Gate organize mode to browse + physical path + persistence-capable platform.
- `packages/interface/src/routes/explorer/organize/organizePreview.ts`
  - Preview tab derivation and sort coercion helpers.
- `packages/interface/src/routes/explorer/organize/OrganizeView.tsx`
  - Top-level organize composition.
- `packages/interface/src/routes/explorer/organize/OrganizeLayout.tsx`
  - Three-column shell.
- `packages/interface/src/routes/explorer/organize/OrganizeLeftPane.tsx`
  - Keep/Discard tabs + delete entry point.
- `packages/interface/src/routes/explorer/organize/OrganizeCenterPane.tsx`
  - Main direct-children list/grid with badges and actions.
- `packages/interface/src/routes/explorer/organize/OrganizePreviewPane.tsx`
  - File/directory preview pane.
- `packages/interface/src/routes/explorer/organize/OrganizeDeleteDialog.tsx`
  - Custom permanent delete modal.
- `packages/interface/src/routes/explorer/context.tsx`
  - Extend `ViewMode` with `organize`.
- `packages/interface/src/components/TabManager/TabManagerContext.tsx`
  - Extend persisted per-tab view-mode union.
- `packages/ts-client/src/stores/viewPreferences.ts`
  - Extend view preference union.
- `packages/interface/src/routes/explorer/ViewModeMenu.tsx`
  - Add organize option and availability filter.
- `packages/interface/src/routes/explorer/ExplorerView.tsx`
  - Pass organize availability into the menu.
- `packages/interface/src/routes/explorer/panes/ExplorerPaneBody.tsx`
  - Route `organize` to `OrganizeView`.
- `packages/interface/src/routes/explorer/views/SearchView/SearchView.tsx`
  - Fallback away from organize.
- `packages/interface/src/routes/explorer/views/RecentsView/RecentsView.tsx`
  - Fallback away from organize.
- `packages/interface/src/routes/explorer/hooks/useDeleteFiles.ts`
  - Extract confirmation-free delete helper for organize.
- `packages/interface/src/locales/en/explorer.json`
  - Add organize copy.
- `packages/interface/src/locales/zh/explorer.json`
  - Add organize copy.
- `packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts`
- `packages/interface/src/routes/explorer/organize/__tests__/organizeAvailability.test.ts`
- `packages/interface/src/routes/explorer/organize/__tests__/organizePreview.test.ts`
- `packages/interface/src/routes/explorer/organize/__tests__/organizeDelete.test.ts`

## Task 1: Add the platform persistence bridge

**Files:**
- `apps/tauri/src-tauri/src/organize.rs`
- `apps/tauri/src-tauri/src/main.rs`
- `apps/tauri/src-tauri/Cargo.toml`
- `packages/interface/src/contexts/PlatformContext.tsx`
- `apps/tauri/src/platform.ts`
- `apps/web/src/platform.ts`

- [ ] **Write the failing Rust tests**

```rust
#[cfg(test)]
mod tests {
	use super::{build_organize_state_path, load_state_file, save_state_file};
	use tempfile::tempdir;

	#[tokio::test]
	async fn build_organize_state_path_nests_file_under_organize_v1() {
		let root = tempdir().unwrap();
		let path = build_organize_state_path(root.path(), "dir-abc123").unwrap();
		assert_eq!(path, root.path().join("organize").join("v1").join("dir-abc123.json"));
	}

	#[tokio::test]
	async fn save_and_load_round_trip_json() {
		let root = tempdir().unwrap();
		let json = r#"{"version":1,"directoryPath":"C:/Photos","updatedAt":"2026-06-05T15:00:00.000Z","items":{}}"#;
		save_state_file(root.path(), "dir-abc123", json).await.unwrap();
		let loaded = load_state_file(root.path(), "dir-abc123").await.unwrap();
		assert_eq!(loaded.as_deref(), Some(json));
	}
}
```

- [ ] **Run the failure first**

Run: `cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml organize::tests`

- [ ] **Implement the bridge**

```toml
[dev-dependencies]
tempfile = "3.13"
```

```rust
use std::path::{Path, PathBuf};
use sd_tauri_core::default_data_dir;
use tokio::fs;

fn build_organize_state_path(root: &Path, directory_key: &str) -> Result<PathBuf, String> {
	if !directory_key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
		return Err(format!("Invalid organize directory key: {}", directory_key));
	}
	Ok(root.join("organize").join("v1").join(format!("{}.json", directory_key)))
}

async fn load_state_file(root: &Path, directory_key: &str) -> Result<Option<String>, String> {
	let path = build_organize_state_path(root, directory_key)?;
	match fs::read_to_string(path).await {
		Ok(json) => Ok(Some(json)),
		Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
		Err(err) => Err(format!("Failed to read organize state: {}", err)),
	}
}

async fn save_state_file(root: &Path, directory_key: &str, json: &str) -> Result<(), String> {
	let path = build_organize_state_path(root, directory_key)?;
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).await.map_err(|err| format!("Failed to create organize directory: {}", err))?;
	}
	fs::write(path, json).await.map_err(|err| format!("Failed to write organize state: {}", err))
}

#[tauri::command]
pub async fn load_organize_state(directory_key: String) -> Result<Option<String>, String> {
	let data_dir = default_data_dir().map_err(|err| format!("Failed to get data directory: {}", err))?;
	load_state_file(&data_dir, &directory_key).await
}

#[tauri::command]
pub async fn save_organize_state(directory_key: String, json: String) -> Result<(), String> {
	let data_dir = default_data_dir().map_err(|err| format!("Failed to get data directory: {}", err))?;
	save_state_file(&data_dir, &directory_key, &json).await
}
```

```rust
mod organize;
```

```rust
organize::load_organize_state,
organize::save_organize_state,
```

```ts
loadOrganizeState?(directoryKey: string): Promise<string | null>;
saveOrganizeState?(directoryKey: string, json: string): Promise<void>;
```

```ts
async loadOrganizeState(directoryKey: string) {
	return await invoke<string | null>("load_organize_state", { directoryKey });
},
async saveOrganizeState(directoryKey: string, json: string) {
	await invoke("save_organize_state", { directoryKey, json });
},
```

- [ ] **Verify**

Run: `cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml organize::tests`

Run: `bun run --filter @sd/interface typecheck`

- [ ] **Commit**

```bash
git add apps/tauri/src-tauri/Cargo.toml apps/tauri/src-tauri/src/main.rs apps/tauri/src-tauri/src/organize.rs apps/tauri/src/platform.ts apps/web/src/platform.ts packages/interface/src/contexts/PlatformContext.tsx
git commit -m "feat: add organize persistence bridge"
```

## Task 2: Add organize state and persistence helpers

**Files:**
- `packages/interface/src/routes/explorer/organize/organizeTypes.ts`
- `packages/interface/src/routes/explorer/organize/organizePersistence.ts`
- `packages/interface/src/routes/explorer/organize/organizeState.ts`
- `packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts`

- [ ] **Write the failing Bun test**

```ts
import { describe, expect, test } from "bun:test";
import type { File } from "@sd/ts-client";
import { buildOrganizeDirectoryKey, createEmptyOrganizeDirectoryState } from "../organizePersistence";
import { projectOrganizeBucket, removeDeletedOrganizeEntries, upsertOrganizeDecision } from "../organizeState";

const makeFile = (overrides: Partial<File>): File => ({
	id: "file-1",
	name: "clip",
	kind: "File",
	extension: "mp4",
	sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/clip.mp4" } },
	...overrides,
} as File);

describe("organize state helpers", () => {
	test("normalizes equivalent directory paths to the same key", () => {
		expect(buildOrganizeDirectoryKey("C:\\Photos\\Trip")).toBe(buildOrganizeDirectoryKey("C:/Photos/Trip/"));
	});

	test("projects keep and discard buckets from persisted state", () => {
		let state = createEmptyOrganizeDirectoryState("C:/Photos");
		state = upsertOrganizeDecision(state, makeFile({ id: "keep-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/keep.mp4" } } }), "keep");
		state = upsertOrganizeDecision(state, makeFile({ id: "discard-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/discard.mp4" } } }), "discard");
		const files = [
			makeFile({ id: "keep-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/keep.mp4" } } }),
			makeFile({ id: "discard-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/discard.mp4" } } }),
		];
		expect(projectOrganizeBucket(files, state, "keep").map((file) => file.id)).toEqual(["keep-1"]);
		expect(projectOrganizeBucket(files, state, "discard").map((file) => file.id)).toEqual(["discard-1"]);
	});

	test("drops persisted entries for deleted paths", () => {
		let state = createEmptyOrganizeDirectoryState("C:/Photos");
		state = upsertOrganizeDecision(state, makeFile({ id: "discard-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/discard.mp4" } } }), "discard");
		state = upsertOrganizeDecision(state, makeFile({ id: "keep-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/keep.mp4" } } }), "keep");
		const next = removeDeletedOrganizeEntries(state, ["C:/Photos/discard.mp4"]);
		expect(Object.values(next.items).map((record) => record.path)).toEqual(["C:/Photos/keep.mp4"]);
	});
});
```

- [ ] **Run the failure first**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts`

- [ ] **Implement the helpers**

```ts
export type OrganizeDecision = "keep" | "discard";
export type OrganizeLeftTab = "keep" | "discard";
export type OrganizeCenterLayout = "list" | "grid";
export type OrganizePreviewTab = "video" | "image" | "list";

export interface OrganizeItemRecord {
	itemId: string | null;
	path: string | null;
	name: string;
	kind: "File" | "Directory";
	decision: OrganizeDecision;
	updatedAt: string;
}

export interface OrganizeDirectoryState {
	version: 1;
	directoryPath: string;
	updatedAt: string;
	items: Record<string, OrganizeItemRecord>;
}
```

```ts
export function normalizeOrganizePath(physicalPath: string): string {
	const normalized = physicalPath.replaceAll("\\", "/").replace(/\/+/g, "/");
	return normalized.length > 1 ? normalized.replace(/\/$/, "") : normalized;
}

export function buildOrganizeDirectoryKey(physicalPath: string): string {
	let hash = 14695981039346656037n;
	for (const char of normalizeOrganizePath(physicalPath)) {
		hash ^= BigInt(char.charCodeAt(0));
		hash = BigInt.asUintN(64, hash * 1099511628211n);
	}
	return `dir-${hash.toString(16).padStart(16, "0")}`;
}

export function getPhysicalPath(sdPath: File["sd_path"] | null | undefined): string | null {
	if (!sdPath || !("Physical" in sdPath) || !sdPath.Physical?.path) return null;
	return sdPath.Physical.path;
}

export function getOrganizeItemKey(file: Pick<File, "id" | "sd_path" | "name" | "kind">): string {
	if (file.id) return `id:${file.id}`;
	const path = getPhysicalPath(file.sd_path);
	if (path) return `path:${normalizeOrganizePath(path)}`;
	return `fallback:${file.kind}:${file.name}`;
}

export function createEmptyOrganizeDirectoryState(directoryPath: string): OrganizeDirectoryState {
	return { version: 1, directoryPath, updatedAt: new Date().toISOString(), items: {} };
}
```

```ts
export function upsertOrganizeDecision(state: OrganizeDirectoryState, file: File, decision: OrganizeDecision): OrganizeDirectoryState {
	const key = getOrganizeItemKey(file);
	return {
		...state,
		updatedAt: new Date().toISOString(),
		items: {
			...state.items,
			[key]: {
				itemId: file.id ?? null,
				path: getPhysicalPath(file.sd_path),
				name: file.name,
				kind: file.kind,
				decision,
				updatedAt: new Date().toISOString(),
			},
		},
	};
}

export function projectOrganizeBucket(files: File[], state: OrganizeDirectoryState, decision: OrganizeDecision): File[] {
	return files.filter((file) => state.items[getOrganizeItemKey(file)]?.decision === decision);
}

export function buildOrganizePresentation(files: File[], state: OrganizeDirectoryState) {
	return files.map((file) => {
		const record = state.items[getOrganizeItemKey(file)] ?? null;
		return { file, decision: record?.decision ?? null, dimmed: Boolean(record) };
	});
}

export function removeDeletedOrganizeEntries(state: OrganizeDirectoryState, deletedPaths: string[]): OrganizeDirectoryState {
	const deleted = new Set(deletedPaths.map((path) => normalizeOrganizePath(path)));
	const items = Object.fromEntries(Object.entries(state.items).filter(([, record]) => !record.path || !deleted.has(normalizeOrganizePath(record.path))));
	return { ...state, updatedAt: new Date().toISOString(), items };
}
```

- [ ] **Verify**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts`

- [ ] **Commit**

```bash
git add packages/interface/src/routes/explorer/organize/organizeTypes.ts packages/interface/src/routes/explorer/organize/organizePersistence.ts packages/interface/src/routes/explorer/organize/organizeState.ts packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts
git commit -m "feat: add organize decision state helpers"
```

## Task 3: Wire organize in as a real Explorer view mode

**Files:**
- `packages/interface/src/routes/explorer/organize/organizeAvailability.ts`
- `packages/interface/src/routes/explorer/context.tsx`
- `packages/interface/src/components/TabManager/TabManagerContext.tsx`
- `packages/ts-client/src/stores/viewPreferences.ts`
- `packages/interface/src/routes/explorer/ViewModeMenu.tsx`
- `packages/interface/src/routes/explorer/ExplorerView.tsx`
- `packages/interface/src/routes/explorer/panes/ExplorerPaneBody.tsx`
- `packages/interface/src/routes/explorer/views/SearchView/SearchView.tsx`
- `packages/interface/src/routes/explorer/views/RecentsView/RecentsView.tsx`
- `packages/interface/src/locales/en/explorer.json`
- `packages/interface/src/locales/zh/explorer.json`
- `packages/interface/src/routes/explorer/organize/__tests__/organizeAvailability.test.ts`

- [ ] **Write the failing availability test**

```ts
import { describe, expect, test } from "bun:test";
import type { SdPath } from "@sd/ts-client";
import type { ExplorerMode } from "../../context";
import type { Platform } from "../../../../contexts/PlatformContext";
import { canUseOrganizeView } from "../organizeAvailability";

const browseMode: ExplorerMode = { type: "browse" };
const searchMode: ExplorerMode = { type: "search", query: "cat", scope: "folder" };
const physicalPath: SdPath = { Physical: { device_slug: "disk", path: "C:/Photos" } };
const tauriPlatform = {
	platform: "tauri",
	openLink() {},
	confirm(_message: string, callback: (result: boolean) => void) { callback(true); },
	loadOrganizeState: async () => null,
	saveOrganizeState: async () => {},
} satisfies Platform;
const webPlatform = {
	platform: "web",
	openLink() {},
	confirm(_message: string, callback: (result: boolean) => void) { callback(true); },
} satisfies Platform;

describe("canUseOrganizeView", () => {
	test("requires browse mode, a physical path, and persistence methods", () => {
		expect(canUseOrganizeView({ platform: tauriPlatform, mode: browseMode, currentPath: physicalPath })).toBe(true);
		expect(canUseOrganizeView({ platform: tauriPlatform, mode: searchMode, currentPath: physicalPath })).toBe(false);
		expect(canUseOrganizeView({ platform: webPlatform, mode: browseMode, currentPath: physicalPath })).toBe(false);
	});
});
```

- [ ] **Run the failure first**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeAvailability.test.ts`

- [ ] **Implement view-mode plumbing**

```ts
export function canUseOrganizeView(args: { platform: Platform; mode: ExplorerMode; currentPath: SdPath | null }): boolean {
	return (
		args.mode.type === "browse" &&
		Boolean(getPhysicalPath(args.currentPath ?? null)) &&
		typeof args.platform.loadOrganizeState === "function" &&
		typeof args.platform.saveOrganizeState === "function"
	);
}
```

```ts
export type ViewMode = "grid" | "list" | "media" | "masonry" | "column" | "size" | "knowledge" | "organize";
```

```tsx
{
	id: "organize",
	label: i18n.t("viewModes.organize", { ns: "explorer" }),
	icon: Folders,
	color: "bg-emerald-500",
	keybind: "⌘8",
},

const availableViews = viewOptions.filter((option) => {
	if (option.id === "organize") return isOrganizeAvailable;
	if (option.id === "knowledge") return import.meta.env.DEV;
	return true;
});
```

```tsx
const isOrganizeAvailable = canUseOrganizeView({ platform, mode, currentPath });
<ViewModeMenuPanel viewMode={viewMode} onViewModeChange={handleViewModeChange} isOrganizeAvailable={isOrganizeAvailable} />
```

```tsx
case "organize":
	return <OrganizeView />;
```

```tsx
case "organize":
	return <GridView />;
```

```json
"viewModes": { "organize": "Organize" },
"organize": {
	"title": "Organize",
	"keepTab": "Keep",
	"discardTab": "Discard",
	"keepAction": "Keep",
	"discardAction": "Discard",
	"clearAction": "Clear",
	"deleteNow": "Delete now",
	"previewList": "List",
	"previewEmpty": "Select an item to preview"
}
```

```json
"viewModes": { "organize": "整理" },
"organize": {
	"title": "整理视图",
	"keepTab": "保留",
	"discardTab": "丢弃",
	"keepAction": "保留",
	"discardAction": "丢弃",
	"clearAction": "撤销",
	"deleteNow": "立即删除",
	"previewList": "列表",
	"previewEmpty": "请选择一个条目进行预览"
}
```

- [ ] **Verify**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeAvailability.test.ts`

Run: `bun run --filter @sd/interface typecheck`

- [ ] **Commit**

```bash
git add packages/interface/src/routes/explorer/organize/organizeAvailability.ts packages/interface/src/routes/explorer/context.tsx packages/interface/src/components/TabManager/TabManagerContext.tsx packages/ts-client/src/stores/viewPreferences.ts packages/interface/src/routes/explorer/ViewModeMenu.tsx packages/interface/src/routes/explorer/ExplorerView.tsx packages/interface/src/routes/explorer/panes/ExplorerPaneBody.tsx packages/interface/src/routes/explorer/views/SearchView/SearchView.tsx packages/interface/src/routes/explorer/views/RecentsView/RecentsView.tsx packages/interface/src/locales/en/explorer.json packages/interface/src/locales/zh/explorer.json packages/interface/src/routes/explorer/organize/__tests__/organizeAvailability.test.ts
git commit -m "feat: add organize explorer view mode"
```

## Task 4: Build the organize shell and left/center panes

**Files:**
- `packages/interface/src/routes/explorer/organize/organizeState.ts`
- `packages/interface/src/routes/explorer/organize/OrganizeView.tsx`
- `packages/interface/src/routes/explorer/organize/OrganizeLayout.tsx`
- `packages/interface/src/routes/explorer/organize/OrganizeLeftPane.tsx`
- `packages/interface/src/routes/explorer/organize/OrganizeCenterPane.tsx`
- `packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts`

- [ ] **Add a failing presentation test**

```ts
test("keeps decided items in the center list while only matching decisions appear in left buckets", () => {
	let state = createEmptyOrganizeDirectoryState("C:/Photos");
	state = upsertOrganizeDecision(state, makeFile({ id: "keep-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/keep.mp4" } } }), "keep");
	const files = [
		makeFile({ id: "keep-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/keep.mp4" } } }),
		makeFile({ id: "fresh-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/fresh.mp4" } } }),
	];
	const presentation = buildOrganizePresentation(files, state);
	expect(presentation.find((item) => item.file.id === "keep-1")).toMatchObject({ decision: "keep", dimmed: true });
	expect(presentation.find((item) => item.file.id === "fresh-1")).toMatchObject({ decision: null, dimmed: false });
});
```

- [ ] **Run the failure first**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts`

- [ ] **Implement the shell and decision flow**

```ts
export function useOrganizeState(args: { currentPath: SdPath | null; files: File[] }) {
	const platform = usePlatform();
	const directoryPath = getPhysicalPath(args.currentPath);
	const [state, setState] = useState<OrganizeDirectoryState | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [hasPersistedFile, setHasPersistedFile] = useState(false);

	useEffect(() => {
		if (!directoryPath || !platform.loadOrganizeState) {
			setState(directoryPath ? createEmptyOrganizeDirectoryState(directoryPath) : null);
			setHasPersistedFile(false);
			return;
		}
		setIsLoading(true);
		platform.loadOrganizeState(buildOrganizeDirectoryKey(directoryPath))
			.then((json) => {
				if (json) {
					setState(JSON.parse(json) as OrganizeDirectoryState);
					setHasPersistedFile(true);
				} else {
					setState(createEmptyOrganizeDirectoryState(directoryPath));
				}
			})
			.finally(() => setIsLoading(false));
	}, [directoryPath, platform]);

	const persist = useCallback(async (next: OrganizeDirectoryState) => {
		if (!directoryPath || !platform.saveOrganizeState) return;
		if (!hasPersistedFile && Object.keys(next.items).length === 0) return;
		await platform.saveOrganizeState(buildOrganizeDirectoryKey(directoryPath), JSON.stringify(next));
		setHasPersistedFile(true);
	}, [directoryPath, hasPersistedFile, platform]);

	const applyDecision = useCallback(async (file: File, decision: OrganizeDecision | null) => {
		if (!state) return;
		const next = decision ? upsertOrganizeDecision(state, file, decision) : clearOrganizeDecision(state, file);
		setState(next);
		await persist(next);
	}, [persist, state]);

	const removeDeleted = useCallback(async (files: File[]) => {
		if (!state) return;
		const next = removeDeletedOrganizeEntries(state, files.map((file) => getPhysicalPath(file.sd_path)).filter(Boolean) as string[]);
		setState(next);
		await persist(next);
	}, [persist, state]);

	return {
		isSupported: Boolean(directoryPath && platform.loadOrganizeState && platform.saveOrganizeState),
		isLoading,
		state,
		keepFiles: state ? projectOrganizeBucket(args.files, state, "keep") : [],
		discardFiles: state ? projectOrganizeBucket(args.files, state, "discard") : [],
		presentation: state ? buildOrganizePresentation(args.files, state) : [],
		markKeep: (file: File) => applyDecision(file, "keep"),
		markDiscard: (file: File) => applyDecision(file, "discard"),
		clearDecision: (file: File) => applyDecision(file, null),
		removeDeleted,
	};
}
```

```tsx
export function OrganizeLayout(props: { left: ReactNode; center: ReactNode; right: ReactNode }) {
	return (
		<div className="grid h-full min-h-0 grid-cols-[280px_minmax(0,1fr)_360px] gap-2 p-2">
			<section className="min-h-0 overflow-hidden rounded-xl border border-app-line bg-app/70">{props.left}</section>
			<section className="min-h-0 overflow-hidden rounded-xl border border-app-line bg-app/70">{props.center}</section>
			<section className="min-h-0 overflow-hidden rounded-xl border border-app-line bg-app/70">{props.right}</section>
		</div>
	);
}
```

```tsx
export function OrganizeView() {
	const platform = usePlatform();
	const explorer = useExplorer();
	const { files, isLoading } = useExplorerFiles();
	const { selectedFiles, selectFile, restoreSelectionFromFiles } = useSelection();
	const organize = useOrganizeState({ currentPath: explorer.currentPath, files });
	const [leftTab, setLeftTab] = useState<OrganizeLeftTab>("keep");
	const [layout, setLayout] = useState<OrganizeCenterLayout>("grid");

	useEffect(() => {
		explorer.setCurrentFiles(files);
		restoreSelectionFromFiles(files);
	}, [explorer, files, restoreSelectionFromFiles]);

	if (!canUseOrganizeView({ platform, mode: explorer.mode, currentPath: explorer.currentPath })) {
		return <GridView />;
	}

	if (isLoading || organize.isLoading || !organize.state) {
		return <div className="flex h-full items-center justify-center text-sm text-ink-dull">Loading organize view…</div>;
	}

	const selectedFile = selectedFiles[0] ?? null;
	return (
		<OrganizeLayout
			left={<OrganizeLeftPane leftTab={leftTab} onLeftTabChange={setLeftTab} keepFiles={organize.keepFiles} discardFiles={organize.discardFiles} onRevealItem={(file) => selectFile(file, files, false, false)} />}
			center={<OrganizeCenterPane files={files} selectedFileId={selectedFile?.id ?? null} layout={layout} onLayoutChange={setLayout} presentation={organize.presentation} onSelectFile={(file) => selectFile(file, files, false, false)} onMarkKeep={organize.markKeep} onMarkDiscard={organize.markDiscard} onClearDecision={organize.clearDecision} />}
			right={<OrganizePreviewPane selectedFile={selectedFile} />}
		/>
	);
}
```

```tsx
export function OrganizeLeftPane(props: { leftTab: OrganizeLeftTab; onLeftTabChange: (tab: OrganizeLeftTab) => void; keepFiles: File[]; discardFiles: File[]; onRevealItem: (file: File) => void; onDeleteClick?: () => void; }) {
	const items = props.leftTab === "keep" ? props.keepFiles : props.discardFiles;
	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="flex gap-1 border-b border-app-line p-2">
				<button className="flex-1 rounded-md px-3 py-2 text-sm" onClick={() => props.onLeftTabChange("keep")}>保留</button>
				<button className="flex-1 rounded-md px-3 py-2 text-sm" onClick={() => props.onLeftTabChange("discard")}>丢弃</button>
			</div>
			{props.leftTab === "discard" ? <div className="border-b border-app-line p-2"><button className="w-full rounded-md bg-rose-500/15 px-3 py-2 text-sm text-rose-300" disabled={props.discardFiles.length === 0} onClick={props.onDeleteClick}>立即删除</button></div> : null}
			<div className="min-h-0 flex-1 overflow-auto p-2">
				{items.map((file) => (
					<button key={file.id} className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-app-box" onClick={() => props.onRevealItem(file)}>
						<FileComponent.Thumb file={file} size={32} />
						<span className="truncate text-sm text-ink">{file.name}</span>
					</button>
				))}
			</div>
		</div>
	);
}
```

```tsx
export function OrganizeCenterPane(props: { files: File[]; layout: OrganizeCenterLayout; onLayoutChange: (layout: OrganizeCenterLayout) => void; presentation: Array<{ file: File; decision: OrganizeDecision | null; dimmed: boolean }>; selectedFileId: string | null; onSelectFile: (file: File) => void; onMarkKeep: (file: File) => void; onMarkDiscard: (file: File) => void; onClearDecision: (file: File) => void; }) {
	const selected = props.presentation.find((item) => item.file.id === props.selectedFileId)?.file ?? null;
	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="flex items-center gap-2 border-b border-app-line px-3 py-2">
				<button className="rounded-md bg-emerald-500/15 px-3 py-1.5 text-sm text-emerald-300" disabled={!selected} onClick={() => selected && props.onMarkKeep(selected)}>保留</button>
				<button className="rounded-md bg-rose-500/15 px-3 py-1.5 text-sm text-rose-300" disabled={!selected} onClick={() => selected && props.onMarkDiscard(selected)}>丢弃</button>
				<button className="rounded-md bg-app-box px-3 py-1.5 text-sm text-ink" disabled={!selected} onClick={() => selected && props.onClearDecision(selected)}>撤销</button>
			</div>
			<div className={clsx("min-h-0 flex-1 overflow-auto p-3", props.layout === "grid" ? "grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3" : "flex flex-col gap-2")}>
				{props.presentation.map((item) => (
					<button key={item.file.id} data-file-id={item.file.id} onClick={() => props.onSelectFile(item.file)} className={clsx("relative rounded-xl border border-app-line bg-app-box/60 p-3 text-left", item.dimmed && "opacity-50", item.file.id === props.selectedFileId && "ring-2 ring-accent")}>
						<FileComponent.Thumb file={item.file} size={props.layout === "grid" ? 96 : 48} />
						<div className="mt-2 truncate text-sm text-ink">{item.file.name}</div>
						{item.decision === "keep" ? <CheckCircle className="absolute bottom-2 right-2 text-emerald-400" size={20} weight="fill" /> : item.decision === "discard" ? <XCircle className="absolute bottom-2 right-2 text-rose-400" size={20} weight="fill" /> : null}
					</button>
				))}
			</div>
		</div>
	);
}
```

- [ ] **Verify**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts`

Run: `bun run --filter @sd/interface typecheck`

- [ ] **Commit**

```bash
git add packages/interface/src/routes/explorer/organize/organizeState.ts packages/interface/src/routes/explorer/organize/OrganizeView.tsx packages/interface/src/routes/explorer/organize/OrganizeLayout.tsx packages/interface/src/routes/explorer/organize/OrganizeLeftPane.tsx packages/interface/src/routes/explorer/organize/OrganizeCenterPane.tsx packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts
git commit -m "feat: add organize explorer panes"
```

## Task 5: Implement preview resolution and preview pane

**Files:**
- `packages/interface/src/routes/explorer/organize/organizePreview.ts`
- `packages/interface/src/routes/explorer/organize/OrganizePreviewPane.tsx`
- `packages/interface/src/routes/explorer/organize/OrganizeView.tsx`
- `packages/interface/src/routes/explorer/organize/__tests__/organizePreview.test.ts`

- [ ] **Write the failing preview test**

```ts
import { describe, expect, test } from "bun:test";
import { deriveDirectoryPreviewAvailability, toMediaSortBy, toPreviewListSortBy } from "../organizePreview";

describe("organize preview helpers", () => {
	test("coerces unsupported sorts safely", () => {
		expect(toMediaSortBy("type")).toBe("name");
		expect(toMediaSortBy("modified")).toBe("modified");
		expect(toPreviewListSortBy("datetaken")).toBe("modified");
	});

	test("disables missing media tabs and falls back to list when nothing exists", () => {
		expect(deriveDirectoryPreviewAvailability([])).toEqual({ renderedTabs: ["list"], enabledTabs: ["list"], defaultTab: "list", firstVideo: null, firstImage: null });
	});
});
```

- [ ] **Run the failure first**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizePreview.test.ts`

- [ ] **Implement preview helpers and pane**

```ts
export function toMediaSortBy(sortBy: DirectorySortBy | MediaSortBy): MediaSortBy {
	switch (sortBy) {
		case "created": return "created";
		case "datetaken": return "datetaken";
		case "modified": return "modified";
		case "name": return "name";
		case "size": return "size";
		case "type": return "name";
	}
}

export function toPreviewListSortBy(sortBy: DirectorySortBy | MediaSortBy): DirectorySortBy {
	switch (sortBy) {
		case "name":
		case "modified":
		case "size":
		case "type":
			return sortBy;
		case "created":
		case "datetaken":
			return "modified";
	}
}

export function deriveDirectoryPreviewAvailability(files: File[]) {
	const firstVideo = files.find((file) => getContentKind(file) === "video") ?? null;
	const firstImage = files.find((file) => getContentKind(file) === "image") ?? null;
	if (!firstVideo && !firstImage) {
		return { renderedTabs: ["list"] as OrganizePreviewTab[], enabledTabs: ["list"] as OrganizePreviewTab[], defaultTab: "list" as OrganizePreviewTab, firstVideo, firstImage };
	}
	return {
		renderedTabs: ["video", "image", "list"] as OrganizePreviewTab[],
		enabledTabs: [...(firstVideo ? (["video"] as const) : []), ...(firstImage ? (["image"] as const) : []), "list" as const],
		defaultTab: firstVideo ? ("video" as const) : ("image" as const),
		firstVideo,
		firstImage,
	};
}
```

```tsx
export function OrganizePreviewPane(props: { selectedFile: File | null }) {
	const platform = usePlatform();
	const { sortBy, viewSettings } = useExplorer();
	const [activeTab, setActiveTab] = useState<OrganizePreviewTab>("list");
	const selectedDirectory = props.selectedFile?.kind === "Directory" ? props.selectedFile : null;

	const mediaQuery = useNormalizedQuery({
		query: "files.media_listing",
		input: selectedDirectory?.sd_path ? { path: selectedDirectory.sd_path, include_descendants: true, media_types: null, limit: 10000, sort_by: toMediaSortBy(sortBy) } : null!,
		resourceType: "file",
		pathScope: selectedDirectory?.sd_path ?? undefined,
		includeDescendants: true,
		enabled: !!selectedDirectory,
	});

	const listQuery = useNormalizedQuery({
		query: "files.directory_listing",
		input: selectedDirectory?.sd_path ? { path: selectedDirectory.sd_path, limit: null, include_hidden: false, sort_by: toPreviewListSortBy(sortBy), folders_first: viewSettings.foldersFirst } : null!,
		resourceType: "file",
		pathScope: selectedDirectory?.sd_path ?? undefined,
		enabled: !!selectedDirectory,
	});

	const availability = useMemo(() => deriveDirectoryPreviewAvailability(((mediaQuery.data as { files: File[] } | undefined)?.files ?? [])), [mediaQuery.data]);

	useEffect(() => {
		setActiveTab(selectedDirectory ? availability.defaultTab : "list");
	}, [availability.defaultTab, selectedDirectory?.id]);

	if (!props.selectedFile) return <div className="flex h-full items-center justify-center text-sm text-ink-dull">请选择一个条目进行预览</div>;

	if (props.selectedFile.kind === "File") {
		const path = getPhysicalPath(props.selectedFile.sd_path);
		const src = path && platform.convertFileSrc ? platform.convertFileSrc(path) : null;
		if (!src) return <div className="flex h-full items-center justify-center text-sm text-ink-dull">Preview unavailable</div>;
		if (props.selectedFile.extension?.match(/^(mp4|mov|mkv|webm|avi)$/i)) return <VideoPlayer src={src} file={props.selectedFile} />;
		if (props.selectedFile.extension?.match(/^(png|jpe?g|gif|webp|bmp|svg)$/i)) return <img src={src} alt={props.selectedFile.name} className="h-full w-full object-contain bg-black" />;
	}

	const previewFile = activeTab === "video" ? availability.firstVideo : availability.firstImage;
	const previewSrc = previewFile ? platform.convertFileSrc?.(getPhysicalPath(previewFile.sd_path) ?? "") ?? null : null;
	const listFiles = ((listQuery.data as { files: File[] } | undefined)?.files ?? []);

	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="flex gap-1 border-b border-app-line p-2">
				{availability.renderedTabs.map((tab) => {
					const enabled = availability.enabledTabs.includes(tab);
					return <button key={tab} disabled={!enabled} onClick={() => enabled && setActiveTab(tab)} className="rounded-md px-3 py-2 text-sm disabled:opacity-40">{tab}</button>;
				})}
			</div>
			<div className="min-h-0 flex-1 overflow-auto">
				{activeTab === "list" ? <div className="flex flex-col gap-2 p-3">{listFiles.map((file) => <div key={file.id} className="flex items-center gap-3 rounded-lg border border-app-line p-2"><FileComponent.Thumb file={file} size={40} /><div className="truncate text-sm text-ink">{file.name}</div></div>)}</div> : previewFile && previewSrc ? activeTab === "video" ? <VideoPlayer src={previewSrc} file={previewFile} /> : <img src={previewSrc} alt={previewFile.name} className="h-full w-full object-contain bg-black" /> : null}
			</div>
		</div>
	);
}
```

- [ ] **Verify**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizePreview.test.ts`

Run: `bun run --filter @sd/interface typecheck`

- [ ] **Commit**

```bash
git add packages/interface/src/routes/explorer/organize/organizePreview.ts packages/interface/src/routes/explorer/organize/OrganizePreviewPane.tsx packages/interface/src/routes/explorer/organize/OrganizeView.tsx packages/interface/src/routes/explorer/organize/__tests__/organizePreview.test.ts
git commit -m "feat: add organize preview pane"
```

## Task 6: Add permanent delete from the Discard tab

**Files:**
- `packages/interface/src/routes/explorer/hooks/useDeleteFiles.ts`
- `packages/interface/src/routes/explorer/organize/organizeState.ts`
- `packages/interface/src/routes/explorer/organize/OrganizeView.tsx`
- `packages/interface/src/routes/explorer/organize/OrganizeLeftPane.tsx`
- `packages/interface/src/routes/explorer/organize/OrganizeDeleteDialog.tsx`
- `packages/interface/src/routes/explorer/organize/__tests__/organizeDelete.test.ts`

- [ ] **Write the failing delete-target test**

```ts
import { describe, expect, test } from "bun:test";
import { createEmptyOrganizeDirectoryState } from "../organizePersistence";
import { collectDiscardDeleteTargets, upsertOrganizeDecision } from "../organizeState";

describe("collectDiscardDeleteTargets", () => {
	test("returns only discard-marked direct children", () => {
		let state = createEmptyOrganizeDirectoryState("C:/Photos");
		const keepFile = makeFile({ id: "keep-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/keep.mp4" } } });
		const discardFile = makeFile({ id: "discard-1", sd_path: { Physical: { device_slug: "disk", path: "C:/Photos/discard.mp4" } } });
		state = upsertOrganizeDecision(state, keepFile, "keep");
		state = upsertOrganizeDecision(state, discardFile, "discard");
		expect(collectDiscardDeleteTargets([keepFile, discardFile], state).map((file) => file.id)).toEqual(["discard-1"]);
	});
});
```

- [ ] **Run the failure first**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeDelete.test.ts`

- [ ] **Implement delete target selection, extracted mutation core, and modal**

```ts
export function collectDiscardDeleteTargets(files: File[], state: OrganizeDirectoryState): File[] {
	return files.filter((file) => state.items[getOrganizeItemKey(file)]?.decision === "discard" && Boolean(file.sd_path));
}
```

```ts
export function useDeleteFilesMutation() {
	const mutation = useLibraryMutation("files.delete");
	const deleteFilesDirect = useCallback(async (files: File[], permanent: boolean) => {
		if (files.length === 0 || files.some((file) => !file.sd_path) || mutation.isPending) return false;
		try {
			await mutation.mutateAsync({ targets: { paths: files.map((file) => file.sd_path) }, permanent, recursive: true });
			return true;
		} catch (err) {
			console.error("Failed to delete:", err);
			return false;
		}
	}, [mutation]);
	return { deleteFilesDirect, isPending: mutation.isPending };
}
```

```tsx
export function OrganizeDeleteDialog(props: { open: boolean; onOpenChange: (open: boolean) => void; files: File[]; onDeleted: (files: File[]) => Promise<void> | void; }) {
	const { deleteFilesDirect, isPending } = useDeleteFilesMutation();
	const handleConfirm = useCallback(async () => {
		const didDelete = await deleteFilesDirect(props.files, true);
		if (!didDelete) return;
		await props.onDeleted(props.files);
		props.onOpenChange(false);
	}, [deleteFilesDirect, props]);

	return (
		<Dialog.Root open={props.open} onOpenChange={props.onOpenChange}>
			<Dialog.Portal>
				<Dialog.Overlay className="fixed inset-0 bg-black/60" />
				<Dialog.Content className="fixed left-1/2 top-1/2 w-[420px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-app-line bg-app p-5" onInteractOutside={(event) => { event.preventDefault(); props.onOpenChange(false); }} onEscapeKeyDown={() => props.onOpenChange(false)} onKeyDown={(event) => { if (event.key === "Enter" && !isPending) { event.preventDefault(); void handleConfirm(); } }}>
					<h2 className="text-lg font-semibold text-ink">永久删除丢弃项？</h2>
					<p className="mt-2 text-sm text-ink-dull">这会永久删除当前目录下所有已标记为丢弃的直接子项，并使用 `files.delete` 的 `permanent: true` 与 `recursive: true`。</p>
					<div className="mt-4 flex justify-end gap-2">
						<button className="rounded-md bg-app-box px-3 py-2 text-sm text-ink" onClick={() => props.onOpenChange(false)}>取消</button>
						<button className="rounded-md bg-rose-600 px-3 py-2 text-sm text-white" disabled={isPending} onClick={() => void handleConfirm()}>立即删除</button>
					</div>
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
```

```tsx
const [deleteOpen, setDeleteOpen] = useState(false);
const deleteTargets = organize.state ? collectDiscardDeleteTargets(files, organize.state) : [];

<OrganizeLeftPane
	leftTab={leftTab}
	onLeftTabChange={setLeftTab}
	keepFiles={organize.keepFiles}
	discardFiles={organize.discardFiles}
	onRevealItem={(file) => selectFile(file, files, false, false)}
	onDeleteClick={() => setDeleteOpen(true)}
/>
<OrganizeDeleteDialog
	open={deleteOpen}
	onOpenChange={setDeleteOpen}
	files={deleteTargets}
	onDeleted={organize.removeDeleted}
/>
```

- [ ] **Verify**

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeDelete.test.ts`

Run: `bun run --filter @sd/interface typecheck`

- [ ] **Commit**

```bash
git add packages/interface/src/routes/explorer/hooks/useDeleteFiles.ts packages/interface/src/routes/explorer/organize/organizeState.ts packages/interface/src/routes/explorer/organize/OrganizeView.tsx packages/interface/src/routes/explorer/organize/OrganizeLeftPane.tsx packages/interface/src/routes/explorer/organize/OrganizeDeleteDialog.tsx packages/interface/src/routes/explorer/organize/__tests__/organizeDelete.test.ts
git commit -m "feat: add organize delete workflow"
```

## Final Verification

Agent execution for this plan stops at automated logic/code verification. Manual UI verification stays user-owned, and any UI debugging discovered there stays outside the current execution scope.

- [ ] **Automated checks**

Run: `cargo test --manifest-path apps/tauri/src-tauri/Cargo.toml organize::tests`

Run: `bun test packages/interface/src/routes/explorer/organize/__tests__/organizeState.test.ts packages/interface/src/routes/explorer/organize/__tests__/organizeAvailability.test.ts packages/interface/src/routes/explorer/organize/__tests__/organizePreview.test.ts packages/interface/src/routes/explorer/organize/__tests__/organizeDelete.test.ts`

Run: `bun run --filter @sd/interface typecheck`

- [ ] **User-owned manual verification in Tauri (out of current execution scope)**

From `apps/tauri/`, run: `bun run tauri:dev`

User checks:
- Browse to a physical directory and switch to `Organize`.
- Mark one item `Keep` and one item `Discard`; confirm the JSON file appears under `.spacedrive/organize/v1/` only after the first decision.
- Leave and re-enter the directory; confirm both decisions restore.
- Confirm decided items stay visible in the center pane, are dimmed, and show green/red badges.
- Confirm the Keep/Discard side tabs show only their matching items.
- Select a video file; confirm it auto-plays muted and preserves the existing `sd-video-volume` and `sd-video-muted` behavior from `VideoPlayer`.
- Select a directory with video + image descendants; confirm preview tab priority is `video > image > list`.
- Select a directory with only one media type; confirm the missing media tab is disabled with tooltip and `list` remains available.
- Select a directory with no media descendants; confirm only the `list` tab is rendered.
- Open `立即删除` from the Discard tab; confirm `Enter` deletes, `Esc` closes, outside click closes, and cancel keeps organize state untouched.
- Confirm successful permanent delete removes the entries from the left pane, center dimming/badges, and persisted JSON.

## Suggested execution mode

- **Recommended:** `superpowers:subagent-driven-development`
  - Best fit because the work splits cleanly into persistence, Explorer plumbing, preview, and delete-flow milestones.
- **Alternative:** `superpowers:executing-plans`
  - Good if a single worker will execute the plan linearly without parallelization.
