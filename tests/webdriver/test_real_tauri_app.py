"""
Real Tauri App WebDriver verification harness.

Connects to the actual running Spacedrive Tauri app via WebView2 DevTools and
verifies the organize commands and the real organize UI work at runtime.

Prerequisites:
- Tauri app built and running with remote debugging enabled
- Selenium installed (pip install selenium)

Usage:
  # Launch the app with debugging enabled (see launch_tauri_debug.cmd):
  set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
  cargo tauri dev --no-watch

  # Then run the tests:
  python tests/webdriver/test_real_tauri_app.py
"""

import sys
import time
import json
import uuid
import tempfile
import urllib.request
from pathlib import Path
from urllib.parse import quote, urlparse

from selenium import webdriver
from selenium.webdriver.edge.options import Options as EdgeOptions
from selenium.webdriver.common.by import By
from selenium.webdriver.support import expected_conditions as EC
from selenium.webdriver.support.ui import WebDriverWait


DEBUG_PORT = 9222
UI_WAIT_SECONDS = 20

# The Tauri app may be running from either:
#   - a packaged build: assets served at http://tauri.localhost/
#   - `cargo tauri dev` (devUrl):  assets served at http://localhost:1420/
# The harness auto-detects which one is live from the running page.
DEV_ORIGINS = ("http://tauri.localhost", "http://localhost:1420")


def explorer_query_for_physical_directory(directory: Path, device_slug: str) -> str:
    path = {
        "Physical": {
            "device_slug": device_slug,
            "path": str(directory),
        }
    }
    return f"/explorer?path={quote(json.dumps(path))}"


def wait_for_text(driver, text: str):
    return WebDriverWait(driver, UI_WAIT_SECONDS).until(
        EC.presence_of_element_located(
            (By.XPATH, f"//*[contains(normalize-space(), '{text}')]")
        )
    )


def get_debug_pages():
    """Get available debugging pages from WebView2."""
    try:
        response = urllib.request.urlopen(f"http://localhost:{DEBUG_PORT}/json")
        return json.loads(response.read())
    except Exception as e:
        print(f"Error connecting to debug port: {e}")
        return []


def connect_to_app():
    """Connect to the running Tauri app via DevTools."""
    options = EdgeOptions()
    options.add_experimental_option("debuggerAddress", f"localhost:{DEBUG_PORT}")
    return webdriver.Edge(options=options)


def quit_driver(driver):
    try:
        driver.quit()
    except Exception as e:
        print(f"  WARNING: driver.quit() failed: {e}")


def detect_app_origin():
    """Return the origin (scheme://host[:port]) the running Tauri app is serving from."""
    pages = get_debug_pages()
    for page in pages:
        url = page.get("url", "")
        parsed = urlparse(url)
        if not parsed.scheme:
            continue
        origin = f"{parsed.scheme}://{parsed.netloc}"
        if origin in DEV_ORIGINS:
            return origin
    return None


def resolve_local_device_slug(driver):
    """Extract a real device slug from the running app's persisted state.

    The TabManager and the view-preferences store both encode the current
    Physical path (including device_slug) as JSON inside their persisted entries.
    The JSON itself is URL-encoded inside the `savedPath` string, so we first
    look for the raw form and then fall back to a URL-decoded scan. Reading
    either one avoids hard-coding a brittle slug guess.
    """
    import re
    from urllib.parse import unquote

    persisted = driver.execute_script(
        """
        return {
            tabs: localStorage.getItem('sd-tabs-state'),
            prefs: localStorage.getItem('spacedrive-view-preferences'),
            currentUrl: window.location.href
        };
        """
    )

    raw_pattern = re.compile(r'"device_slug"\s*:\s*"([^"]+)"')

    def scan(value):
        if not value:
            return None
        # Try the raw form first.
        m = raw_pattern.search(value)
        if m:
            return m.group(1)
        # Fall back to URL-decoded form (savedPath embeds the JSON encoded).
        try:
            decoded = unquote(value)
            m = raw_pattern.search(decoded)
            if m:
                return m.group(1)
        except Exception:
            pass
        return None

    for key in ("tabs", "prefs", "currentUrl"):
        slug = scan(persisted.get(key))
        if slug:
            return slug
    return None


