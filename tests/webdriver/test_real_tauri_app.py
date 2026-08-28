"""
Real Tauri App WebDriver verification harness.

Connects to the actual running Spacedrive Tauri app via WebView2 DevTools and
verifies the recursive organize task UI and its file-system lifecycle at runtime.

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

import json
import base64
import shutil
import sys
import tempfile
import time
import urllib.request
import uuid
from contextlib import contextmanager
from pathlib import Path
from urllib.parse import quote, urlparse

from selenium import webdriver
from selenium.webdriver.edge.options import Options as EdgeOptions
from selenium.webdriver.common.by import By
from selenium.webdriver.support import expected_conditions as EC
from selenium.webdriver.support.ui import WebDriverWait
from selenium.common.exceptions import TimeoutException
from selenium.webdriver.common.action_chains import ActionChains


DEBUG_PORT = 9222
UI_WAIT_SECONDS = 20
REPO_ROOT = Path(__file__).resolve().parents[2]
FNV_OFFSET = 14695981039346656037
FNV_PRIME = 1099511628211
HARNESS_LOCAL_STORAGE_KEYS = (
    "sd-language",
    "sd-tabs-state",
    "sd-video-muted",
    "sd-video-volume",
)
ONE_PIXEL_PNG_BASE64 = (
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO5X3r0AAAAASUVORK5CYII="
)
VIDEO_FIXTURE_PATH = REPO_ROOT / "packages" / "assets" / "videos" / "SdIntro.mp4"

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


def count_center_pane_cards(driver, filename: str) -> int:
    return len(driver.find_elements(
        By.XPATH,
        "//button[.//div[normalize-space()='" + filename + "']]",
    ))


def left_pane_item_visible(driver, filename: str) -> bool:
    return len(driver.find_elements(
        By.XPATH,
        "//button[.//span[normalize-space()='" + filename + "']]",
    )) > 0


def capture_local_storage_keys(driver, keys=HARNESS_LOCAL_STORAGE_KEYS):
    return driver.execute_script(
        """
        const snapshot = {};
        for (const key of arguments[0]) {
            snapshot[key] = localStorage.getItem(key);
        }
        return snapshot;
        """,
        list(keys),
    )


def restore_local_storage_keys(driver, snapshot):
    driver.execute_script(
        """
        for (const [key, value] of Object.entries(arguments[0])) {
            if (value === null || value === undefined) {
                localStorage.removeItem(key);
            } else {
                localStorage.setItem(key, value);
            }
        }
        """,
        snapshot,
    )


@contextmanager
def preserved_local_storage_keys(driver, keys=HARNESS_LOCAL_STORAGE_KEYS):
    snapshot = capture_local_storage_keys(driver, keys)
    try:
        yield snapshot
    finally:
        restore_local_storage_keys(driver, snapshot)


def normalize_organize_path(path: Path | str) -> str:
    normalized = str(path).replace("\\", "/")
    while "//" in normalized:
        normalized = normalized.replace("//", "/")
    if len(normalized) > 1 and normalized.endswith("/"):
        normalized = normalized[:-1]
    return normalized


def build_organize_directory_key(path: Path | str) -> str:
    hash_value = FNV_OFFSET
    for ch in normalize_organize_path(path):
        hash_value ^= ord(ch)
        hash_value = (hash_value * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return f"dir-{hash_value:016x}"


def load_persisted_organize_state_by_key(driver, directory_key: str):
    result = driver.execute_script(LOAD_ORGANIZE_STATE_BY_KEY_SCRIPT, directory_key)
    assert "error" not in result, result
    raw = result.get("raw")
    return {
        **result,
        "parsed": json.loads(raw) if raw is not None else None,
    }


def delete_persisted_organize_state_by_key(driver, directory_key: str):
    result = driver.execute_script(DELETE_ORGANIZE_STATE_BY_KEY_SCRIPT, directory_key)
    assert "error" not in result, result
    assert result.get("raw") is None, (
        f"Organize state key {directory_key} should be removed during cleanup; got {result}"
    )
    return result


class OrganizeStateTracker:
    def __init__(self, driver):
        self.driver = driver
        self.directory_keys = set()

    def track_directory(self, directory: Path) -> str:
        return self.track_key(build_organize_directory_key(directory))

    def track_key(self, directory_key: str) -> str:
        self.directory_keys.add(directory_key)
        delete_persisted_organize_state_by_key(self.driver, directory_key)
        return directory_key

    def cleanup(self):
        for directory_key in sorted(self.directory_keys):
            delete_persisted_organize_state_by_key(self.driver, directory_key)


@contextmanager
def cleaned_organize_state_keys(driver, directory_keys=()):
    tracker = OrganizeStateTracker(driver)
    try:
        for directory_key in directory_keys:
            tracker.track_key(directory_key)
        yield tracker
    finally:
        tracker.cleanup()


@contextmanager
def preserved_harness_state(driver, directory: Path | None = None):
    with preserved_local_storage_keys(driver), cleaned_organize_state_keys(driver) as tracker:
        if directory is not None:
            tracker.track_directory(directory)
        yield tracker


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


def normalize_path_for_assertions(path: Path | str) -> str:
    return str(path).replace("\\", "/")


def load_persisted_organize_state(driver, directory: Path):
    return load_persisted_organize_state_by_key(
        driver, build_organize_directory_key(directory)
    )


def find_persisted_item_by_path(parsed_state, target_path: Path | str):
    if not parsed_state:
        return None
    target_norm = normalize_path_for_assertions(target_path)
    for item in parsed_state.get("items", {}).values():
        if normalize_path_for_assertions(item.get("path", "")) == target_norm:
            return item
    return None


def open_seeded_organize_view(driver, origin: str, directory: Path) -> str:
    device_slug = resolve_local_device_slug(driver)
    assert device_slug, (
        "Could not resolve local device slug from persisted app state. "
        "Open the explorer in the app at least once before running this test."
    )
    target = explorer_query_for_physical_directory(directory, device_slug)
    seed_tab_state_for_directory(driver, directory, target)
    url = f"{origin}{target}"
    driver.get(url)
    return url


def write_tiny_png(path: Path):
    path.write_bytes(base64.b64decode(ONE_PIXEL_PNG_BASE64))


def copy_video_fixture(path: Path):
    assert VIDEO_FIXTURE_PATH.exists(), f"Missing video fixture at {VIDEO_FIXTURE_PATH}"
    shutil.copy2(VIDEO_FIXTURE_PATH, path)


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
            with preserved_harness_state(driver, directory):
                url = open_seeded_organize_view(driver, origin, directory)
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
            with preserved_harness_state(driver, directory):
                url = open_seeded_organize_view(driver, origin, directory)
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

                untouched_card = find_card_for_filename(driver, untouched_name)
                untouched_class = untouched_card.get_attribute("class") or ""
                assert "opacity-50" not in untouched_class, (
                    f"Untouched card was dimmed unexpectedly. class={untouched_class!r}"
                )
                assert not untouched_card.find_elements(
                    By.XPATH, ".//*[contains(@class, 'text-emerald-400') or contains(@class, 'text-rose-400')]"
                ), "Untouched card unexpectedly had a decision badge"
                print(f"  Card '{untouched_name}' is undimmed with no badge")

                keep_tab = find_button_by_text(driver, "Keep")
                discard_tab = find_button_by_text(driver, "Discard")

                assert left_pane_item_visible(driver, keep_name), (
                    f"Expected '{keep_name}' in the left Keep tab list"
                )
                assert not left_pane_item_visible(driver, discard_name), (
                    f"Did not expect '{discard_name}' under the Keep tab"
                )
                print("  Left Keep tab lists kept file only")

                discard_tab.click()
                WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    lambda d: left_pane_item_visible(d, discard_name)
                )
                assert not left_pane_item_visible(driver, keep_name), (
                    f"Did not expect '{keep_name}' under the Discard tab"
                )
                delete_now = find_clickable_by_text(driver, "Delete now")
                assert delete_now is not None
                print("  Left Discard tab lists discarded file only; Delete now is enabled")

                find_button_by_text(driver, "Keep").click()
                WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    lambda d: left_pane_item_visible(d, keep_name)
                )

                driver.get(url)
                print("  Reloaded the page")

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
    open_seeded_organize_view(driver, origin, directory)

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
            with preserved_harness_state(driver, directory):
                _open_organize_delete_dialog(driver, directory, [target_name])
                wait_for_text(driver, "This will permanently delete all direct children")
                print("  Delete dialog opened with expected title and description")

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

                persisted = load_persisted_organize_state(driver, directory)
                assert persisted["parsed"] is not None, (
                    "Persisted organize JSON should still exist after cancel"
                )
                discarded_item = find_persisted_item_by_path(
                    persisted["parsed"], target_file
                )
                assert discarded_item is not None, (
                    "Cancel should preserve the persisted discard decision"
                )
                assert discarded_item.get("decision") == "discard", discarded_item
                print("  Persisted organize JSON still carries the discard decision after cancel")

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

            with preserved_harness_state(driver, directory):
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
            with preserved_harness_state(driver, directory):
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

            with preserved_harness_state(driver, directory):
                _open_organize_delete_dialog(driver, directory, [target_name])

                submit_btn = find_clickable_by_text(driver, "Delete permanently")
                driver.execute_script("arguments[0].focus();", submit_btn)
                ActionChains(driver).send_keys(Keys.ENTER).perform()

                _wait_dialog_closed(driver)
                print("  Enter submitted the form and closed the dialog")

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

                WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    lambda d: not left_pane_item_visible(d, target_name)
                )
                print("  Deleted file no longer appears in the Discard tab list")

                WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    lambda d: count_center_pane_cards(d, target_name) == 0
                )
                assert count_center_pane_cards(driver, target_name) == 0, (
                    "Deleted file should be removed from the center pane"
                )
                assert count_center_pane_cards(driver, survivor_name) == 1, (
                    "Surviving file should still render in the center pane"
                )
                print("  Center pane no longer shows the deleted card or badge")

                persisted = load_persisted_organize_state(driver, directory)
                assert persisted["parsed"] is not None, (
                    "Organize state JSON file should exist after a real decision was made"
                )
                deleted_item = find_persisted_item_by_path(
                    persisted["parsed"], target_file
                )
                assert deleted_item is None, (
                    "Deleted file should not keep a persisted decision entry"
                )
                print("  Persisted organize JSON no longer carries the deleted file's decision")

                print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_preview_no_media_renders_list_only():
    """For a directory with no recognised media, only the list tab is rendered.

    This proves the real Tauri app takes the 'no media -> list only' branch for
    a selected directory. It does not claim anything about disabled missing tabs
    because those controls are not rendered in this branch.
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
            with preserved_harness_state(driver, directory):
                open_seeded_organize_view(driver, origin, directory)

                wait_for_card_for_filename(driver, "child-folder")
                find_card_for_filename(driver, "child-folder").click()

                preview_list_tab = WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    EC.presence_of_element_located((
                        By.XPATH,
                        "//button[normalize-space()='Preview list']",
                    ))
                )
                assert preview_list_tab is not None
                print("  Preview list tab rendered for the selected subdirectory")

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


