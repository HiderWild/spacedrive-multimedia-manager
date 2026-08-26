# Album Scroll Position (Session Memory)

**Date:** 2026-07-12  
**Status:** Implemented for Grid / Masonry / List / Media views.

## Goal

When the user opens a photo (QuickPreview) and closes it, the album/explorer
grid stays at the pre-open scroll offset instead of jumping to the top.

## Scope

- **In memory only** (session / per browser tab state via TabManager).
- **Not** disk / library persistence across app restarts.

## Implementation

Explorer context already stored per-tab `scrollTop` / `scrollLeft`. Views had
TODOs and never wrote or restored them.

| Piece | Role |
|-------|------|
| `hooks/usePreserveScrollPosition.ts` | Save on scroll/unmount; restore on mount |
| Grid / Masonry / List / Media scroll containers | Use the hook for their overflow element |
| MediaView | Only auto-scroll to “most recent” when no saved offset |

## Behaviour

1. User scrolls the album → offset written into tab state.
2. User opens QuickPreview → view may remount; final offset flushed on unmount.
3. User closes preview → restore `scrollTop`/`scrollLeft` before paint + short retry for virtualizers.

## Out of scope

- Persisting scroll across app restarts (disk).
- Restoring after navigating to a **different** folder (new path resets via tab state usage).
- Cross-device sync of scroll position.
