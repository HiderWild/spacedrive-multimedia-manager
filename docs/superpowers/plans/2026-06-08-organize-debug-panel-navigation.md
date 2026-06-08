# Floating Debug Panel + Parent Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a draggable floating debug panel to organize view and a parent directory navigation button to the TopBar.

**Architecture:** Create a reusable `FloatingDebugPanel` component with drag-and-drop logic. Refactor `OrganizeView` to overlay the debug panel on top of preview content instead of replacing it. Add a parent navigation button to the existing navigation button group in `ExplorerView`. Implement cross-platform parent path resolution in explorer context.

**Tech Stack:** React 19, TypeScript, Tailwind CSS, @phosphor-icons/react, framer-motion (for animations)

---

## File Structure

**New files:**
- `packages/interface/src/components/FloatingDebugPanel.tsx` - Reusable draggable floating panel component

**Modified files:**
- `packages/interface/src/routes/explorer/organize/OrganizeView.tsx:149-180` - Change debug panel to floating overlay
- `packages/interface/src/routes/explorer/ExplorerView.tsx:1-10,234-245` - Add ArrowUp import and button
- `packages/interface/src/routes/explorer/context.tsx:405-414` - Add navigateToParent method

---

## Task 1: Create FloatingDebugPanel Component

**Files:**
- Create: `packages/interface/src/components/FloatingDebugPanel.tsx`

This component provides a draggable, collapsible floating panel that can overlay content. It's a pure UI component with no business logic.

- [ ] **Step 1: Create component file with basic structure**

Create `packages/interface/src/components/FloatingDebugPanel.tsx`:

```tsx
import {Minus, X} from '@phosphor-icons/react';
import clsx from 'clsx';
import {useCallback, useEffect, useRef, useState} from 'react';

interface FloatingDebugPanelProps {
	children: React.ReactNode;
	initialPosition?: {top: number; right: number};
	onClose: () => void;
	title?: string;
}

export function FloatingDebugPanel({
	children,
	initialPosition = {top: 16, right: 16},
	onClose,
	title = 'Debug Panel'
}: FloatingDebugPanelProps) {
	const panelRef = useRef<HTMLDivElement>(null);
	const containerRef = useRef<HTMLDivElement>(null);
	const [position, setPosition] = useState(initialPosition);
	const [isDragging, setIsDragging] = useState(false);
	const [dragOffset, setDragOffset] = useState({x: 0, y: 0});

	// Load collapsed state from localStorage
	const [isCollapsed, setIsCollapsed] = useState(() => {
		const stored = localStorage.getItem('organize-debug-panel-collapsed');
		return stored === 'true';
	});

	// Persist collapsed state to localStorage
	useEffect(() => {
		localStorage.setItem('organize-debug-panel-collapsed', String(isCollapsed));
	}, [isCollapsed]);

	const handleMouseDown = (e: React.MouseEvent) => {
		if ((e.target as HTMLElement).closest('button')) {
			return;
		}

		const rect = panelRef.current?.getBoundingClientRect();
		if (!rect) return;

		setIsDragging(true);
		setDragOffset({
			x: e.clientX - rect.left,
			y: e.clientY - rect.top
		});
	};

	const handleMouseMove = useCallback(
		(e: MouseEvent) => {
			if (!isDragging || !panelRef.current) return;

			const panel = panelRef.current;
			const container = panel.parentElement;
			if (!container) return;

			const containerRect = container.getBoundingClientRect();
			const panelRect = panel.getBoundingClientRect();

			let newLeft = e.clientX - containerRect.left - dragOffset.x;
			let newTop = e.clientY - containerRect.top - dragOffset.y;

			// Clamp within container bounds
			newLeft = Math.max(0, Math.min(newLeft, containerRect.width - panelRect.width));
			newTop = Math.max(0, Math.min(newTop, containerRect.height - panelRect.height));

			// Convert left position to right-based positioning
			const newRight = containerRect.width - newLeft - panelRect.width;

			setPosition({top: newTop, right: newRight});
		},
		[isDragging, dragOffset]
	);

	const handleMouseUp = useCallback(() => {
		setIsDragging(false);
	}, []);

	useEffect(() => {
		if (isDragging) {
			window.addEventListener('mousemove', handleMouseMove);
			window.addEventListener('mouseup', handleMouseUp);
			return () => {
				window.removeEventListener('mousemove', handleMouseMove);
				window.removeEventListener('mouseup', handleMouseUp);
			};
		}
	}, [isDragging, handleMouseMove, handleMouseUp]);

	// Handle ESC key to close
	useEffect(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.key === 'Escape') {
				onClose();
			}
		};

		window.addEventListener('keydown', handleKeyDown);
		return () => window.removeEventListener('keydown', handleKeyDown);
	}, [onClose]);

	return (
		<div
			ref={panelRef}
			className={clsx(
				'absolute z-50',
				'bg-app-box/95 backdrop-blur-md',
				'border border-app-line rounded-lg',
				'shadow-lg',
				'overflow-hidden',
				'w-[300px]',
				isDragging && 'cursor-move'
			)}
			style={{
				top: `${position.top}px`,
				right: `${position.right}px`,
				minHeight: isCollapsed ? '40px' : '100px',
				maxHeight: isCollapsed ? '40px' : '400px'
			}}
		>
			{/* Title bar (draggable) */}
			<div
				className={clsx(
					'flex items-center justify-between gap-2 px-3 py-2',
					'border-b border-app-line',
					'cursor-move select-none',
					'bg-app-box/50'
				)}
				onMouseDown={handleMouseDown}
				aria-label="Drag to move panel"
			>
				<span className="text-ink text-xs font-semibold">{title}</span>
				<div className="flex items-center gap-1">
					<button
						type="button"
						onClick={() => setIsCollapsed(!isCollapsed)}
						className="text-ink-dull hover:text-ink hover:bg-app-hover rounded p-1 transition-colors"
						aria-label={isCollapsed ? 'Expand panel' : 'Collapse panel'}
						title={isCollapsed ? 'Expand' : 'Collapse'}
					>
						<Minus size={12} weight="bold" />
					</button>
					<button
						type="button"
						onClick={onClose}
						className="text-ink-dull hover:text-ink hover:bg-app-hover rounded p-1 transition-colors"
						aria-label="Close debug panel"
						title="Close"
					>
						<X size={12} weight="bold" />
					</button>
				</div>
			</div>

			{/* Content area (scrollable when not collapsed) */}
			{!isCollapsed && (
				<div className="overflow-auto p-2" style={{maxHeight: '360px'}}>
					{children}
				</div>
			)}
		</div>
	);
}
```

