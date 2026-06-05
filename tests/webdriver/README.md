# Spacedrive Organize View - WebDriver Verification

WebDriver-based runtime verification that the organize view commands work in a real Tauri app.

## What This Proves

Connects to a running Spacedrive Tauri app via WebView2 DevTools (port 9222) and verifies:

1. **App launches**: Title is "Spacedrive", URL is `tauri.localhost`
2. **Tauri API available**: `window.__TAURI__`, `__TAURI_INTERNALS__`, and `core.invoke` exist
3. **Daemon runs**: `get_daemon_status` returns `is_running: true`
4. **Organize state round-trips**: `save_organize_state` + `load_organize_state` preserve version, path, items, and decisions
5. **State structure correct**: persisted items have all required fields (itemId, path, name, kind, decision, updatedAt)

## Prerequisites

- **Python Selenium**: `pip install selenium`
- **msedgedriver**: auto-downloaded by Selenium on first run
- **Tauri app built and running** with WebView2 debugging enabled

## Usage

```bash
# Build
cargo build --bin sd-daemon
cp target/debug/sd-daemon.exe target/release/sd-daemon-x86_64-pc-windows-msvc.exe
mkdir -p apps/tauri/dist && echo '<!DOCTYPE html><html><body></body></html>' > apps/tauri/dist/index.html
cd apps/tauri/src-tauri && cargo build

# Launch with debugging
export WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
../../target/debug/Spacedrive.exe &

# Run
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

## Test Results

| Test | What It Asserts |
|------|----------------|
| App Connection | Title == "Spacedrive", URL contains "tauri.localhost" |
| Tauri API | `__TAURI__`, `__TAURI_INTERNALS__`, `core.invoke` all present |
| Daemon Status | `is_running == true` |
| Load Empty State | Returns `null` for nonexistent key |
| Save/Load State | Round-trip: version=1, path="/test/photos", 2 items, decisions=["discard","keep"] |
| State Structure | All 11 field-level checks pass on persisted items |

## Known Limitations

- No frontend UI required (minimal dist) — tests exercise backend commands only
- Requires the app to be running before test execution
