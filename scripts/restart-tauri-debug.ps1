param(
    [string] $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string] $Cargo = "",
    [string] $Bun = "",
    [ValidateSet("Debug", "Release")]
    [string] $BuildProfile = "Debug",
    [switch] $SkipRebuild
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$tauriPort = 1420
$daemonPort = 6969

function Resolve-BuildTool {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [string] $PreferredPath = ""
    )

    if ($PreferredPath -and (Test-Path $PreferredPath)) {
        return (Resolve-Path $PreferredPath).Path
    }

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    if ($Name -ieq "cargo") {
        $candidates = @()
        if ($env:CARGO_HOME) {
            $candidates += Join-Path (Join-Path $env:CARGO_HOME "bin") "cargo.exe"
        }
        $candidates += Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
        $toolchains = Join-Path $env:USERPROFILE ".rustup\toolchains"
        if (Test-Path $toolchains) {
            $candidates += Get-ChildItem -Path $toolchains -Directory -ErrorAction SilentlyContinue |
                Where-Object { Test-Path (Join-Path $_.FullName "bin\cargo.exe") } |
                ForEach-Object { Join-Path $_.FullName "bin\cargo.exe" }
        }
        foreach ($candidate in $candidates) {
            if (Test-Path $candidate) {
                return (Resolve-Path $candidate).Path
            }
        }
    }

    if ($Name -ieq "bun") {
        if ($env:BUN_INSTALL) {
            $candidate = Join-Path (Join-Path $env:BUN_INSTALL "bin") "bun.exe"
            if (Test-Path $candidate) {
                return (Resolve-Path $candidate).Path
            }
        }
    }

    throw "$Name not found. Use -Cargo/-Bun to specify the full path."
}

function Stop-ProcessTree {
    param([int[]]$ProcessIds)
    $ids = @($ProcessIds | Sort-Object -Unique)
    foreach ($id in $ids) {
        try {
            taskkill /PID $id /T /F | Out-Null
            Write-Host "Stopped PID $id"
        } catch {
            # process may have already exited
        }
    }
}

function Stop-ByCommandLine {
    param([string]$ProjectPath)

    $escapedPath = [regex]::Escape((Resolve-Path $ProjectPath).Path)
    $candidates = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue

    $target = @()
    foreach ($proc in $candidates) {
        if (-not $proc.CommandLine) {
            continue
        }
        if ($proc.ProcessId -eq $PID) {
            continue
        }

        # Backend and Tauri related
        if ($proc.Name -in @("sd-daemon.exe","sd-daemon","Spacedrive.exe","Spacedrive","tauri.exe","bun.exe","node.exe","cargo.exe")) {
            if ($proc.CommandLine -match $escapedPath -or $proc.Name -in @("sd-daemon.exe","sd-daemon","Spacedrive.exe","Spacedrive")) {
                $target += $proc
                continue
            }
        }
        $cliRe = 'bun run tauri:dev|bun run dev:with-daemon|tauri dev|vite dev|@tauri-apps\\cli\\tauri'
        if ($proc.CommandLine -match $escapedPath -and $proc.CommandLine -match $cliRe) {
            $target += $proc
        }
    }

    if ($target) {
        $target | Select-Object ProcessId, Name, CommandLine | Format-Table -AutoSize | Out-String | Write-Host
        Stop-ProcessTree -ProcessIds $target.ProcessId
    } else {
        Write-Host "No matching debug processes found."
    }
}

function Wait-ProcessExit {
    param([int[]]$ProcessIds, [int]$TimeoutSec = 8)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $running = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object { $ProcessIds -contains $_.ProcessId }
        if (-not $running) {
            return
        }
        Start-Sleep -Milliseconds 500
    }
    Stop-ProcessTree -ProcessIds $ProcessIds
}

function Stop-PortListeners {
    param([int]$Port)
    try {
        $listeners = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    } catch {
        return
    }
    if (-not $listeners) { return }

    $pids = @($listeners | Select-Object -ExpandProperty OwningProcess | Sort-Object -Unique)
    Write-Host "Stopping processes listening on port $Port : $($pids -join ',')"
    Stop-ProcessTree -ProcessIds $pids
    Wait-ProcessExit -ProcessIds $pids -TimeoutSec 6
}

$repo = Resolve-Path $RepoRoot | Select-Object -ExpandProperty Path
Write-Host "Starting cleanup for project: $repo"

# 1) Clean up old processes
Stop-ByCommandLine -ProjectPath $repo

# 2) Clean up critical ports so tauri can load the webview address
Stop-PortListeners -Port $tauriPort

# Daemon port may not be cleaned up yet in rare cases
Stop-PortListeners -Port $daemonPort

# 3) Resolve build tools and ensure tauri subprocesses can find cargo/bun
$cargoCmd = Resolve-BuildTool -Name "cargo" -PreferredPath $Cargo
$bunCmd = Resolve-BuildTool -Name "bun" -PreferredPath $Bun

$env:PATH = "$(Split-Path $cargoCmd);$(Split-Path $bunCmd);$env:PATH"

# 4) Rebuild daemon (if needed)
Set-Location $repo
if (-not $SkipRebuild) {
    $buildArgs = @(
        "build",
        "--manifest-path", (Join-Path $repo "Cargo.toml"),
        "--bin", "sd-daemon"
    )
    if ($BuildProfile -eq "Release") {
        $buildArgs += "--release"
    }
    Write-Host "Rebuilding sd-daemon (${BuildProfile})..."
    & $cargoCmd @buildArgs
}

# 5) Start Tauri debug (native window, not web UI)
$env:HOST = "127.0.0.1"
$tauriDir = Join-Path $repo "apps\tauri"
Set-Location $tauriDir
Write-Host "Starting Tauri debug window: bun run tauri:dev"
& $bunCmd run tauri:dev