- [ ] **Step 2: Verify component compiles**

Run: `npm run typecheck`
Expected: No type errors related to FloatingDebugPanel

- [ ] **Step 3: Commit**

```bash
git add packages/interface/src/components/FloatingDebugPanel.tsx
git commit -m "feat(interface): add FloatingDebugPanel component

- Draggable floating panel with title bar
- Collapsible state persisted to localStorage
- ESC key to close
- Clamped dragging within parent bounds
- Semantic color classes and backdrop blur

Co-Authored-By: Claude Sonnet 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Integrate FloatingDebugPanel into OrganizeView

**Files:**
- Modify: `packages/interface/src/routes/explorer/organize/OrganizeView.tsx:1-20,149-180`

Change the debug panel from a mutually-exclusive toggle to a floating overlay that appears on top of the preview content.

- [ ] **Step 1: Add FloatingDebugPanel import**

In `packages/interface/src/routes/explorer/organize/OrganizeView.tsx`, add to imports at the top:

```tsx
import {FloatingDebugPanel} from '../../../components/FloatingDebugPanel';
```

- [ ] **Step 2: Refactor right panel layout**

Replace lines 149-180 (the entire right panel section) with:

```tsx
<div className="relative flex h-full min-h-0 flex-col">
	{/* Debug toggle button */}
	<div className="flex justify-end border-b border-app-line p-2">
		<button
			type="button"
			onClick={() => setShowDebug(!showDebug)}
			className="rounded-md px-2 py-1 text-xs text-ink-dull hover:bg-app-hover hover:text-ink"
		>
			{showDebug ? 'Hide' : 'Show'} Debug
		</button>
	</div>

	{/* Preview content - always visible */}
	<div className="min-h-0 flex-1">
		{selectedFile && previewState.defaultTabId ? (
			<OrganizePreviewContent
				selectedFile={selectedFile}
				activeTab={previewState.defaultTabId}
				context={{sortBy: 'name', foldersFirst: false}}
			/>
		) : (
			<div className="flex h-full items-center justify-center p-4 text-sm text-ink-dull">
				{t('organize.selectFileToPreview')}
			</div>
		)}
	</div>

	{/* Floating debug panel - overlays on top when enabled */}
	{showDebug && selectedFile && (
		<FloatingDebugPanel
			title="Preview State"
			onClose={() => setShowDebug(false)}
		>
			<OrganizeDebugPanel
				title="Preview State"
				payload={{
					selectedFile: selectedFile.name,
					previewState
				}}
			/>
		</FloatingDebugPanel>
	)}
