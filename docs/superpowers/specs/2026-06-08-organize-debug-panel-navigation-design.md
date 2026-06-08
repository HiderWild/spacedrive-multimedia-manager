---
name: organize-debug-panel-navigation
description: Floating debug panel for organize view and parent directory navigation button
metadata:
  type: design
  date: 2026-06-08
  status: approved
---

# Organize View: Floating Debug Panel + Parent Navigation

## Overview

This design addresses two UX improvements for the organize view and explorer navigation:

1. **Floating Debug Panel**: Transform the debug information display from a mutually-exclusive toggle into a floating overlay that allows simultaneous viewing of preview content and debug information
2. **Parent Directory Navigation**: Add an "up" arrow button to the TopBar navigation group for quick parent directory navigation

## Problem Statement

### Current Issues

**Debug Panel:**
- Currently, "Show Debug" replaces the preview content with debug information
- This creates two separate "preview frames" - one for content, one for debug
- Users cannot see both preview and debug info simultaneously
- Debugging workflow requires constant toggling between states

**Navigation:**
- Users can go back/forward but lack a direct "up one level" button
- Common file manager pattern (⬆️) is missing
- Forces users to use path bar or back button for parent navigation

## Goals

1. Enable simultaneous viewing of preview content and debug information
2. Provide a flexible, non-intrusive debug panel that doesn't disrupt the main workflow
3. Add intuitive parent directory navigation following standard file manager conventions
4. Maintain consistency with existing UI patterns (semantic colors, rounded V2 style)

## Design

### 1. Floating Debug Panel

#### Visual Design

**Position:**
- Default: Top-right corner of preview area
- Offset: 16px from top and right edges
- Z-index: 50 (above preview content, below modals)

**Styling:**
```tsx
className={clsx(
  "absolute z-50",
  "bg-app-box/95 backdrop-blur-md",
  "border border-app-line rounded-lg",
  "shadow-lg",
  "overflow-hidden"
)}
```

**Dimensions:**
- Width: 300px (fixed)
- Height: Auto (content-dependent, max 400px)
- Min-height: 100px (when collapsed)
- Scrollable content area when exceeding max height

**Structure:**
```
┌──────────────────────────────────┐
│ Preview State         [−] [×]    │  ← Title bar (draggable)
├──────────────────────────────────┤
│                                  │
│  {                               │
│    "selectedFile": "...",        │
│    "previewState": {...}         │
│  }                               │
│                                  │
│  (scrollable)                    │
│                                  │
└──────────────────────────────────┘
```

#### Interaction Design

**Dragging:**
- Click and hold title bar to drag
- Cursor changes to `cursor-move` on title bar
- Position clamped within parent container bounds
- Smooth dragging with no jank

**Collapsing:**
- Click minimize button [−] to collapse
- Collapsed state shows only title bar
- Click again to expand
- State persisted in localStorage: `organize-debug-panel-collapsed`

**Closing:**
- Click [×] button calls `onClose` callback
- Main "Show/Hide Debug" button toggles panel visibility
- Panel smoothly fades out on close

**State Management:**
- Position stored in component state (not persisted)
- Collapsed state persisted in localStorage
- Reset to default position on each mount

#### Implementation

**New Component: `FloatingDebugPanel.tsx`**

```tsx
interface FloatingDebugPanelProps {
  children: React.ReactNode;
  initialPosition?: { top: number; right: number };
  onClose: () => void;
  title?: string;
}

export function FloatingDebugPanel({
  children,
  initialPosition = { top: 16, right: 16 },
  onClose,
  title = 'Debug Panel'
}: FloatingDebugPanelProps) {
  // State: position, isDragging, isCollapsed
  // Handlers: handleMouseDown, handleMouseMove, handleMouseUp
  // Render: draggable panel with controls
}
```

**Integration in `OrganizeView.tsx`:**

Current structure (lines 149-180):
```tsx
<div className="flex h-full min-h-0 flex-col">
  {showDebug ? (
    <OrganizeDebugPanel />
  ) : (
    <OrganizePreviewContent />
  )}
</div>
```

New structure:
```tsx
<div className="relative flex h-full min-h-0 flex-col">
  {/* Preview always visible */}
  <OrganizePreviewContent
    selectedFile={selectedFile}
    activeTab={previewState.defaultTabId}
    context={{sortBy: 'name', foldersFirst: false}}
  />
  
  {/* Debug panel floats on top when enabled */}
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

### 2. Parent Directory Navigation Button

#### Visual Design

**Location:**
- TopBar left section, in navigation button group
- After back/forward buttons
- Part of existing `CircleButtonGroup`

**Icon:**
- `ArrowUp` from `@phosphor-icons/react`
- Weight: `bold` (consistent with other nav buttons)
- Size: Default (16px)

**State:**
- Always enabled (no disabled state)
- Hover effect consistent with other CircleButtons
- Active state: none (navigation buttons don't have active state)

#### Interaction Design

**Behavior:**
- Click navigates to parent directory of current path
- If already at root, click has no effect (no error, no feedback)
- Works in all explorer modes (browse, search, organize)
- No special handling for organize mode (uses same navigation)

**Path Logic:**
```tsx
function getParentPath(path: string): string | null {
  if (!path) return null;
  
  // Handle different path formats
  // Windows: "C:\Users\Tom\Documents" → "C:\Users\Tom"
  // Unix: "/home/user/documents" → "/home/user"
  // Root: "/" → null (or same path)
  
  const separator = path.includes('\\') ? '\\' : '/';
  const parts = path.split(separator).filter(Boolean);
  
  if (parts.length === 0) return null;
  if (parts.length === 1) {
    // Windows drive root: "C:" → null
    // Unix root: "/" → null
    return null;
  }
  
  parts.pop();
  
  // Reconstruct path
  if (separator === '\\' && parts[0].endsWith(':')) {
    // Windows: preserve drive letter format
    return parts.join(separator);
  }
  
  return separator + parts.join(separator);
}