def test_app_connection():
    """Test basic connection to the Tauri app and identify its origin."""
    print("\n[App Connection]")
    pages = get_debug_pages()
    assert len(pages) > 0, "No debugging pages found"
    print(f"  Found {len(pages)} page(s)")

    origin = detect_app_origin()
    assert origin in DEV_ORIGINS, (
        f"Expected app origin to be one of {DEV_ORIGINS}, got page urls: "
        f"{[p.get('url') for p in pages]}"
    )

    driver = connect_to_app()
    try:
        title = driver.title
        url = driver.current_url
        print(f"  Title: {title}")
        print(f"  URL: {url}")
        # In packaged builds the page title is 'Spacedrive'; in dev the Vite page
        # may have an empty title before the app boots — accept either.
        assert title == "" or title == "Spacedrive", (
            f"Unexpected title: '{title}'"
        )
        assert url.startswith(origin), (
            f"Expected URL to start with {origin}, got '{url}'"
        )
        print(f"  Origin: {origin}")
        print("  PASSED")
    finally:
        quit_driver(driver)


def test_tauri_api():
    """Test Tauri API availability."""
    print("\n[Tauri API]")
    driver = connect_to_app()
    try:
        has_tauri = driver.execute_script(
            "return window.__TAURI__ !== undefined;"
        )
        assert has_tauri, "Tauri API not found"
        print(f"  window.__TAURI__ available: {has_tauri}")

        has_internals = driver.execute_script(
            "return window.__TAURI_INTERNALS__ !== undefined;"
        )
        assert has_internals, "Tauri internals not found (required for invoke)"
        print(f"  window.__TAURI_INTERNALS__ available: {has_internals}")

        has_core = driver.execute_script(
            "return window.__TAURI__?.core !== undefined;"
        )
        assert has_core, "Tauri core API not found"
        print(f"  Tauri core API available: {has_core}")

        has_invoke = driver.execute_script(
            "return typeof window.__TAURI__?.core?.invoke === 'function';"
        )
        assert has_invoke, "Tauri invoke not available"
        print(f"  Tauri invoke function available: {has_invoke}")

        print("  PASSED")
    finally:
        quit_driver(driver)


def test_daemon_status():
    """Test daemon status command."""
    print("\n[Daemon Status]")
    driver = connect_to_app()
    try:
        result = driver.execute_script("""
            return new Promise(async (resolve) => {
                try {
                    const status = await window.__TAURI__.core.invoke('get_daemon_status');
                    resolve({ success: true, status: status });
                } catch (e) {
                    resolve({ success: false, error: e.toString() });
                }
            });
        """)
        assert result["success"], f"Failed: {result.get('error')}"
        status = result["status"]
        print(f"  Daemon running: {status['is_running']}")
        print(f"  Socket address: {status['socket_addr']}")
        print(f"  Started by us: {status['started_by_us']}")
        assert status["is_running"], "Daemon should be running"
        print("  PASSED")
    finally:
        quit_driver(driver)


