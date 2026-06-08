@echo off
REM Spacedrive Development Startup Script (Windows Batch)
REM Simple wrapper that calls PowerShell script

echo Starting Spacedrive Development...
echo.

PowerShell -ExecutionPolicy Bypass -File "%~dp0start-dev.ps1"

if errorlevel 1 (
    echo.
    echo Failed to start. Press any key to exit...
    pause >nul
    exit /b 1
)