def test_organize_real_ui_preview_one_media_disables_missing_tab_with_title():
    """An image-only directory should still render the Video tab, but disabled
    with the missing-video title on the actual button control."""
    print("\n[Organize Real UI - Preview one-media disabled tab title]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-one-media-") as temp_dir:
        directory = Path(temp_dir)
        subdir = directory / "images-only"
        subdir.mkdir()
        write_tiny_png(subdir / "tiny.png")

        driver = connect_to_app()
        try:
            with preserved_harness_state(driver, directory):
                open_seeded_organize_view(driver, origin, directory)

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
                print("  Video tab rendered disabled with the expected title")

                assert image_tab.get_attribute("disabled") is None
                assert preview_list_tab.get_attribute("disabled") is None
                print("  Image and Preview list tabs remained enabled")
                print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_preview_mixed_media_prefers_video_tab():
    """A directory containing both video and image media should default to the
    Video tab and render the video preview first."""
    print("\n[Organize Real UI - Preview mixed-media priority]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-mixed-media-") as temp_dir:
        directory = Path(temp_dir)
        subdir = directory / "mixed-media"
        subdir.mkdir()
        copy_video_fixture(subdir / "clip.mp4")
        write_tiny_png(subdir / "tiny.png")

        driver = connect_to_app()
        try:
            with preserved_harness_state(driver, directory):
                open_seeded_organize_view(driver, origin, directory)

                wait_for_card_for_filename(driver, "mixed-media")
                find_card_for_filename(driver, "mixed-media").click()

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

                assert video_tab.get_attribute("disabled") is None
                assert image_tab.get_attribute("disabled") is None
                assert preview_list_tab.get_attribute("disabled") is None
                assert "bg-accent/15" in (video_tab.get_attribute("class") or ""), (
                    "Video tab should be active by default when both media kinds exist"
                )
                assert "bg-accent/15" not in (image_tab.get_attribute("class") or ""), (
                    "Image tab should not be the default active tab when video exists"
                )

                preview_video = WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    lambda d: d.find_element(By.TAG_NAME, "video")
                )
                assert preview_video.get_attribute("autoplay") == "true"
                print("  Mixed-media directory defaulted to the Video tab and rendered a video preview")
                print("  PASSED")
        finally:
            quit_driver(driver)


