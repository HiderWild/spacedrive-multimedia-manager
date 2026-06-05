"""
Real Tauri App WebDriver verification harness.

Connects to the actual running Spacedrive Tauri app via WebView2 DevTools
and verifies the organize commands work at runtime.

Prerequisites:
- Tauri app built and running with remote debugging enabled
- Selenium installed (pip install selenium)

Usage:
  # First, launch the app with debugging enabled:
  set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
  target/debug/Spacedrive.exe

  # Then run the tests:
  python tests/webdriver/test_real_tauri_app.py
"""

import sys
import json
import urllib.request
from selenium import webdriver
from selenium.webdriver.edge.options import Options as EdgeOptions


DEBUG_PORT = 9222


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


def test_app_connection():
    """Test basic connection to the Tauri app."""
    print("\n[App Connection]")
    pages = get_debug_pages()
    assert len(pages) > 0, "No debugging pages found"
    print(f"  Found {len(pages)} page(s)")

    driver = connect_to_app()
    try:
        title = driver.title
        url = driver.current_url
        print(f"  Title: {title}")
        print(f"  URL: {url}")
        assert title == "Spacedrive", f"Expected 'Spacedrive', got '{title}'"
        assert "tauri.localhost" in url, f"Expected tauri.localhost URL, got '{url}'"
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
