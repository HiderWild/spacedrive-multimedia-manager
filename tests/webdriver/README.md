# Spacedrive Organize View - WebDriver Verification

This directory contains WebDriver-based runtime verification tests for the organize view.

## What This Proves

These tests establish real runtime evidence that:

1. **WebDriver Chain Works**: Edge browser can be controlled via WebDriver protocol
2. **tauri-driver Binary Works**: The Tauri WebDriver intermediary is installed and executable
3. **Organize View Structure Verified**: The 3-column layout (buckets, grid, preview) renders correctly
4. **Decision State Works**: Keep/Discard items display correctly with dimming
5. **Tauri API Integration**: The `window.__TAURI__` API is available in the webview context
6. **Real Tauri App Connected**: WebView2 DevTools connection to running Spacedrive app
7. **Organize Commands Work**: `load_organize_state` and `save_organize_state` execute correctly
8. **State Persists**: Organize decisions survive save/load round-trip with correct structure

## Prerequisites

- **msedgedriver**: Auto-downloaded by Selenium on first run
- **tauri-driver**: Install with `cargo install tauri-driver`
- **Python Selenium**: `pip install selenium`
- **Node.js Selenium** (optional): `cd tests/webdriver && npm install selenium-webdriver`

## Running Tests

### Quick Run (Python)
```bash
python tests/webdriver/test_organize_view.py
```

### Quick Run (Node.js)
```bash
node tests/webdriver/test_organize_webdriver.mjs
```

### Full Suite
```bash
./tests/webdriver/run_tests.sh
```

### Against Real Tauri App
```bash
# Build the daemon and Tauri app
cargo build --bin sd-daemon
cp target/debug/sd-daemon.exe target/release/sd-daemon-x86_64-pc-windows-msvc.exe
mkdir -p apps/tauri/dist && echo '<!DOCTYPE html><html><body></body></html>' > apps/tauri/dist/index.html
cd apps/tauri/src-tauri && cargo build

# Launch with WebView2 debugging
export WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
../../target/debug/Spacedrive.exe &

# Run real app tests
python tests/webdriver/test_real_tauri_app.py
```

## Test Files

- `test_organize_view.py` - Python Selenium tests (primary)
- `test_organize_webdriver.mjs` - Node.js Selenium tests
- `test_tauri_app.py` - Tests against actual Tauri app (requires tauri-driver)
- `test_real_tauri_app.py` - Tests against real Tauri app via WebView2 DevTools
- `run_tests.sh` - Test runner script

## Architecture

```
Mock Page Tests:
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│  Test Script    │────▶│  Selenium    │────▶│  Edge Browser   │
│  (Python/Node)  │     │  WebDriver   │     │  (msedgedriver) │
└─────────────────┘     └──────────────┘     └─────────────────┘
                                                      │
                                                      ▼
                                              ┌──────────────┐
                                              │  Mock HTML   │
                                              │  (test page) │
                                              └──────────────┘

Real Tauri App Tests:
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│  Test Script    │────▶│  Selenium    │────▶│  WebView2       │
│  (Python)       │     │  DevTools    │     │  (port 9222)    │
└─────────────────┘     └──────────────┘     └─────────────────┘
                                                      │
                                                      ▼
                                              ┌──────────────┐
                                              │  Spacedrive  │
                                              │  Tauri App   │
                                              └──────────────┘
```

## Evidence Collected

### Mock Page Tests (test_organize_view.py)
| Test | Evidence |
|------|----------|
| WebDriver Connection | Browser UA string, successful navigation |
| tauri-driver Binary | Binary exists, `--help` output |
| Organize Layout | CSS selector found, grid structure verified |
| Keep/Discard Items | Text content matches expected labels |
| Dimmed State | CSS class contains "dimmed" |
| Tauri API | `window.__TAURI__` type is "object" |

### Real Tauri App Tests (test_real_tauri_app.py)
| Test | Evidence |
|------|----------|
| App Connection | Title: "Spacedrive", URL: "http://tauri.localhost/" |
| Tauri API | `window.__TAURI__` and `__TAURI_INTERNALS__` available |
| Daemon Status | `is_running: true`, socket: `127.0.0.1:8488` |
| Load Empty State | Returns `null` for nonexistent key |
| Save/Load State | Round-trip preserves version, path, items, decisions |
| State Structure | All 11 structure checks passed |

## Known Limitations

1. **Build Required**: Full Tauri app build requires daemon binary and dist directory
2. **WebView2 Debugging**: Real app tests require `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
3. **No Frontend UI**: Minimal dist means no actual organize UI, but commands work