def test_organize_real_ui_preview_single_video_uses_saved_audio_prefs():
    """Selecting a real video file should render the single-file video preview,
    preserve autoplay, and hydrate muted/volume from localStorage."""
    print("\n[Organize Real UI - Preview single-file video]")
    origin = detect_app_origin()
    assert origin, "Could not detect app origin"

    with tempfile.TemporaryDirectory(prefix="spacedrive-organize-video-file-") as temp_dir:
        directory = Path(temp_dir)
        copy_video_fixture(directory / "clip.mp4")

        driver = connect_to_app()
        try:
            with preserved_harness_state(driver, directory):
                device_slug = resolve_local_device_slug(driver)
                assert device_slug, "Could not resolve local device slug"
                target = explorer_query_for_physical_directory(directory, device_slug)
                seed_tab_state_for_directory(driver, directory, target)
                driver.execute_script(
                    "localStorage.setItem('sd-video-muted', 'true');"
                    "localStorage.setItem('sd-video-volume', '0.25');"
                )
                url = f"{origin}{target}"
                driver.get(url)
                print(f"  Opened {url}")

                wait_for_card_for_filename(driver, "clip.mp4")
                find_card_for_filename(driver, "clip.mp4").click()

                preview_video = WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    lambda d: d.find_element(By.TAG_NAME, "video")
                )
                state = driver.execute_script(
                    """
                    const video = document.querySelector('video');
                    return {
                        muted: video ? video.muted : null,
                        volume: video ? video.volume : null,
                        localMuted: localStorage.getItem('sd-video-muted'),
                        localVolume: localStorage.getItem('sd-video-volume')
                    };
                    """
                )
                assert preview_video.get_attribute("autoplay") == "true"
                assert state["muted"] is True, state
                assert abs(state["volume"] - 0.25) < 0.001, state
                assert state["localMuted"] == "true", state
                assert state["localVolume"] == "0.25", state
                print("  Single-file video preview rendered with autoplay and preserved audio prefs")
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
            with preserved_harness_state(driver, directory):
                open_seeded_organize_view(driver, origin, directory)
                wait_for_card_for_filename(driver, name)

                before = load_persisted_organize_state(driver, directory)
                assert before["raw"] is None, (
                    f"JSON state must not exist before any decision; got: {before}"
                )
                print(f"  Pre-decision: load_organize_state returned null for key {before['key']}")

                find_card_for_filename(driver, name).click()
                find_clickable_by_text(driver, "Keep this tab").click()
                WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    lambda d: d.find_element(
                        By.XPATH,
                        "//button[.//div[normalize-space()='" + name + "']"
                        " and contains(@class, 'opacity-50')]",
                    )
                )

                after = None
                for _ in range(40):
                    after = load_persisted_organize_state(driver, directory)
                    if after.get("raw") is not None:
                        break
                    time.sleep(0.25)
                assert after and after.get("raw") is not None, (
                    f"JSON state must exist after the first decision; final read: {after}"
                )
                parsed = after["parsed"]
                assert parsed is not None
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
            with preserved_harness_state(driver, directory):
                url = open_seeded_organize_view(driver, origin, directory)
                print(f"  Opened {url}")

                wait_for_text(driver, "No items to preview")
                print("  Preview empty state rendered with no selection")

                # Select a plain text file: the preview pane is not a supported
                # renderer (not video/image), so it should still show the empty
                # placeholder string. This proves the selection wiring updated
                # the pane without requiring asset-backed media rendering.
                wait_for_card_for_filename(driver, plain_name)
                find_card_for_filename(driver, plain_name).click()

                WebDriverWait(driver, UI_WAIT_SECONDS).until(
                    lambda d: d.find_element(
                        By.XPATH,
                        "//button[.//div[normalize-space()='" + plain_name + "']"
                        " and contains(@class, 'ring-2')]",
                    )
                )
                print(f"  Selecting '{plain_name}' applied the selection ring")

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
        with cleaned_organize_state_keys(driver, ("webdriver-e2e-test",)):
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
        with cleaned_organize_state_keys(driver, ("webdriver-structure-test",)):
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


