"""
Real Tauri App WebDriver verification harness.

This file is a small Windows-first smoke harness for the recursive organize
task route. It connects to a running WebView2 instance over DevTools and uses
the visible application UI for organize-task coverage.

Prerequisites:
- Tauri app built and running with remote debugging enabled
- Selenium installed (``pip install selenium``)

Usage:
  set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
  python tests/webdriver/test_real_tauri_app.py
"""

import json
import shutil
import tempfile
import urllib.request
from pathlib import Path
from urllib.parse import urlparse

from selenium import webdriver
from selenium.common.exceptions import NoAlertPresentException
from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.by import By
from selenium.webdriver.edge.options import Options as EdgeOptions
from selenium.webdriver.support import expected_conditions as EC
from selenium.webdriver.support.ui import WebDriverWait


DEBUG_PORT = 9222
UI_WAIT_SECONDS = 20
DEV_ORIGINS = ("http://tauri.localhost", "http://localhost:1420")


def wait_for_text(driver, text: str):
    """Wait until visible page text contains the requested string."""
    return WebDriverWait(driver, UI_WAIT_SECONDS).until(
        EC.presence_of_element_located(
            (By.XPATH, f"//*[contains(normalize-space(), '{text}')]")
        )
    )


def find_clickable_by_text(driver, text: str):
    """Find an enabled button whose visible text exactly matches ``text``."""
    return WebDriverWait(driver, UI_WAIT_SECONDS).until(
        EC.element_to_be_clickable(
            (By.XPATH, f"//button[normalize-space()='{text}' and not(@disabled)]")
        )
    )


def get_debug_pages():
    """Return the pages exposed by the WebView2 DevTools endpoint."""
    try:
        response = urllib.request.urlopen(f"http://localhost:{DEBUG_PORT}/json")
        return json.loads(response.read())
    except Exception as error:
        print(f"Error connecting to debug port: {error}")
        return []


def connect_to_app():
    """Attach Selenium to the running Tauri WebView2 instance."""
    options = EdgeOptions()
    options.add_experimental_option("debuggerAddress", f"localhost:{DEBUG_PORT}")
    return webdriver.Edge(options=options)


def quit_driver(driver):
    """Close the attached driver without hiding an earlier test failure."""
    try:
        driver.quit()
    except Exception as error:
        print(f"  WARNING: driver.quit() failed: {error}")


def detect_app_origin():
    """Return the recognised origin of the running Tauri application."""
    for page in get_debug_pages():
        parsed = urlparse(page.get("url", ""))
        origin = f"{parsed.scheme}://{parsed.netloc}" if parsed.scheme else ""
        if origin in DEV_ORIGINS:
            return origin
    return None


def test_app_connection():
    """Verify that a Tauri page is available on a supported origin."""
    print("\n[App Connection]")
    pages = get_debug_pages()
    assert pages, "No debugging pages found"
    origin = detect_app_origin()
    assert origin in DEV_ORIGINS, (
        f"Expected app origin to be one of {DEV_ORIGINS}, got "
        f"{[page.get('url') for page in pages]}"
    )
    driver = connect_to_app()
    try:
        assert driver.title in ("", "Spacedrive"), f"Unexpected title: {driver.title!r}"
        assert driver.current_url.startswith(origin)
        print(f"  Origin: {origin}")
        print("  PASSED")
    finally:
        quit_driver(driver)


def test_tauri_api():
    """Verify that the attached page exposes the Tauri runtime."""
    print("\n[Tauri API]")
    driver = connect_to_app()
    try:
        assert driver.execute_script("return window.__TAURI__ !== undefined;")
        assert driver.execute_script("return window.__TAURI_INTERNALS__ !== undefined;")
        assert driver.execute_script("return window.__TAURI__?.core !== undefined;")
        assert driver.execute_script(
            "return typeof window.__TAURI__?.core?.invoke === 'function';"
        )
        print("  Tauri runtime and core invoke are available")
        print("  PASSED")
    finally:
        quit_driver(driver)


def test_daemon_status():
    """Verify that the app has a running daemon for library operations."""
    print("\n[Daemon Status]")
    driver = connect_to_app()
    try:
        result = driver.execute_script(
            """
            return new Promise(async (resolve) => {
                try {
                    const status = await window.__TAURI__.core.invoke('get_daemon_status');
                    resolve({success: true, status});
                } catch (error) {
                    resolve({success: false, error: error.toString()});
                }
            });
            """
        )
        assert result["success"], result.get("error")
        assert result["status"]["is_running"]
        print("  Daemon is running")
        print("  PASSED")
    finally:
        quit_driver(driver)