</div>
```

- [ ] **Step 3: Verify component compiles**

Run: `npm run typecheck`
Expected: No type errors in OrganizeView

- [ ] **Step 4: Test in browser**

1. Start dev server: `npm run dev`
2. Navigate to organize view
3. Select a file
4. Click "Show Debug" - floating panel should appear over preview
5. Preview content should remain visible behind panel
6. Drag panel by title bar - should move smoothly
7. Click minimize button - panel should collapse to title bar only
8. Click again - panel should expand
9. Refresh page - collapsed state should persist
10. Click X button - panel should close
11. Press ESC - panel should close

Expected: All interactions work smoothly, no console errors

- [ ] **Step 5: Commit**

```bash
git add packages/interface/src/routes/explorer/organize/OrganizeView.tsx
git commit -m "feat(interface): use floating debug panel in organize view

- Preview content now always visible
- Debug panel overlays on top when enabled
- No more toggle between debug and preview
- Maintains existing Show/Hide Debug button

Co-Authored-By: Claude Sonnet 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Add Parent Navigation Button to ExplorerView

**Files:**
- Modify: `packages/interface/src/routes/explorer/ExplorerView.tsx:1-10,234-245`
- Modify: `packages/interface/src/routes/explorer/context.tsx:1-50,405-468`

Add an "up" arrow button to the navigation group for parent directory navigation.

- [ ] **Step 1: Add ArrowUp icon import**

In `packages/interface/src/routes/explorer/ExplorerView.tsx`, modify the import from `@phosphor-icons/react` (around line 1-10):

```tsx
import {
	ArrowLeft,
	ArrowRight,
	ArrowUp,  // Add this
	Columns,
	FilmStrip,
	Info,
	MagicWand,
	SidebarSimple,
	Tag as TagIcon
} from '@phosphor-icons/react';
```

- [ ] **Step 2: Add navigateToParent to explorer context destructuring**

In `packages/interface/src/routes/explorer/ExplorerView.tsx`, add to the destructuring from `useExplorer()` (around line 33-63):

```tsx
const {
	sidebarVisible,
	setSidebarVisible,
	inspectorVisible,
	setInspectorVisible,
	tagModeActive,
	setTagModeActive,
	viewMode,
	setViewMode,
	sortBy,
	setSortBy,
	viewSettings,
	setViewSettings,
	goBack,
	goForward,
	canGoBack,
	canGoForward,
	navigateToParent,  // Add this
	currentPath,
	currentView,
	currentTarget,
	navigateToPath,
	devices,
	quickPreviewFileId,
	mode,
	enterSearchMode,
	exitSearchMode,
	currentFiles,
	columnStack
} = useExplorer();
```

- [ ] **Step 3: Add ArrowUp button to navigation group**

In `packages/interface/src/routes/explorer/ExplorerView.tsx`, modify the navigation TopBarItem (lines 229-246):

```tsx
<TopBarItem
	id="navigation"
	label={t('topBar.navigation')}
	priority="high"
>
	<CircleButtonGroup>
		<CircleButton
			icon={ArrowLeft}
			onClick={goBack}
			disabled={!canGoBack}
		/>
		<CircleButton
			icon={ArrowRight}
			onClick={goForward}
			disabled={!canGoForward}
		/>
		<CircleButton
			icon={ArrowUp}
			onClick={navigateToParent}
			disabled={false}
		/>
	</CircleButtonGroup>
</TopBarItem>
```

- [ ] **Step 4: Verify component compiles**