def test_organize_real_ui_renders_for_physical_directory():
    """Drive the real Tauri UI to render the organize view for a physical directory."""
    print("\n[Organize Real UI]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-ui-") as temp_dir:
        directory = Path(temp_dir)
        (directory / "keep-candidate.txt").write_text("keep", encoding="utf-8")
        (directory / "discard-candidate.txt").write_text("discard", encoding="utf-8")

        driver = connect_to_app()
        try:
            # Resolve a real device_slug from the running app's own state
            # rather than guessing one. Without this the daemon returns no
            # entries for an unknown device and the explorer pane stays empty.
            device_slug = resolve_local_device_slug(driver)
            assert device_slug, (
                "Could not resolve local device slug from persisted app state. "
                "Open the explorer in the app at least once before running this test."
            )
            print(f"  Resolved local device slug: {device_slug}")

            target = explorer_query_for_physical_directory(directory, device_slug)

            # The TabManager restores its persisted tab on startup; pushState
            # and `driver.get()` alone cannot defeat that restore. Pre-seed the
            # persisted tab state to (a) the target directory and (b) the
            # organize view mode, then full-reload so the seeded state is used
            # for the initial render. Also force English locale so the UI
            # strings the assertions look for actually render in English.
            tab_id = str(uuid.uuid4())
            tabs_state = {
                "tabs": [
                    {
                        "id": tab_id,
                        "title": directory.name,
                        "icon": None,
                        "isPinned": False,
                        "lastActive": int(time.time() * 1000),
                        "savedPath": target,
                    }
                ],
                "activeTabId": tab_id,
                "explorerStates": {
                    tab_id: {
                        "viewMode": "organize",
                        "sortBy": "name",
                        "gridSize": 120,
                        "gapSize": 16,
                        "foldersFirst": True,
                        "columnStack": [],
                        "scrollTop": 0,
                        "scrollLeft": 0,
                        "sizeViewTransform": {"k": 1, "x": 0, "y": 0},
                    }
                },
            }

            driver.execute_script(
                "localStorage.setItem('sd-language', 'en');"
                "localStorage.setItem('sd-tabs-state', arguments[0]);",
                json.dumps(tabs_state),
            )

            url = f"{origin}{target}"
            driver.get(url)
            print(f"  Opened physical directory URL: {url}")

            wait_for_text(driver, "Keep this tab")
            wait_for_text(driver, "Discard this tab")
            wait_for_text(driver, "No items to preview")

            body_text = driver.find_element(By.TAG_NAME, "body").text
            assert "keep-candidate.txt" in body_text, (
                "Expected keep candidate in real organize UI. "
                f"Body sample: {body_text[:500]!r}"
            )
            assert "discard-candidate.txt" in body_text, (
                "Expected discard candidate in real organize UI. "
                f"Body sample: {body_text[:500]!r}"
            )

            print("  Organize controls and physical directory entries are visible")
            print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_load_empty():
    """Test loading organize state for non-existent key."""
    print("\n[Organize Load Empty]")
    driver = connect_to_app()
    try:
        result = driver.execute_script("""
            return new Promise(async (resolve) => {
                try {
                    const loadResult = await window.__TAURI__.core.invoke(
                        'load_organize_state',
                        { directoryKey: 'nonexistent-key-12345' }
                    );
                    resolve({ success: true, result: loadResult });
                } catch (e) {
                    resolve({ success: false, error: e.toString() });
                }
            });
        """)
        assert result["success"], f"Failed: {result.get('error')}"
        assert result["result"] is None, "Should return null for missing key"
        print(f"  Load nonexistent key: null (correct)")
        print("  PASSED")
    finally:
        quit_driver(driver)


def test_organize_save_and_load():
    """Test saving and loading organize state."""
    print("\n[Organize Save/Load]")
    driver = connect_to_app()
    try:
        result = driver.execute_script("""
            return new Promise(async (resolve) => {
                try {
                    const testState = JSON.stringify({
                        version: 1,
                        directoryPath: "/test/photos",
                        updatedAt: new Date().toISOString(),
                        items: {
                            "file-1": {
                                itemId: "file-1",
                                path: "/test/photos/sunset.jpg",
                                name: "sunset.jpg",
                                kind: "File",
                                decision: "keep",
                                updatedAt: new Date().toISOString()
                            },
                            "file-2": {
                                itemId: "file-2",
                                path: "/test/photos/blurry.jpg",
                                name: "blurry.jpg",
                                kind: "File",
                                decision: "discard",
                                updatedAt: new Date().toISOString()
                            }
                        }
                    });

                    await window.__TAURI__.core.invoke('save_organize_state', {
                        directoryKey: 'webdriver-e2e-test',
                        json: testState
                    });

                    const loaded = await window.__TAURI__.core.invoke(
                        'load_organize_state',
                        { directoryKey: 'webdriver-e2e-test' }
                    );

                    const parsed = loaded ? JSON.parse(loaded) : null;

                    resolve({
                        success: true,
                        saved: true,
                        loaded: loaded !== null,
                        version: parsed?.version,
                        directoryPath: parsed?.directoryPath,
                        itemCount: parsed ? Object.keys(parsed.items).length : 0,
                        decisions: parsed ? Object.values(parsed.items).map(i => i.decision) : []
                    });
                } catch (e) {
                    resolve({ success: false, error: e.toString() });
                }
            });
        """)
        assert result["success"], f"Failed: {result.get('error')}"
        assert result["saved"], "Should have saved"
        assert result["loaded"], "Should have loaded"
        assert result["version"] == 1, f"Version should be 1, got {result['version']}"
        assert result["directoryPath"] == "/test/photos"
        assert result["itemCount"] == 2
        assert sorted(result["decisions"]) == ["discard", "keep"]

        print(f"  Saved: {result['saved']}")
        print(f"  Loaded: {result['loaded']}")
        print(f"  Version: {result['version']}")
        print(f"  Directory: {result['directoryPath']}")
        print(f"  Items: {result['itemCount']}")
        print(f"  Decisions: {result['decisions']}")
        print("  PASSED")
    finally:
        quit_driver(driver)


