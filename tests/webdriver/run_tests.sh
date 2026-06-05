#!/bin/bash
# Run WebDriver verification for the Spacedrive organize view.
#
# Usage:
#   ./tests/webdriver/run_tests.sh
#
# Prerequisites:
#   - Tauri app built and running with WebView2 remote debugging on port 9222
#   - Python Selenium installed (pip install selenium)
#
# Launch the app first:
#   set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
#   target/debug/Spacedrive.exe

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "============================================================"
echo "Spacedrive Organize View - WebDriver Verification"
echo "============================================================"
echo ""

cd "$ROOT_DIR"
python tests/webdriver/test_real_tauri_app.py "$@"
