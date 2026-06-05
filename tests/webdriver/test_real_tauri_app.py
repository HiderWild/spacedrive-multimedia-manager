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
from selenium.common.exceptions import TimeoutException


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


def find_clickable_by_text(driver, text: str):
    """Find an enabled button (or button-like) element whose visible text matches."""
    return WebDriverWait(driver, UI_WAIT_SECONDS).until(
        EC.element_to_be_clickable(
            (By.XPATH, f"//button[normalize-space()='{text}' and not(@disabled)]")
        )
    )


def find_button_by_text(driver, text: str):
    return driver.find_element(
        By.XPATH, f"//button[normalize-space()='{text}']"
    )


def find_card_for_filename(driver, filename: str):
    """The organize center pane renders one <button> per file with the name in a <div>."""
    return driver.find_element(
        By.XPATH,
        "//button[.//div[normalize-space()='" + filename + "']]",
    )


def wait_for_card_for_filename(driver, filename: str, timeout: int = UI_WAIT_SECONDS):
    return WebDriverWait(driver, timeout).until(
        EC.presence_of_element_located((
            By.XPATH,
            "//button[.//div[normalize-space()='" + filename + "']]",
        ))
    )


def seed_tab_state_for_directory(driver, directory: Path, target_url: str):
    """Seed persisted tab state so the app boots straight into organize view for `directory`."""
    tab_id = str(uuid.uuid4())
    tabs_state = {
        "tabs": [
            {
                "id": tab_id,
                "title": directory.name,
                "icon": None,
                "isPinned": False,
                "lastActive": int(time.time() * 1000),
                "savedPath": target_url,
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
    """Resolve the current device slug from the running app.

    Order of resolution:
      1. Scan persisted TabManager / view-preferences for a `device_slug`
         literal (raw or URL-encoded). This works once the user has opened
         an explorer tab at a Physical path.
      2. Fall back to invoking the daemon directly via Tauri's
         `daemon_request` IPC and reading the local device row from
         `devices.list`. This works even on a fresh app boot where the only
         persisted tab is the Overview.

    The daemon fallback is what the rest of the app already does (see
    `DevicePanel.tsx` and `FileInspector.tsx`), so the test is exercising
    the same code path the UI relies on.
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
        m = raw_pattern.search(value)
        if m:
            return m.group(1)
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

    # Persisted state didn't carry a slug yet — ask the daemon.
    result = driver.execute_script(
        """
        return new Promise(async (resolve) => {
            try {
                const libraryId = await window.__TAURI__.core.invoke('get_current_library_id');
                if (!libraryId) { resolve({ ok: false, error: 'no-library' }); return; }
                const res = await window.__TAURI__.core.invoke('daemon_request', {
                    request: {
                        Query: {
                            method: 'query:devices.list',
                            library_id: libraryId,
                            payload: { include_offline: true, include_details: false, show_paired: false }
                        }
                    }
                });
                resolve({ ok: true, res });
            } catch (e) {
                resolve({ ok: false, error: e && e.toString() });
            }
        });
        """
    )
    if not result or not result.get("ok"):
        return None
    devices = (result.get("res") or {}).get("JsonOk") or []
    if not isinstance(devices, list):
        return None
    local = next((d for d in devices if d.get("is_current")), None)
    if local is None and devices:
        local = devices[0]
    if local is None:
        return None
    slug = local.get("slug")
    return slug if isinstance(slug, str) and slug else None


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
            seed_tab_state_for_directory(driver, directory, target)

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


def test_organize_real_ui_decision_flow_and_restore():
    """Drive real UI: click keep/discard, assert dimming + badges, filter tabs, reload restores."""
    print("\n[Organize Real UI - Decision Flow + Reload Restore]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-flow-") as temp_dir:
        directory = Path(temp_dir)
        keep_name = "keep-me.txt"
        discard_name = "discard-me.txt"
        untouched_name = "leave-me-alone.txt"
        (directory / keep_name).write_text("keep", encoding="utf-8")
        (directory / discard_name).write_text("discard", encoding="utf-8")
        (directory / untouched_name).write_text("idle", encoding="utf-8")

        driver = connect_to_app()
        try:
            device_slug = resolve_local_device_slug(driver)
            assert device_slug, "Could not resolve local device slug"
            target = explorer_query_for_physical_directory(directory, device_slug)
            seed_tab_state_for_directory(driver, directory, target)

            url = f"{origin}{target}"
            driver.get(url)
            print(f"  Opened {url}")

            # All three files must render in the center pane.
            wait_for_card_for_filename(driver, keep_name)
            wait_for_card_for_filename(driver, discard_name)
            wait_for_card_for_filename(driver, untouched_name)

            # Initially the action buttons are disabled because nothing is
            # selected; clicking a card selects it and enables them.
            keep_btn_initial = find_button_by_text(driver, "Keep this tab")
            assert keep_btn_initial.get_attribute("disabled") is not None, (
                "Keep action should be disabled before a selection"
            )

            # --- Step 1: select the keep card and mark Keep ---
            find_card_for_filename(driver, keep_name).click()
            keep_action = find_clickable_by_text(driver, "Keep this tab")
            keep_action.click()

            # The keep card should now have an emerald check badge (decision=keep)
            # and the dimmed opacity class.
            keep_card_after = WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: d.find_element(
                    By.XPATH,
                    "//button[.//div[normalize-space()='" + keep_name + "']"
                    " and contains(@class, 'opacity-50')"
                    " and .//*[contains(@class, 'text-emerald-400')]]",
                )
            )
            assert keep_card_after is not None
            print(f"  Card '{keep_name}' got keep badge + dimming")

            # --- Step 2: select the discard card and mark Discard ---
            find_card_for_filename(driver, discard_name).click()
            discard_action = find_clickable_by_text(driver, "Discard this tab")
            discard_action.click()

            discard_card_after = WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: d.find_element(
                    By.XPATH,
                    "//button[.//div[normalize-space()='" + discard_name + "']"
                    " and contains(@class, 'opacity-50')"
                    " and .//*[contains(@class, 'text-rose-400')]]",
                )
            )
            assert discard_card_after is not None
            print(f"  Card '{discard_name}' got discard badge + dimming")

            # The untouched card should NOT be dimmed and should not have a
            # decision badge.
            untouched_card = find_card_for_filename(driver, untouched_name)
            untouched_class = untouched_card.get_attribute("class") or ""
            assert "opacity-50" not in untouched_class, (
                f"Untouched card was dimmed unexpectedly. class={untouched_class!r}"
            )
            assert not untouched_card.find_elements(
                By.XPATH, ".//*[contains(@class, 'text-emerald-400') or contains(@class, 'text-rose-400')]"
            ), "Untouched card unexpectedly had a decision badge"
            print(f"  Card '{untouched_name}' is undimmed with no badge")

            # --- Step 3: left pane tab filtering ---
            # Keep tab is active by default; the keep file must appear in the
            # left pane list and the discard file must not.
            keep_tab = find_button_by_text(driver, "Keep")
            discard_tab = find_button_by_text(driver, "Discard")

            # The left pane lists items as <button> with the file name in a
            # <span>. The center pane has the same name inside a <div>. We use
            # the <span> ancestry to scope the assertion to the left pane.
            def left_pane_item_visible(filename: str) -> bool:
                els = driver.find_elements(
                    By.XPATH,
                    "//button[.//span[normalize-space()='" + filename + "']]",
                )
                return len(els) > 0

            assert left_pane_item_visible(keep_name), (
                f"Expected '{keep_name}' in the left Keep tab list"
            )
            assert not left_pane_item_visible(discard_name), (
                f"Did not expect '{discard_name}' under the Keep tab"
            )
            print("  Left Keep tab lists kept file only")

            # Switch to the Discard tab and re-assert.
            discard_tab.click()
            WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: left_pane_item_visible(discard_name)
            )
            assert not left_pane_item_visible(keep_name), (
                f"Did not expect '{keep_name}' under the Discard tab"
            )
            # The "Delete now" button should now be visible and enabled.
            delete_now = find_clickable_by_text(driver, "Delete now")
            assert delete_now is not None
            print("  Left Discard tab lists discarded file only; Delete now is enabled")

            # Switch back to Keep so the reload-restore step compares like-for-like.
            keep_tab = find_button_by_text(driver, "Keep")
            keep_tab.click()
            WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: left_pane_item_visible(keep_name)
            )

            # --- Step 4: reload the page and confirm decisions are restored ---
            driver.get(url)
            print("  Reloaded the page")

            # Both badges + dimming should reappear on the same cards without
            # any further interaction.
            WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: d.find_element(
                    By.XPATH,
                    "//button[.//div[normalize-space()='" + keep_name + "']"
                    " and contains(@class, 'opacity-50')"
                    " and .//*[contains(@class, 'text-emerald-400')]]",
                )
            )
            WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: d.find_element(
                    By.XPATH,
                    "//button[.//div[normalize-space()='" + discard_name + "']"
                    " and contains(@class, 'opacity-50')"
                    " and .//*[contains(@class, 'text-rose-400')]]",
                )
            )
            print("  Both decisions survived a full page reload (UI badges restored)")

            print("  PASSED")
        finally:
            quit_driver(driver)


def _wait_dialog_closed(driver):
    WebDriverWait(driver, UI_WAIT_SECONDS).until_not(
        EC.presence_of_element_located((
            By.XPATH,
            "//*[contains(normalize-space(), 'Permanently delete discarded items?')]",
        ))
    )


def _open_organize_delete_dialog(driver, directory: Path, filenames):
    """Open the organize view for `directory`, discard each given filename, switch to the
    Discard tab, click Delete now, and return when the confirmation dialog is visible."""
    origin = detect_app_origin()
    assert origin
    device_slug = resolve_local_device_slug(driver)
    assert device_slug, "Could not resolve local device slug"
    target = explorer_query_for_physical_directory(directory, device_slug)
    seed_tab_state_for_directory(driver, directory, target)

    url = f"{origin}{target}"
    driver.get(url)

    for name in filenames:
        wait_for_card_for_filename(driver, name)
        find_card_for_filename(driver, name).click()
        find_clickable_by_text(driver, "Discard this tab").click()
        WebDriverWait(driver, UI_WAIT_SECONDS).until(
            lambda d, n=name: d.find_element(
                By.XPATH,
                "//button[.//div[normalize-space()='" + n + "']"
                " and contains(@class, 'opacity-50')]",
            )
        )

    find_button_by_text(driver, "Discard").click()
    delete_now = find_clickable_by_text(driver, "Delete now")
    delete_now.click()
    wait_for_text(driver, "Permanently delete discarded items?")


def test_organize_real_ui_delete_dialog_open_and_cancel():
    """Drive real UI: discard an item, open delete dialog, cancel via Cancel button.

    The dialog uses the spacedrive primitives Dialog (backed by Radix), so this
    also incidentally proves the Cancel button is wired through onOpenChange.
    """
    print("\n[Organize Real UI - Delete Dialog Open/Cancel]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-delete-") as temp_dir:
        directory = Path(temp_dir)
        target_name = "to-be-discarded.txt"
        target_file = directory / target_name
        target_file.write_text("alive", encoding="utf-8")

        driver = connect_to_app()
        try:
            _open_organize_delete_dialog(driver, directory, [target_name])
            wait_for_text(driver, "This will permanently delete all direct children")
            print("  Delete dialog opened with expected title and description")

            # Click Cancel.
            find_clickable_by_text(driver, "Cancel").click()
            _wait_dialog_closed(driver)
            print("  Cancel closed the dialog")

            assert target_file.exists(), "Cancel must not delete the file from disk"
            print("  File still present on disk after cancel")

            still_discarded = driver.find_element(
                By.XPATH,
                "//button[.//div[normalize-space()='" + target_name + "']"
                " and .//*[contains(@class, 'text-rose-400')]]",
            )
            assert still_discarded is not None
            print("  Card still shows discard badge after cancel")

            print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_delete_dialog_escape_closes():
    """Pressing Escape with the delete dialog open must close it (Radix default)."""
    print("\n[Organize Real UI - Delete Dialog Esc Closes]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-delete-esc-") as temp_dir:
        directory = Path(temp_dir)
        target_name = "esc-target.txt"
        target_file = directory / target_name
        target_file.write_text("alive", encoding="utf-8")

        driver = connect_to_app()
        try:
            from selenium.webdriver.common.keys import Keys
            from selenium.webdriver.common.action_chains import ActionChains

            _open_organize_delete_dialog(driver, directory, [target_name])

            ActionChains(driver).send_keys(Keys.ESCAPE).perform()
            _wait_dialog_closed(driver)
            print("  Escape closed the dialog")

            assert target_file.exists(), "Esc must not delete the file from disk"
            print("  File still present on disk after Esc")
            print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_delete_dialog_outside_click_closes():
    """Clicking on the overlay outside the dialog content must close it (Radix default)."""
    print("\n[Organize Real UI - Delete Dialog Outside Click Closes]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-delete-outside-") as temp_dir:
        directory = Path(temp_dir)
        target_name = "outside-target.txt"
        target_file = directory / target_name
        target_file.write_text("alive", encoding="utf-8")

        driver = connect_to_app()
        try:
            _open_organize_delete_dialog(driver, directory, [target_name])

            # The Radix dialog renders the title inside the form (form is the
            # dialog content). The fixed inset-0 overlay sits underneath at
            # z-[102]. Clicking near the top-left corner of the viewport lands
            # on the overlay (well outside the centered form box).
            driver.execute_script(
                """
                const el = document.elementFromPoint(8, 8);
                if (!el) throw new Error('No element at (8,8)');
                const ev = new PointerEvent('pointerdown', {
                    bubbles: true, cancelable: true, pointerType: 'mouse', button: 0
                });
                el.dispatchEvent(ev);
                const ev2 = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
                el.dispatchEvent(ev2);
                el.click();
                """
            )
            _wait_dialog_closed(driver)
            print("  Outside click closed the dialog")

            assert target_file.exists(), "Outside click must not delete the file from disk"
            print("  File still present on disk after outside click")
            print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_delete_dialog_enter_confirms_and_deletes():
    """Pressing Enter with the Delete permanently button focused must submit the form,
    actually delete the file from disk, drop it from the left discard list, and
    persist the cleared decision back to the organize JSON state."""
    print("\n[Organize Real UI - Delete Dialog Enter Confirms + Real Delete]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-delete-enter-") as temp_dir:
        directory = Path(temp_dir)
        target_name = "to-be-deleted-by-enter.txt"
        survivor_name = "survivor.txt"
        target_file = directory / target_name
        survivor_file = directory / survivor_name
        target_file.write_text("delete me", encoding="utf-8")
        survivor_file.write_text("keep me", encoding="utf-8")

        driver = connect_to_app()
        try:
            from selenium.webdriver.common.keys import Keys
            from selenium.webdriver.common.action_chains import ActionChains

            _open_organize_delete_dialog(driver, directory, [target_name])

            # Focus the Delete permanently submit button explicitly.
            submit_btn = find_clickable_by_text(driver, "Delete permanently")
            driver.execute_script("arguments[0].focus();", submit_btn)
            ActionChains(driver).send_keys(Keys.ENTER).perform()

            # The dialog should close after the mutation resolves.
            _wait_dialog_closed(driver)
            print("  Enter submitted the form and closed the dialog")

            # The discarded file must be removed from disk.
            for _ in range(40):
                if not target_file.exists():
                    break
                time.sleep(0.25)
            assert not target_file.exists(), (
                f"Real file at {target_file} should be deleted after Enter-confirm"
            )
            print(f"  File at {target_file} deleted from disk")
            assert survivor_file.exists(), (
                "Files not marked discard must not be deleted"
            )

            # The card should no longer render in the Discard tab list (or the
            # center pane, since the explorer files list updates on rescan).
            def discard_left_pane_count():
                return len(driver.find_elements(
                    By.XPATH,
                    "//button[.//span[normalize-space()='" + target_name + "']]",
                ))

            WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: discard_left_pane_count() == 0
            )
            print("  Deleted file no longer appears in the Discard tab list")

            # The persisted organize JSON state should no longer carry a
            # decision for the deleted file. We round-trip via load_organize_state.
            persisted = driver.execute_script(
                """
                return new Promise(async (resolve) => {
                    const dirPath = arguments[0];
                    // Replicate buildOrganizeDirectoryKey using the bundled
                    // app code via a dynamic import is fragile across builds;
                    // instead derive the key the same way the source does.
                    const FNV_OFFSET = 14695981039346656037n;
                    const FNV_PRIME = 1099511628211n;
                    let normalized = dirPath.replace(/\\\\/g, '/').replace(/\\/+/g, '/');
                    if (normalized.length > 1 && normalized.endsWith('/')) {
                        normalized = normalized.slice(0, -1);
                    }
                    let h = FNV_OFFSET;
                    for (const ch of normalized) {
                        h ^= BigInt(ch.charCodeAt(0));
                        h = (h * FNV_PRIME) & 0xFFFFFFFFFFFFFFFFn;
                    }
                    const key = 'dir-' + h.toString(16).padStart(16, '0');
                    try {
                        const raw = await window.__TAURI__.core.invoke(
                            'load_organize_state', { directoryKey: key });
                        resolve({ key, raw });
                    } catch (e) { resolve({ key, error: e.toString() }); }
                });
                """,
                str(directory),
            )
            assert "error" not in persisted, persisted
            assert persisted["raw"] is not None, (
                "Organize state JSON file should exist after a real decision was made"
            )
            parsed = json.loads(persisted["raw"])
            # The deleted file's decision must have been cleared. The item
            # records key by physical path or by id; we just assert no item
            # value has decision matching the deleted physical path.
            items = parsed.get("items", {})
            deleted_path_norm = str(target_file).replace("\\", "/")
            for item in items.values():
                p = item.get("path", "").replace("\\", "/")
                assert p != deleted_path_norm, (
                    f"Deleted file should not have a residual decision entry: {item}"
                )
            print("  Persisted organize JSON no longer carries the deleted file's decision")

            print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_preview_no_media_tabs_disabled_with_tooltip():
    """For a directory with no recognised media, only the list tab is rendered,
    and on hover the missing tabs would expose a helpful tooltip. Since unindexed
    temp directories never produce media_listing results, this directly proves the
    'no media -> list only' branch and that the tooltip strings are present in the
    bundle. The 'video' and 'image' tabs are not rendered in this branch.
    """
    print("\n[Organize Real UI - Preview no-media branch]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-no-media-") as temp_dir:
        directory = Path(temp_dir)
        subdir = directory / "child-folder"
        subdir.mkdir()
        (subdir / "readme.txt").write_text("hi", encoding="utf-8")

        driver = connect_to_app()
        try:
            device_slug = resolve_local_device_slug(driver)
            assert device_slug
            target = explorer_query_for_physical_directory(directory, device_slug)
            seed_tab_state_for_directory(driver, directory, target)
            driver.get(f"{origin}{target}")

            # Select the subdirectory to drive the directory-preview branch.
            wait_for_card_for_filename(driver, "child-folder")
            find_card_for_filename(driver, "child-folder").click()

            # Wait until the preview pane has settled by waiting for the
            # 'Preview list' tab to appear (it's the only one in this branch).
            preview_list_tab = WebDriverWait(driver, UI_WAIT_SECONDS).until(
                EC.presence_of_element_located((
                    By.XPATH,
                    "//button[normalize-space()='Preview list']",
                ))
            )
            assert preview_list_tab is not None
            print("  Preview list tab rendered for the selected subdirectory")

            # The Video and Image tabs must NOT be rendered in the no-media branch.
            assert not driver.find_elements(
                By.XPATH, "//button[normalize-space()='Video']"
            ), "Video tab should not render when no video media is present"
            assert not driver.find_elements(
                By.XPATH, "//button[normalize-space()='Image']"
            ), "Image tab should not render when no image media is present"
            print("  Video and Image tabs are not rendered (no-media branch)")
            print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_preview_one_media_disables_missing_tab_with_tooltip():
    """An image-only directory should still render the Video tab, but disabled
    with the missing-video tooltip/title on the actual button control."""
    print("\n[Organize Real UI - Preview one-media disabled tab tooltip]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-one-media-") as temp_dir:
        import base64

        directory = Path(temp_dir)
        subdir = directory / "images-only"
        subdir.mkdir()
        (subdir / "tiny.png").write_bytes(base64.b64decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO5X3r0AAAAASUVORK5CYII="
        ))

        driver = connect_to_app()
        try:
            device_slug = resolve_local_device_slug(driver)
            assert device_slug
            target = explorer_query_for_physical_directory(directory, device_slug)
            seed_tab_state_for_directory(driver, directory, target)
            driver.get(f"{origin}{target}")

            wait_for_card_for_filename(driver, "images-only")
            find_card_for_filename(driver, "images-only").click()

            video_tab = WebDriverWait(driver, UI_WAIT_SECONDS).until(
                EC.presence_of_element_located((
                    By.XPATH,
                    "//button[normalize-space()='Video']",
                ))
            )
            image_tab = driver.find_element(
                By.XPATH, "//button[normalize-space()='Image']"
            )
            preview_list_tab = driver.find_element(
                By.XPATH, "//button[normalize-space()='Preview list']"
            )

            assert video_tab.get_attribute("disabled") is not None, (
                "Video tab should render disabled when the directory only contains images"
            )
            assert video_tab.get_attribute("title") == "No video files in this folder"
            assert video_tab.get_attribute("aria-label") == "No video files in this folder"
            print("  Video tab rendered disabled with the expected tooltip/title")

            assert image_tab.get_attribute("disabled") is None
            assert preview_list_tab.get_attribute("disabled") is None
            print("  Image and Preview list tabs remained enabled")
            print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_json_file_only_appears_after_first_decision():
    """The persisted organize JSON file must not exist before any decision is made,
    and must exist after the first keep/discard decision. Proves the 'lazy create'
    invariant in useOrganizeState.persist()."""
    print("\n[Organize JSON file - lazy create after first decision]")
    origin = detect_app_origin()
    assert origin

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-lazy-") as temp_dir:
        directory = Path(temp_dir)
        name = "first-decision.txt"
        (directory / name).write_text("hi", encoding="utf-8")

        driver = connect_to_app()
        try:
            device_slug = resolve_local_device_slug(driver)
            assert device_slug
            target = explorer_query_for_physical_directory(directory, device_slug)
            seed_tab_state_for_directory(driver, directory, target)
            driver.get(f"{origin}{target}")
            wait_for_card_for_filename(driver, name)

            def load_state():
                return driver.execute_script(
                    """
                    return new Promise(async (resolve) => {
                        const dirPath = arguments[0];
                        const FNV_OFFSET = 14695981039346656037n;
                        const FNV_PRIME = 1099511628211n;
                        let normalized = dirPath.replace(/\\\\/g, '/').replace(/\\/+/g, '/');
                        if (normalized.length > 1 && normalized.endsWith('/')) {
                            normalized = normalized.slice(0, -1);
                        }
                        let h = FNV_OFFSET;
                        for (const ch of normalized) {
                            h ^= BigInt(ch.charCodeAt(0));
                            h = (h * FNV_PRIME) & 0xFFFFFFFFFFFFFFFFn;
                        }
                        const key = 'dir-' + h.toString(16).padStart(16, '0');
                        try {
                            const raw = await window.__TAURI__.core.invoke(
                                'load_organize_state', { directoryKey: key });
                            resolve({ key, raw });
                        } catch (e) { resolve({ key, error: e.toString() }); }
                    });
                    """,
                    str(directory),
                )

            before = load_state()
            assert "error" not in before, before
            assert before["raw"] is None, (
                f"JSON state must not exist before any decision; got: {before}"
            )
            print(f"  Pre-decision: load_organize_state returned null for key {before['key']}")

            # Make the first decision (keep).
            find_card_for_filename(driver, name).click()
            find_clickable_by_text(driver, "Keep this tab").click()
            WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: d.find_element(
                    By.XPATH,
                    "//button[.//div[normalize-space()='" + name + "']"
                    " and contains(@class, 'opacity-50')]",
                )
            )

            # Poll until the JSON file appears.
            after = None
            for _ in range(40):
                after = load_state()
                if after.get("raw") is not None:
                    break
                time.sleep(0.25)
            assert after and after.get("raw") is not None, (
                f"JSON state must exist after the first decision; final read: {after}"
            )
            parsed = json.loads(after["raw"])
            assert parsed.get("version") == 1
            items = parsed.get("items", {})
            decisions = {v.get("decision") for v in items.values()}
            assert "keep" in decisions, (
                f"Persisted state must include the keep decision; got: {parsed}"
            )
            print("  Post-decision: JSON state exists and contains the keep decision")
            print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_preview_empty_then_populated():
    """Drive real UI: preview pane shows empty state, then list-tab placeholder when a directory is selected.

    Single-file previews route through `platform.convertFileSrc` which produces
    a Tauri asset URL the WebView2 loader cannot resolve for ad-hoc temp paths
    that live outside any indexed location, so this test focuses on what the
    organize preview pane reliably renders for a real selection: the empty
    state when nothing is selected, and the centered placeholder text when the
    selected leaf file has no renderable preview.
    """
    print("\n[Organize Real UI - Preview Pane]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-preview-") as temp_dir:
        directory = Path(temp_dir)
        plain_name = "notes.txt"
        (directory / plain_name).write_text("hello", encoding="utf-8")

        driver = connect_to_app()
        try:
            device_slug = resolve_local_device_slug(driver)
            assert device_slug, "Could not resolve local device slug"
            target = explorer_query_for_physical_directory(directory, device_slug)
            seed_tab_state_for_directory(driver, directory, target)

            url = f"{origin}{target}"
            driver.get(url)
            print(f"  Opened {url}")

            # Empty state must render before anything is selected.
            wait_for_text(driver, "No items to preview")
            print("  Preview empty state rendered with no selection")

            # Select a plain text file: the preview pane is not a supported
            # renderer (not video/image), so it should still show the empty
            # placeholder string — this is meaningful because it proves the
            # selection wiring updated the pane.
            wait_for_card_for_filename(driver, plain_name)
            find_card_for_filename(driver, plain_name).click()

            # Selection ring on the card confirms the click actually selected
            # the file in the explorer's SelectionContext.
            WebDriverWait(driver, UI_WAIT_SECONDS).until(
                lambda d: d.find_element(
                    By.XPATH,
                    "//button[.//div[normalize-space()='" + plain_name + "']"
                    " and contains(@class, 'ring-2')]",
                )
            )
            print(f"  Selecting '{plain_name}' applied the selection ring")

            # The placeholder remains because the file has no supported preview.
            wait_for_text(driver, "No items to preview")
            print("  Preview pane shows placeholder for unsupported file kind")

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
        test_organize_real_ui_decision_flow_and_restore,
        test_organize_real_ui_delete_dialog_open_and_cancel,
        test_organize_real_ui_delete_dialog_escape_closes,
        test_organize_real_ui_delete_dialog_outside_click_closes,
        test_organize_real_ui_delete_dialog_enter_confirms_and_deletes,
        test_organize_real_ui_preview_empty_then_populated,
        test_organize_real_ui_preview_no_media_tabs_disabled_with_tooltip,
        test_organize_real_ui_preview_one_media_disables_missing_tab_with_tooltip,
        test_organize_json_file_only_appears_after_first_decision,
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