def test_organize_state_structure():
    """Test that persisted state has correct structure."""
    print("\n[Organize State Structure]")
    driver = connect_to_app()
    try:
        result = driver.execute_script("""
            return new Promise(async (resolve) => {
                try {
                    const testState = JSON.stringify({
                        version: 1,
                        directoryPath: "/test/structure-check",
                        updatedAt: new Date().toISOString(),
                        items: {
                            "item-1": {
                                itemId: "item-1",
                                path: "/test/structure-check/a.jpg",
                                name: "a.jpg",
                                kind: "File",
                                decision: "keep",
                                updatedAt: new Date().toISOString()
                            }
                        }
                    });

                    await window.__TAURI__.core.invoke('save_organize_state', {
                        directoryKey: 'webdriver-structure-test',
                        json: testState
                    });

                    const loaded = await window.__TAURI__.core.invoke(
                        'load_organize_state',
                        { directoryKey: 'webdriver-structure-test' }
                    );

                    if (!loaded) {
                        resolve({ success: false, error: 'No state found' });
                        return;
                    }

                    const parsed = JSON.parse(loaded);

                    const checks = {
                        hasVersion: parsed.version === 1,
                        hasDirectoryPath: typeof parsed.directoryPath === 'string',
                        hasUpdatedAt: typeof parsed.updatedAt === 'string',
                        hasItems: typeof parsed.items === 'object',
                        itemsHaveItemId: Object.values(parsed.items).every(i => 'itemId' in i),
                        itemsHavePath: Object.values(parsed.items).every(i => 'path' in i),
                        itemsHaveName: Object.values(parsed.items).every(i => 'name' in i),
                        itemsHaveKind: Object.values(parsed.items).every(i => 'kind' in i),
                        itemsHaveDecision: Object.values(parsed.items).every(i => 'decision' in i),
                        itemsHaveUpdatedAt: Object.values(parsed.items).every(i => 'updatedAt' in i),
                        validDecisions: Object.values(parsed.items).every(
                            i => i.decision === 'keep' || i.decision === 'discard'
                        )
                    };

                    resolve({
                        success: true,
                        checks: checks,
                        allPassed: Object.values(checks).every(v => v)
                    });
                } catch (e) {
                    resolve({ success: false, error: e.toString() });
                }
            });
        """)
        assert result["success"], f"Failed: {result.get('error')}"
        assert result["allPassed"], f"Checks failed: {result['checks']}"

        print(f"  All structure checks passed: {result['allPassed']}")
        for check, passed in result["checks"].items():
            print(f"    {check}: {'PASS' if passed else 'FAIL'}")
        print("  PASSED")
    finally:
        quit_driver(driver)


def main():
    print("=" * 60)
    print("Real Tauri App - WebDriver Verification")
    print("=" * 60)

    # Check connection first
    pages = get_debug_pages()
    if not pages:
        print("\nERROR: No Tauri app found on debug port")
        print("Launch with: WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 Spacedrive.exe")
        return False

    tests = [
        test_app_connection,
        test_tauri_api,
        test_daemon_status,
        test_organize_real_ui_renders_for_physical_directory,
        test_organize_load_empty,
        test_organize_save_and_load,
        test_organize_state_structure,
    ]

    passed = 0
    failed = 0

    for t in tests:
        try:
            t()
            passed += 1
        except Exception as e:
            print(f"  FAILED: {e}")
            failed += 1

    print("\n" + "=" * 60)
    print(f"Results: {passed} passed, {failed} failed")
    print("=" * 60)

    return failed == 0


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
