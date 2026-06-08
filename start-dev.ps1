# Spacedrive Development Startup Script
# Optimized for quick restarts by killing old processes first

Write-Host "🚀 Starting Spacedrive Development..." -ForegroundColor Cyan
Write-Host ""

# Step 1: Kill all old processes
Write-Host "🧹 Cleaning up old processes..." -ForegroundColor Yellow

# Kill Node/Bun processes
$processesToKill = @(
    "node",
    "bun",
    "vite",
    "cargo",
    "rust-analyzer",
    "sd-daemon",
    "spacedrive"
)

foreach ($procName in $processesToKill) {
    $processes = Get-Process -Name $procName -ErrorAction SilentlyContinue
    if ($processes) {
        Write-Host "  Killing $($processes.Count) $procName process(es)..." -ForegroundColor Gray
        $processes | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 100
    }
}

# Kill any processes using common dev ports
$ports = @(5173, 3000, 8080, 1420)
foreach ($port in $ports) {
    $connections = Get-NetTCPConnection -LocalPort $port -ErrorAction SilentlyContinue
    if ($connections) {
        foreach ($conn in $connections) {
            $proc = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
            if ($proc) {
                Write-Host "  Killing process on port ${port}: $($proc.Name)" -ForegroundColor Gray
                Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            }
        }
    }
}

Write-Host "✓ Cleanup complete" -ForegroundColor Green
Write-Host ""

# Step 2: Start development server
Write-Host "🔧 Starting Tauri development server..." -ForegroundColor Cyan
Write-Host ""

# Navigate to project root (if script is run from subdirectory)
if (Test-Path "apps/tauri") {
    # Already in project root
} elseif (Test-Path "../apps/tauri") {
    Set-Location ..
} elseif (Test-Path "../../apps/tauri") {
    Set-Location ../..
}

# Start the dev server
try {
    bun run --filter @sd/tauri dev
} catch {
    Write-Host ""
    Write-Host "❌ Error starting development server" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}