Run: `npm run typecheck`
Expected: Type error about `navigateToParent` not existing on ExplorerContextValue (this is expected, we'll fix it in next task)

- [ ] **Step 5: Commit (will compile after next task)**

```bash
git add packages/interface/src/routes/explorer/ExplorerView.tsx
git commit -m "feat(interface): add parent navigation button to TopBar

- Add ArrowUp icon to navigation button group
- Button always enabled (no disabled state)
- Positioned after back/forward buttons

Co-Authored-By: Claude Sonnet 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Implement Parent Path Resolution in Explorer Context

**Files:**
- Modify: `packages/interface/src/routes/explorer/context.tsx:1-50,405-468`

Add the `navigateToParent` method to the explorer context with cross-platform path resolution.

- [ ] **Step 1: Add helper function for parent path resolution**

In `packages/interface/src/routes/explorer/context.tsx`, add this helper function before the `ExplorerProvider` component (around line 500, before the component definition):

```tsx
/**
 * Resolve parent directory path, handling Windows and Unix paths.
 * Returns null if already at root or no parent exists.
 */
function getParentPath(path: string): string | null {
	if (!path) return null;

	// Detect path separator (Windows uses \, Unix uses /)
	const separator = path.includes('\\') ? '\\' : '/';
	const parts = path.split(separator).filter(Boolean);

	// No parts or single part means we're at root
	if (parts.length === 0) return null;
	if (parts.length === 1) {
		// Windows drive root (e.g., "C:") or Unix root
		return null;
	}

	// Remove last part to get parent
	parts.pop();

	// Reconstruct path
	if (separator === '\\') {
		// Windows path
		if (parts[0].endsWith(':')) {
			// Preserve drive letter format: ["C:", "Users"] -> "C:\Users"
			return parts.join(separator);
		}
		// UNC path: ["", "", "server", "share"] -> "\\server\share"
		return parts.join(separator);
	} else {
		// Unix path: always starts with /
		return separator + parts.join(separator);
	}
}
```

- [ ] **Step 2: Add navigateToParent method to ExplorerProvider**

In `packages/interface/src/routes/explorer/context.tsx`, find the `ExplorerProvider` component and add the `navigateToParent` function. Add it after the `goForward` function (around line 650-700):

```tsx
const navigateToParent = useCallback(() => {
	if (!currentTarget || currentTarget.type !== 'path') {
		return;
	}

	const pathObj = currentTarget.path;
	let currentPathString: string | null = null;

	if ('Physical' in pathObj && pathObj.Physical) {
		currentPathString = pathObj.Physical.path;
	} else if ('Virtual' in pathObj && pathObj.Virtual) {
		currentPathString = pathObj.Virtual;
	}

	if (!currentPathString) return;

	const parentPath = getParentPath(currentPathString);
	if (!parentPath) {
		// Already at root, silent no-op
		return;
	}

	// Navigate to parent using the same format as current path
	if ('Physical' in pathObj && pathObj.Physical) {
		navigateToPath({
			Physical: {
				device_slug: pathObj.Physical.device_slug,
				path: parentPath
			}
		});
	} else if ('Virtual' in pathObj) {
		navigateToPath({Virtual: parentPath});
	}
}, [currentTarget, navigateToPath]);
```

- [ ] **Step 3: Add navigateToParent to context value**

In `packages/interface/src/routes/explorer/context.tsx`, find the return statement of `ExplorerProvider` where the context value is defined (around line 800-900). Add `navigateToParent` to the value object:

```tsx
return {
	// ... existing properties
	goBack,
	goForward,
	canGoBack,
	canGoForward,
	navigateToParent,  // Add this line
	// ... rest of properties
};
```

- [ ] **Step 4: Add navigateToParent to ExplorerContextValue interface**

In `packages/interface/src/routes/explorer/context.tsx`, find the `ExplorerContextValue` interface (around line 380-468) and add the method signature after the navigation methods:

```tsx
export interface ExplorerContextValue {
	// ... existing properties
	
	navigateToPath: (path: SdPath) => void;
	navigateToView: (
		view: string,
		id?: string,
		params?: Record<string, string>,
	) => void;
	goBack: () => void;
	goForward: () => void;
	canGoBack: boolean;
	canGoForward: boolean;
	navigateToParent: () => void;  // Add this line

	// ... rest of properties
}
```

- [ ] **Step 5: Verify component compiles**

Run: `npm run typecheck`
Expected: No type errors

- [ ] **Step 6: Test parent navigation**

1. Start dev server: `npm run dev`
2. Navigate to a nested directory (e.g., `/Users/Tom/Documents/Projects`)
3. Click the up arrow button
4. Expected: Navigate to `/Users/Tom/Documents`
5. Click up arrow again
6. Expected: Navigate to `/Users/Tom`
7. Continue until root
8. Click up arrow at root
9. Expected: No navigation, no error, stays at root
10. Test from organize view - should work the same
11. Test from search mode - should work the same

Expected: All tests pass, no console errors

- [ ] **Step 7: Commit**

```bash
git add packages/interface/src/routes/explorer/context.tsx
git commit -m "feat(interface): implement parent directory navigation

- Add getParentPath helper for cross-platform path resolution
- Handle Windows (backslash) and Unix (forward slash) paths
- Silent no-op at root directory (no error)
- Works with Physical and Virtual paths
- Export navigateToParent in explorer context

Co-Authored-By: Claude Sonnet 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Manual Testing and Documentation

**Files:**
- None (testing only)

Comprehensive manual testing to verify all functionality works correctly.

- [ ] **Step 1: Test floating debug panel interactions**

Test checklist:
1. Navigate to organize view
2. Select a file
3. Click "Show Debug" button
   - Panel appears in top-right corner
   - Preview content visible behind panel
4. Drag panel by title bar
   - Moves smoothly
   - Cursor changes to move cursor
   - Stays within container bounds
5. Try to drag by clicking content area
   - Should not drag (only title bar is draggable)
6. Click minimize button
   - Panel collapses to title bar only
   - Content hidden
7. Click minimize button again
   - Panel expands
   - Content visible
8. Refresh page
   - Collapsed state persists (same as before refresh)
9. Click X button
   - Panel closes
10. Click "Show Debug" again
    - Panel reappears in default position
11. Press ESC key
    - Panel closes
12. Select different file
    - Debug content updates
13. Test with long JSON content
    - Content area scrolls
    - Max height respected (400px)
14. Test on small window
    - Panel stays within bounds when dragged

Expected: All tests pass, smooth interactions, no errors

- [ ] **Step 2: Test parent navigation button**

Test checklist:
1. Navigate to nested directory (at least 3 levels deep)
2. Verify up arrow button visible in navigation group
3. Click up arrow
   - Navigates to parent directory
   - Path bar updates correctly
4. Click up arrow repeatedly
   - Each click goes up one level
   - Stops at root
5. At root, click up arrow
   - No navigation occurs
   - No error message
   - No console error
6. Navigate into directory from root
7. Click up arrow
   - Returns to root
8. Test in organize mode
   - Up arrow works
   - Organize state preserved
9. Test in search mode
   - Up arrow works
   - Search cleared (expected behavior)
10. Test on Windows paths (if available)
    - `C:\Users\Tom\Documents` → `C:\Users\Tom`
    - Works correctly with backslashes
11. Test on Unix paths
    - `/home/user/documents` → `/home/user`
    - Works correctly with forward slashes
12. Test button styling
    - Hover effect works
    - No active state (correct for navigation)
    - Same visual style as back/forward buttons

Expected: All tests pass, consistent behavior across modes

- [ ] **Step 3: Test edge cases**

1. Very long file names in debug panel
   - Text wraps or truncates appropriately
2. Small browser window
   - Debug panel remains functional
   - Doesn't overflow viewport
3. Multiple rapid clicks on up button
   - No race conditions
   - Navigation smooth
4. Network paths (if available)
   - `\\server\share\folder` → `\\server\share`
   - Works correctly
5. Rapid show/hide of debug panel
   - No visual glitches
   - Position resets correctly

Expected: No crashes, graceful handling

- [ ] **Step 4: Cross-browser testing (if applicable)**

Test on:
- Chrome/Edge
- Firefox  
- Safari (if on macOS)

Expected: Consistent behavior across browsers

- [ ] **Step 5: Document completion**

All functionality complete and tested:
- ✅ Floating debug panel component created
- ✅ Integrated into OrganizeView
- ✅ Parent navigation button added
- ✅ Parent path resolution implemented
- ✅ Cross-platform support verified
- ✅ Edge cases handled
- ✅ Manual testing passed

---

## Success Criteria

All tasks completed when:
1. ✅ FloatingDebugPanel component exists and compiles
2. ✅ OrganizeView uses floating overlay instead of toggle
3. ✅ Debug panel can be dragged, collapsed, and closed
4. ✅ Collapsed state persists across page refreshes
5. ✅ ArrowUp button appears in navigation group
6. ✅ Parent navigation works on Windows and Unix paths
7. ✅ Silent no-op at root directory (no errors)
8. ✅ All manual tests pass
9. ✅ No TypeScript errors
10. ✅ No console errors during normal usage

---

## Notes

- The FloatingDebugPanel is a pure UI component - it can be reused elsewhere in the app if needed
- The collapsed state uses localStorage key `organize-debug-panel-collapsed`
- The panel position resets to default on each mount (not persisted)
- Parent navigation is always enabled - it's a silent no-op at root rather than disabled
- Path resolution handles Windows (backslash), Unix (forward slash), and UNC network paths
- The implementation follows CLAUDE.md guidelines: semantic colors, no style tags, React 19 patterns
