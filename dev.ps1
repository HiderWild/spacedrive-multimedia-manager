# Quick Spacedrive Dev Startup
# Usage: Just double-click or run ./dev.ps1

# Kill old processes
Get-Process node,bun,vite,cargo,sd-daemon,spacedrive -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

# Kill processes on dev ports
@(5173, 3000, 8080, 1420) | ForEach-Object {
    Get-NetTCPConnection -LocalPort $_ -ErrorAction SilentlyContinue |
    ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }
}

# Start dev server
Write-Host "Starting Spacedrive..." -ForegroundColor Cyan
bun run --filter @sd/tauri dev
