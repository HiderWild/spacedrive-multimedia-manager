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


def find_card(driver, name: str):
    """Find the rendered organize card for a snapshot item by its visible name."""
    return WebDriverWait(driver, UI_WAIT_SECONDS).until(
        EC.presence_of_element_located(
            (
                By.XPATH,
                "//div[@data-organize-item-id]"
                f"[.//span[normalize-space()='{name}']]",
            )
        )
    )


def click_card(driver, name: str):
    """Select a visible organize card by its snapshot item name."""
    card = find_card(driver, name)
    card.find_element(By.TAG_NAME, "button").click()
    return card


def wait_for_card_decision(driver, name: str, decision: str):
    """Wait until the current projection shows the requested decision."""
    return WebDriverWait(driver, UI_WAIT_SECONDS).until(
        lambda current: decision.lower()
        in find_card(current, name).text.lower()
    )


def open_directory(driver, name: str):
    """Double-click a visible directory card and wait for its children."""
    card = find_card(driver, name)
    ActionChains(driver).double_click(card.find_element(By.TAG_NAME, "button")).perform()
    wait_for_text(driver, "direct children")


def back_to_task_root(driver):
    """Return from a nested task directory to the task root."""
    find_clickable_by_text(driver, "Back to task root").click()
    wait_for_text(driver, "direct children")


