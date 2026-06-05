# Spacedrive Organize View - WebDriver Verification

WebDriver-based runtime verification that the organize view (UI + commands)
actually works inside a running Spacedrive Tauri app.

## What This Proves

Connects to the running Spacedrive Tauri app via WebView2 DevTools (port 9222),
drives the real React UI, and asserts that:

1. **App launches**: title `Spacedrive`, page served from the dev or packaged origin
2. **Tauri runtime**: `window.__TAURI__`, `__TAURI_INTERNALS__`, and `core.invoke` are present
3. **Daemon running**: `get_daemon_status` returns `is_running: true`
4. **Real organize UI renders** for a physical directory: keep/discard controls,
   preview placeholder, and the actual filenames appear in the center pane
5. **Decision flow on the real UI**:
   - Clicking a card selects it (selection ring appears)
   - Clicking *Keep this tab* / *Discard this tab* marks the selected card
   - The marked card gets the dim (`opacity-50`) state and the matching badge
     (emerald check for keep, rose X for discard)
   - Cards that were not marked stay undimmed and have no badge
6. **Left-pane tab filtering on the real UI**:
   - The Keep tab lists only kept files
   - The Discard tab lists only discarded files
   - The Discard tab exposes an enabled *Delete now* button
7. **Reload restores decisions in the UI**: after `driver.get(url)` the same
   badges and dim state reappear on the same cards with no further interaction
8. **Delete dialog open/cancel on the real UI**:
   - Clicking *Delete now* opens the confirmation dialog with the expected
      title and description
   - Clicking *Cancel* closes the dialog
   - The file on disk is **not** removed
   - The card stays discarded in the center pane
   - The persisted organize JSON still carries the discard decision
9. **Delete confirm on the real UI**:
   - Pressing Enter on *Delete permanently* removes the discarded file from disk
   - The file disappears from the left Discard list and the center pane
   - The persisted organize JSON drops the deleted file's decision entry
10. **Preview pane wiring**:
   - Empty placeholder renders with no selection
   - Selecting a card applies the selection ring (proves selection wiring)
   - For a leaf file with no supported renderer the placeholder is still shown
11. **Directory preview branches on the real UI**:
   - No-media directories render `Preview list` only
   - Image-only directories render a disabled `Video` tab with the missing-video title
   - Mixed-media directories default to the `Video` tab and mount a video preview first
12. **Single-file video preview on the real UI**:
   - Selecting a real `mp4` renders the `<video>` preview
   - The preview keeps the `autoplay` attribute
   - The preview hydrates `sd-video-muted` / `sd-video-volume` from `localStorage`
13. **Organize state commands**: `save_organize_state` / `load_organize_state`
     round-trip preserves version, path, items, and decisions, and the persisted
     JSON shape passes 11 field-level structural checks

## What This Does *Not* Prove

These intentionally remain out of scope of the harness:

- **Codec-specific playback guarantees**. The single-file and mixed-media tests
  prove the video preview mounts and hydrates state, but they do not prove that
  every arbitrary local media file will decode, advance playback time, or pass
  browser autoplay policy checks on every machine.
- **Center-pane list-vs-grid layout toggle**. The layout state exists but has
  no user-facing trigger in the current UI surface.

## Prerequisites

- **Python Selenium**: `pip install selenium`
- **msedgedriver**: auto-downloaded by Selenium on first run
- **Tauri app built and running** with WebView2 debugging enabled
- The app must have opened the explorer at least once so a real `device_slug`
  is present in `localStorage` (the harness reads it from `sd-tabs-state` /
  `spacedrive-view-preferences` rather than guessing)

## Usage

```bash
# Launch with debugging (cargo tauri dev or a packaged build both work)
set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
cargo tauri dev --no-watch    # or run the packaged Spacedrive.exe

# Run the harness
python tests/webdriver/test_real_tauri_app.py
```

## Architecture

```
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│  test_real_     │────▶│  Selenium    │────▶│  WebView2       │
│  tauri_app.py   │     │  DevTools    │     │  (port 9222)    │
└─────────────────┘     └──────────────┘     └─────────────────┘
                                                      │
                                                      ▼
                                              ┌──────────────┐
                                              │  Spacedrive  │
                                              │  Tauri App   │
                                              └──────────────┘
```

Each UI test temporarily seeds `sd-tabs-state` so the TabManager boots straight
into organize view for a fresh temp directory, then restores the original
`localStorage` keys before quitting the driver. The harness drives the real
React UI and asserts on the rendered DOM (badge classes, dim classes, dialog
title, selection ring, on-disk file presence, persisted organize JSON).

## Test Results

| Test | What It Asserts |
|------|----------------|
| App Connection | Title `Spacedrive`, URL on a recognised app origin |
| Tauri API | `__TAURI__`, `__TAURI_INTERNALS__`, `core.invoke` all present |
| Daemon Status | `is_running == true` |
| Organize Real UI | Real organize controls + actual filenames render for a physical directory |
| Decision Flow + Reload Restore | Keep/discard clicks dim cards and add badges; tabs filter correctly; full page reload restores both decisions |
| Delete Dialog Open/Cancel | Delete dialog opens with the right title; cancel closes it; file survives on disk; card stays discarded; persisted state stays discard |
| Delete Dialog Enter Confirms + Real Delete | Enter confirms deletion; file disappears from disk, left Discard list, center pane, and persisted organize state |
| Preview Pane | Empty state renders; selecting a card applies the ring; unsupported-file placeholder shows |
| Preview no-media branch | Selecting a no-media directory renders `Preview list` only |
| Preview one-media disabled tab title | Image-only directory disables `Video` with the expected title |
| Preview mixed-media priority | Mixed-media directory defaults to `Video` and mounts a video preview first |
| Preview single-file video | Real `mp4` preview mounts with `autoplay` and saved muted/volume prefs |
| Load Empty State | Returns `null` for nonexistent key |
| Save/Load State | Round-trip: version=1, path, 2 items, decisions=keep+discard |
| State Structure | 11 field-level checks on persisted items |

## Known Limitations

- Requires the app to be running before test execution.
- Repeated runs reuse fixed test directory keys (`webdriver-e2e-test` and
  `webdriver-structure-test`) and overwrite prior test state rather than
  creating unbounded new entries.
- The UI assertions key on Tailwind class names (`opacity-50`,
   `text-emerald-400`, `text-rose-400`, `ring-2`) and visible English strings;
  the harness temporarily forces `sd-language = en` to keep those strings
  stable, then restores the original keys before the driver exits.