def test_recursive_organize_task_vertical_flow():
    """Exercise the visible recursive organize task lifecycle end to end."""
    print("\n[Recursive Organize Task - Vertical Flow]")
    driver = connect_to_app()
    root = Path(tempfile.mkdtemp(prefix="spacedrive-organize-task-"))
    destination = root / "sorted"
    destination.mkdir()
    nested = root / "nested" / "deeper"
    nested.mkdir(parents=True)
    keep = root / "keep.txt"
    discard = nested / "discard.txt"
    moved = nested / "move.txt"
    for path in (keep, discard, moved):
        path.write_text(path.name, encoding="utf-8")

    try:
        origin = detect_app_origin()
        assert origin, "No recognised Tauri app origin found"
        driver.get(f"{origin}/organize")
        wait_for_text(driver, "New organize task")

        inputs = driver.find_elements(By.TAG_NAME, "input")
        assert len(inputs) >= 2, "Expected device and Windows folder inputs"
        inputs[0].clear()
        inputs[0].send_keys("local")
        inputs[1].send_keys(str(root))
        find_clickable_by_text(driver, "Start scan").click()

        WebDriverWait(driver, UI_WAIT_SECONDS).until(
            lambda current: "/organize/" in current.current_url
        )
        wait_for_text(driver, "direct children")
        wait_for_text(driver, "nested")

        nested_card = driver.find_element(
            By.XPATH, "//button[.//span[normalize-space()='nested']]"
        )
        ActionChains(driver).double_click(nested_card).perform()
        wait_for_text(driver, "discard.txt")

        driver.find_element(
            By.XPATH, "//button[.//span[normalize-space()='discard.txt']]"
        ).click()
        find_clickable_by_text(driver, "Discard").click()

        driver.find_element(
            By.XPATH, "//button[.//span[normalize-space()='move.txt']]"
        ).click()
        find_clickable_by_text(driver, "Move…").click()
        move_input = driver.find_element(
            By.CSS_SELECTOR, "input[placeholder='C:\\Sorted\\Keep']"
        )
        move_input.send_keys(str(destination))
        find_clickable_by_text(driver, "Set destination").click()

        # Decisions are task state, while files remain unchanged until commit.
        driver.refresh()
        wait_for_text(driver, "discard.txt")
        assert keep.exists() and discard.exists() and moved.exists()

        find_clickable_by_text(driver, "Finish").click()
        try:
            driver.switch_to.alert.accept()
        except NoAlertPresentException:
            pass
        WebDriverWait(driver, UI_WAIT_SECONDS).until(
            lambda current: len(
                current.find_elements(By.XPATH, "//button[normalize-space()='Reopen']")
            ) == 1
        )
        assert keep.exists() and discard.exists() and moved.exists()
        assert driver.find_element(
            By.XPATH, "//button[normalize-space()='Keep']"
        ).get_attribute("disabled") is not None

        find_clickable_by_text(driver, "Reopen").click()
        WebDriverWait(driver, UI_WAIT_SECONDS).until(
            lambda current: len(
                current.find_elements(By.XPATH, "//button[normalize-space()='Finish']")
            ) == 1
        )

        drift = nested / "drift.txt"
        drift.write_text("external", encoding="utf-8")
        find_clickable_by_text(driver, "Scan changes").click()
        find_clickable_by_text(driver, "Review commit").click()
        wait_for_text(driver, "changed")
        assert keep.exists() and discard.exists() and moved.exists() and drift.exists()
        print("  Recursive task, nested navigation, decisions, reload, lifecycle, and drift safety passed")
        print("  PASSED")
    finally:
        quit_driver(driver)
        shutil.rmtree(root, ignore_errors=True)


def main():
    """Run the connection checks and the single recursive-task acceptance flow."""
    print("=" * 60)
    print("Real Tauri App - WebDriver Verification")
    print("=" * 60)

    if not get_debug_pages():
        print("\nERROR: No Tauri app found on debug port")
        print(
            "Launch with: WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="
            "--remote-debugging-port=9222 Spacedrive.exe"
        )
        return False

    tests = [
        test_app_connection,
        test_tauri_api,
        test_daemon_status,
        test_recursive_organize_task_vertical_flow,
    ]
    passed = 0
    failed = 0
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as error:
            print(f"  FAILED: {error}")
            failed += 1

    print("\n" + "=" * 60)
    print(f"Results: {passed} passed, {failed} failed")
    print("=" * 60)
    return failed == 0


if __name__ == "__main__":
    raise SystemExit(0 if main() else 1)