def test_recursive_organize_task_vertical_flow():
    """Exercise the real /organize/:taskId workflow without private invokes or seeded view state."""
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
        assert len(inputs) >= 2, "Expected device and Windows folder inputs on the real task entry page"
        inputs[0].clear(); inputs[0].send_keys("local")
        inputs[1].send_keys(str(root))
        find_clickable_by_text(driver, "Start scan").click()
        WebDriverWait(driver, UI_WAIT_SECONDS).until(lambda d: "/organize/" in d.current_url)
        wait_for_text(driver, "direct children")
        wait_for_text(driver, "nested")
        assert "organize/" in driver.current_url

        # Double-clicking the real directory card is the recursive navigation contract.
        nested_card = driver.find_element(By.XPATH, "//button[.//span[normalize-space()='nested']]")
        ActionChains(driver).double_click(nested_card).perform()
        wait_for_text(driver, "discard.txt")
        assert "/organize/" in driver.current_url

        # Mark one item in each decision class through the visible decision bar.
        driver.find_element(By.XPATH, "//button[.//span[normalize-space()='discard.txt']]").click()
        find_clickable_by_text(driver, "Discard").click()
        driver.find_element(By.XPATH, "//button[.//span[normalize-space()='move.txt']]").click()
        find_clickable_by_text(driver, "Move…").click()
        move_input = driver.find_element(By.CSS_SELECTOR, "input[placeholder='C:\\Sorted\\Keep']")
        move_input.send_keys(str(destination))
        find_clickable_by_text(driver, "Set destination").click()

        # Reload proves task/revision persistence, while the physical files remain untouched before commit.
        driver.refresh()
        wait_for_text(driver, "discard.txt")
        assert keep.exists() and discard.exists() and moved.exists()

        # Finish is read-only with respect to files, and completed tasks expose Reopen instead of decisions.
        find_clickable_by_text(driver, "Finish").click()
        WebDriverWait(driver, UI_WAIT_SECONDS).until(lambda d: len(d.find_elements(By.XPATH, "//button[normalize-space()='Reopen']")) == 1)
        assert keep.exists() and discard.exists() and moved.exists()
        assert driver.find_element(By.XPATH, "//button[normalize-space()='Keep']").get_attribute("disabled") is not None
        find_clickable_by_text(driver, "Reopen").click()
        WebDriverWait(driver, UI_WAIT_SECONDS).until(lambda d: len(d.find_elements(By.XPATH, "//button[normalize-space()='Finish']")) == 1)

        # Drift is surfaced in Review commit and does not move or delete anything before confirmation.
        drift = nested / "drift.txt"
        drift.write_text("external", encoding="utf-8")
        find_clickable_by_text(driver, "Scan changes").click()
        find_clickable_by_text(driver, "Review commit").click()
        wait_for_text(driver, "changed")
        assert keep.exists() and discard.exists() and moved.exists() and drift.exists()
        print("  Created recursive task, navigated nested children, persisted decisions, completed/reopened, and blocked drift without side effects")
        print("  PASSED")
    finally:
        quit_driver(driver)
        shutil.rmtree(root, ignore_errors=True)


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
        test_recursive_organize_task_vertical_flow,
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
