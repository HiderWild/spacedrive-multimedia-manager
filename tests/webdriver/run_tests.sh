#!/bin/bash
# Run WebDriver tests for Spacedrive organize view
#
# Usage:
#   ./tests/webdriver/run_tests.sh [--headless] [--tauri-app]
#
# Options:
#   --headless    Run Edge in headless mode
#   --tauri-app   Run tests against the actual Tauri app (requires tauri-driver)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "============================================================"
echo "Spacedrive Organize View - WebDriver Test Runner"
echo "============================================================"
echo ""
echo "Root: $ROOT_DIR"
echo ""

# Check prerequisites
echo "Checking prerequisites..."

# Check msedgedriver
if command -v msedgedriver &> /dev/null; then
    echo "  ✓ msedgedriver found"
else
    echo "  ⚠ msedgedriver not in PATH (Selenium will auto-download)"
fi

# Check tauri-driver
TAURI_DRIVER="$HOME/.cargo/bin/tauri-driver"
if [ -f "$TAURI_DRIVER.exe" ] || [ -f "$TAURI_DRIVER" ]; then
    echo "  ✓ tauri-driver found"
else
    echo "  ✗ tauri-driver not found (install: cargo install tauri-driver)"
fi

# Check Python Selenium
if python -c "import selenium" 2>/dev/null; then
    echo "  ✓ Python Selenium available"
else
    echo "  ✗ Python Selenium not found (install: pip install selenium)"
fi

echo ""

# Run Python tests
echo "Running Python WebDriver tests..."
echo "------------------------------------------------------------"
cd "$ROOT_DIR"
python tests/webdriver/test_organize_view.py "$@"
PYTHON_EXIT=$?
echo ""

# Run Node.js tests if available
if [ -f "$SCRIPT_DIR/node_modules/selenium-webdriver/index.js" ]; then
    echo "Running Node.js WebDriver tests..."
    echo "------------------------------------------------------------"
    node tests/webdriver/test_organize_webdriver.mjs "$@"
    NODE_EXIT=$?
    echo ""
else
    echo "Skipping Node.js tests (selenium-webdriver not installed locally)"
    NODE_EXIT=0
fi

# Run Tauri app tests if requested
if [[ "$*" == *"--tauri-app"* ]]; then
    echo "Running Tauri App WebDriver tests..."
    echo "------------------------------------------------------------"
    python tests/webdriver/test_tauri_app.py "$@"
    TAURI_EXIT=$?
    echo ""
else
    echo "Skipping Tauri app tests (use --tauri-app to enable)"
    TAURI_EXIT=0
fi

# Summary
echo "============================================================"
echo "Test Summary"
echo "============================================================"
echo "  Python tests:    $([ $PYTHON_EXIT -eq 0 ] && echo 'PASSED' || echo 'FAILED')"
echo "  Node.js tests:   $([ $NODE_EXIT -eq 0 ] && echo 'PASSED' || echo 'FAILED/SKIPPED')"
echo "  Tauri app tests: $([ $TAURI_EXIT -eq 0 ] && echo 'PASSED/SKIPPED' || echo 'FAILED')"
echo "============================================================"

# Exit with failure if any test failed
if [ $PYTHON_EXIT -ne 0 ] || [ $NODE_EXIT -ne 0 ] || [ $TAURI_EXIT -ne 0 ]; then
    exit 1
fi

exit 0