function navigateToParent() {
  const parent = getParentPath(currentPath);
  if (parent) {
    navigateToPath(parent);
  }
  // Silent no-op at root
}
```

#### Implementation

**Modify `ExplorerView.tsx` (lines 229-246):**

Current:
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
  </CircleButtonGroup>
</TopBarItem>
```

New:
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

**Add to explorer context (`context.tsx`):**

```tsx
const navigateToParent = useCallback(() => {
  const parent = getParentPath(currentPath);
  if (parent) {
    navigateToPath(parent);
  }
}, [currentPath, navigateToPath]);

// Export in context
return {
  // ... existing exports
  navigateToParent,
};
```

## Component Architecture

### New Components

**`FloatingDebugPanel.tsx`**
- Reusable floating panel container
- Drag-and-drop logic
- Collapse/expand state
- Close callback
- Position management
- No business logic (pure UI component)

### Modified Components

**`OrganizeView.tsx`**
- Remove toggle between debug/preview
- Make preview always visible
- Add floating debug panel overlay
- Keep existing "Show/Hide Debug" button

**`ExplorerView.tsx`**
- Add ArrowUp button to navigation group
- Import ArrowUp icon
- Add navigateToParent handler

**`context.tsx` (if needed)**
- Add `navigateToParent()` method
- Implement parent path resolution
- Export in useExplorer hook

## Technical Specifications

### Drag Logic

```tsx
const [position, setPosition] = useState(initialPosition);
const [isDragging, setIsDragging] = useState(false);
const [dragOffset, setDragOffset] = useState({ x: 0, y: 0 });

const handleMouseDown = (e: React.MouseEvent) => {
  const rect = panelRef.current?.getBoundingClientRect();
  if (!rect) return;
  
  setIsDragging(true);
  setDragOffset({
    x: e.clientX - rect.left,
    y: e.clientY - rect.top
  });
};

const handleMouseMove = useCallback((e: MouseEvent) => {
  if (!isDragging) return;
  
  const container = containerRef.current?.getBoundingClientRect();
  if (!container) return;
  
  let newX = e.clientX - container.left - dragOffset.x;
  let newY = e.clientY - container.top - dragOffset.y;
  
  // Clamp within bounds
  newX = Math.max(0, Math.min(newX, container.width - panelWidth));
  newY = Math.max(0, Math.min(newY, container.height - panelHeight));
  
  setPosition({ top: newY, right: container.width - newX - panelWidth });
}, [isDragging, dragOffset]);

useEffect(() => {
  if (isDragging) {
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', () => setIsDragging(false));
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', () => setIsDragging(false));
    };
  }
}, [isDragging, handleMouseMove]);
```

### Collapse State Persistence

```tsx
const [isCollapsed, setIsCollapsed] = useState(() => {
  const stored = localStorage.getItem('organize-debug-panel-collapsed');
  return stored === 'true';
});

useEffect(() => {
  localStorage.setItem('organize-debug-panel-collapsed', String(isCollapsed));
}, [isCollapsed]);
```

### Path Resolution

**Cross-platform considerations:**
- Windows: backslash separators, drive letters
- Unix/macOS: forward slash separators
- Handle edge cases: root directory, single-level paths
- No error throwing - silent no-op at boundaries

**Why:** This is a navigation utility, not a critical operation. Users expect "up" to work when possible and do nothing at root, not throw errors.

## Testing Strategy

### Manual Testing

**Floating Debug Panel:**
- [ ] Panel appears in correct default position
- [ ] Panel can be dragged to any position
- [ ] Panel stays within container bounds
- [ ] Panel collapse/expand works
- [ ] Collapsed state persists across toggles
- [ ] Close button hides panel
- [ ] "Show Debug" button brings it back
- [ ] Preview content visible behind panel
- [ ] Panel scrolls when content exceeds max height
- [ ] Backdrop blur effect visible

**Parent Navigation:**
- [ ] Button appears in navigation group
- [ ] ArrowUp icon renders correctly
- [ ] Click navigates to parent directory
- [ ] Works from nested directories
- [ ] No-op at root (no crash, no error)
- [ ] Works in organize mode
- [ ] Works in search mode
- [ ] Path bar updates correctly after navigation

### Edge Cases

- Very long debug JSON (test scrolling)
- Small preview area (test panel doesn't overflow)
- Root directory navigation (test no-op behavior)
- Windows drive root (C:\)
- Unix root (/)
- Network paths (\\server\share)

## Accessibility

**Floating Debug Panel:**
- Title bar has proper aria-label for drag handle
- Close button has aria-label: "Close debug panel"
- Collapse button has aria-label: "Collapse panel" / "Expand panel"
- Keyboard escape to close (ESC key)
- Focus management: trap focus when dragging

**Parent Navigation:**
- Button has aria-label: "Navigate to parent directory"
- Keyboard accessible (already handled by CircleButton)
- No disabled state confusion (always enabled)

## Future Enhancements

Not in scope for this design:

- Remember panel position across sessions
- Multiple debug panels for different data
- Resizable panel (fixed 300px width is sufficient)
- Dockable panel (always floating for now)
- Keyboard shortcuts for panel (ESC to close is enough)

## Open Questions

None - design is complete and approved.

## Resources

- Existing file: `OrganizeDebugPanel.tsx`
- Existing file: `OrganizeView.tsx`
- Existing file: `ExplorerView.tsx`
- Phosphor Icons: https://phosphoricons.com/
- Design system: `packages/interface/CLAUDE.md`
