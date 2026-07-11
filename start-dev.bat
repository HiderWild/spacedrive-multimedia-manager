@echo off
REM Compatibility wrapper for start.ps1
REM Default is formal RELEASE instance (no installer). Use start.ps1 -Dev for debug hot reload.

echo Starting Spacedrive (default: release formal instance)...
echo.

PowerShell -ExecutionPolicy Bypass -File "%~dp0start.ps1" %*

if errorlevel 1 (
    echo.
    echo Failed to start. Press any key to exit...
    pause >nul
    exit /b 1
)