def lasso_select_card(driver, name: str):
    """Use the real pointer lasso to select one rendered card."""
    surface = driver.find_element(By.CSS_SELECTOR, "[data-testid='organize-grid']")
    card = find_card(driver, name)
    surface_rect = driver.execute_script(
        "return arguments[0].getBoundingClientRect();", surface
    )
    card_rect = driver.execute_script(
        "return arguments[0].getBoundingClientRect();", card
    )
    center_x = surface_rect["x"] + surface_rect["width"] / 2
    center_y = surface_rect["y"] + surface_rect["height"] / 2
    start_x = card_rect["left"] + 5
    start_y = card_rect["top"] + 5
    end_x = card_rect["right"] - 5
    end_y = card_rect["bottom"] - 5

    ActionChains(driver).move_to_element_with_offset(
        surface, start_x - center_x, start_y - center_y
    ).click_and_hold().move_to_element_with_offset(
        surface, end_x - center_x, end_y - center_y
    ).release().perform()

    WebDriverWait(driver, UI_WAIT_SECONDS).until(
        lambda current: find_card(current, name)
        .find_element(By.TAG_NAME, "button")
        .get_attribute("data-selected")
        == "true"
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
    # Keep the destination outside the source tree so the real commit preflight
    # does not reject the fixture as an unsafe move topology.
    destination = root.with_name(f"{root.name}-sorted")
    destination.mkdir()
    nested = root / "nested"
    deeper = nested / "deeper"
    conflict_dir = nested / "conflict-dir"
    deeper.mkdir(parents=True)
    conflict_dir.mkdir(parents=True)
    keep = root / "keep.txt"
    discard = deeper / "discard.txt"
    moved = deeper / "move.txt"
    lasso = deeper / "lasso.txt"
    preserve = conflict_dir / "preserve.txt"
    for path in (keep, discard, moved, lasso, preserve):
        path.write_text(path.name, encoding="utf-8")

    try:
        origin = detect_app_origin()
        assert origin, "No recognised Tauri app origin found"
        driver.get(f"{origin}/organize")
        wait_for_text(driver, "New organize task")

        device_input = driver.find_element(
            By.XPATH, "//label[contains(normalize-space(), 'Device')]/input"
        )
        folder_input = driver.find_element(
            By.XPATH, "//label[contains(normalize-space(), 'Windows folder')]/input"
        )
        device_input.clear()
        device_input.send_keys("local")
        folder_input.clear()
        folder_input.send_keys(str(root))
        find_clickable_by_text(driver, "Start scan").click()

        WebDriverWait(driver, UI_WAIT_SECONDS).until(
            lambda current: "/organize/" in current.current_url
        )
        wait_for_text(driver, "direct children")
        wait_for_text(driver, "nested")
        # The snapshot job is asynchronous. Decisions are disabled until the
        # active task state is rendered by the lifecycle controls.
        find_clickable_by_text(driver, "Finish")

        click_card(driver, "keep.txt")
        find_clickable_by_text(driver, "Keep").click()
        wait_for_card_decision(driver, "keep.txt", "keep")

        open_directory(driver, "nested")
        open_directory(driver, "deeper")
        wait_for_text(driver, "discard.txt")

        lasso_select_card(driver, "lasso.txt")
        find_clickable_by_text(driver, "Keep").click()
        wait_for_card_decision(driver, "lasso.txt", "keep")

        click_card(driver, "discard.txt")
        find_clickable_by_text(driver, "Discard").click()
        wait_for_card_decision(driver, "discard.txt", "discard")

        click_card(driver, "move.txt")
        find_clickable_by_text(driver, "Move…").click()
        move_input = driver.find_element(
            By.CSS_SELECTOR, "input[placeholder='C:\\Sorted\\Keep']"
        )
        move_input.send_keys(str(destination))
        find_clickable_by_text(driver, "Set destination").click()
        wait_for_card_decision(driver, "move.txt", "move")

        # Establish a Keep descendant, then exercise the real parent conflict
        # dialog once with Cancel and once with Confirm override.
        back_to_task_root(driver)
        open_directory(driver, "nested")
        open_directory(driver, "conflict-dir")
        click_card(driver, "preserve.txt")
        find_clickable_by_text(driver, "Keep").click()
        wait_for_card_decision(driver, "preserve.txt", "keep")
        back_to_task_root(driver)
        open_directory(driver, "nested")
        click_card(driver, "conflict-dir")
        find_clickable_by_text(driver, "Discard").click()
        conflict_dialog = WebDriverWait(driver, UI_WAIT_SECONDS).until(
            EC.visibility_of_element_located((By.CSS_SELECTOR, "[role='alertdialog']"))
        )
        conflict_dialog.find_element(
            By.XPATH, ".//button[normalize-space()='Cancel']"
        ).click()
        open_directory(driver, "conflict-dir")
        assert "keep" in find_card(driver, "preserve.txt").text.lower()
        back_to_task_root(driver)
        open_directory(driver, "nested")
        click_card(driver, "conflict-dir")
        find_clickable_by_text(driver, "Discard").click()
        conflict_dialog = WebDriverWait(driver, UI_WAIT_SECONDS).until(
            EC.visibility_of_element_located((By.CSS_SELECTOR, "[role='alertdialog']"))
        )
        conflict_dialog.find_element(
            By.XPATH, ".//button[normalize-space()='Confirm override']"
        ).click()
        wait_for_card_decision(driver, "conflict-dir", "discard")

        # Decisions are task state, while files remain unchanged until commit.
        driver.refresh()
        find_clickable_by_text(driver, "Finish")
        open_directory(driver, "nested")
        open_directory(driver, "deeper")
        assert "discard" in find_card(driver, "discard.txt").text.lower()
        assert "move" in find_card(driver, "move.txt").text.lower()
        assert keep.exists() and discard.exists() and moved.exists()

        # Change a decided source after the snapshot. The review must require
        # explicit drift confirmation before the commit can be dispatched.
        discard.write_text("external drift", encoding="utf-8")
        find_clickable_by_text(driver, "Scan changes").click()
        # The scan action dispatches a job. Refresh after the action so the
        # review reads the settled change-scan result, not the old plan cache.
        driver.refresh()
        find_clickable_by_text(driver, "Finish")
        WebDriverWait(driver, UI_WAIT_SECONDS).until(
            lambda current: "1 changed or missing roots"
            in current.find_element(
                By.CSS_SELECTOR, "[aria-label='Organize changes']"
            ).text
        )
        find_clickable_by_text(driver, "Review commit").click()
        commit_dialog = WebDriverWait(driver, UI_WAIT_SECONDS).until(
            EC.visibility_of_element_located((By.CSS_SELECTOR, "[role='dialog']"))
        )
        assert "changed or missing" in commit_dialog.text.lower()
        delete_confirmation = commit_dialog.find_element(
            By.XPATH, ".//label[contains(., 'permanently deleted')]//input"
        )
        delete_confirmation.click()
        commit_button = commit_dialog.find_element(
            By.XPATH, ".//button[normalize-space()='Commit plan']"
        )
        assert commit_button.get_attribute("disabled") is not None
        # No commit was dispatched while drift remained unconfirmed.
        assert keep.exists() and discard.exists() and moved.exists()

        commit_dialog.find_element(
            By.XPATH, ".//label[contains(., 'allow current subtree drift')]//input"
        ).click()
        commit_dialog.find_element(
            By.XPATH, ".//button[normalize-space()='Commit plan']"
        ).click()
        WebDriverWait(driver, UI_WAIT_SECONDS).until(
            lambda _current: not discard.exists()
            and not conflict_dir.exists()
            and not moved.exists()
            and (destination / moved.name).exists()
        )
        find_clickable_by_text(driver, "Finish")

        assert keep.exists() and lasso.exists()
        assert not discard.exists()
        assert not conflict_dir.exists()
        assert not moved.exists()
        assert (destination / moved.name).exists()

        find_clickable_by_text(driver, "Finish").click()
        WebDriverWait(driver, UI_WAIT_SECONDS).until(
            lambda current: len(
                current.find_elements(By.XPATH, "//button[normalize-space()='Reopen']")
            ) == 1
        )
        assert driver.find_element(
            By.XPATH, "//button[normalize-space()='Keep']"
        ).get_attribute("disabled") is not None

        find_clickable_by_text(driver, "Reopen").click()
        find_clickable_by_text(driver, "Finish")
        print("  Recursive task, nested navigation, decisions, lasso, conflict safety, reload, commit effects, drift gate, and lifecycle passed")
        print("  PASSED")
    finally:
        quit_driver(driver)
        shutil.rmtree(root, ignore_errors=True)
        shutil.rmtree(destination, ignore_errors=True)


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
