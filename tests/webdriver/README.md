# Spacedrive Organize View - WebDriver Verification

WebDriver-based runtime verification that recursive organize tasks work inside a
running Spacedrive Tauri app.

## What This Proves

Connects to the running Spacedrive Tauri app via WebView2 DevTools (port 9222),
drives the real React UI, and asserts that:

1. **App launches**: title `Spacedrive`, page served from the dev or packaged origin
2. **Tauri runtime**: `window.__TAURI__`, `__TAURI_INTERNALS__`, and `core.invoke` are present
3. **Daemon running**: `get_daemon_status` returns `is_running: true`
4. **One real recursive task** is created from the `/organize` entry form for a
   physical Windows directory and navigated through nested children.
5. **Visible decisions** cover Keep, Discard, and Move. Parent/descendant
   conflicts are handled by the real confirmation dialog.
6. **Persistence and safety**: reload restores the task and decisions, and no
   file changes occur before commit.
7. **Lifecycle**: Finish makes the task read-only, Reopen restores editing, and
   the review dialog reports filesystem drift without side effects.

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

The vertical test opens `/organize`, fills the visible device and Windows-folder
inputs, starts a real snapshot, and follows the redirect to `/organize/:taskId`.
It uses only visible DOM controls and temporary files. It does not seed
Explorer `viewMode`, call private Tauri JSON commands, or inspect an internal
organize-state file.

## Test Results

| Test | What It Asserts |
|------|----------------|
| App Connection | Title `Spacedrive`, URL on a recognised app origin |
| Tauri API | `__TAURI__`, `__TAURI_INTERNALS__`, `core.invoke` all present |
| Daemon Status | `is_running == true` |
| Recursive task vertical flow | Create, nested navigation, Keep/Discard/Move, reload, Finish/Reopen, and drift safety |

## Known Limitations

- Requires the app to be running before test execution.
- The UI assertions key on Tailwind class names (`opacity-50`,
   `text-emerald-400`, `text-rose-400`, `ring-2`) and visible English strings;
  the harness temporarily forces `sd-language = en` to keep those strings
  stable, then restores the original keys before the driver exits.
