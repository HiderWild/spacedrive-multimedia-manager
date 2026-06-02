# Restart Spacedrive debug environment (backend daemon + Tauri desktop app).
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

function Stop-ProcessTree {
    param([int[]]$ProcessIds)

    foreach ($id in ($ProcessIds | Sort-Object -Unique)) {
        try {
            Write-Host "Stopping PID $id..."
            taskkill.exe /PID $id /T /F | Out-Null
        } catch {
            Write-Host "Skip stop PID ${id}: $($_.Exception.Message)"
        }
    }
}

function Get-RepoProcessCandidates {
    param([string]$ProjectPath)

    $projectEscaped = [Regex]::Escape($ProjectPath)
    $projectEscaped = $projectEscaped.TrimEnd("\\")
    $processes = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue

    $tauriDirEscaped = [Regex]::Escape((Join-Path $ProjectPath "apps\tauri"))
    $webDirEscaped = [Regex]::Escape((Join-Path $ProjectPath "apps\web"))
    $daemonNamePatterns = @("sd-daemon.exe", "sd-daemon", "Spacedrive.exe", "Spacedrive", "sd-desktop.exe", "sd-desktop")
    $scriptRunnerNames = @("bun.exe", "node.exe", "cargo.exe", "rustc.exe", "pnpm.exe")
    $cliPattern = 'bun run tauri:dev|bun run dev:with-daemon|bun run tauri|@tauri-apps\\cli\\tauri|tauri dev|sd-daemon|cargo run --bin sd-daemon|cargo build .*--bin sd-daemon|cargo build .* --bin sd-daemon|vite dev|bun run dev'

    $results = @()

    foreach ($proc in $processes) {
        if ($proc.ProcessId -eq $PID -or -not $proc.CommandLine) {
            continue
        }

        $name = $proc.Name
        $cmd = $proc.CommandLine

        if ($daemonNamePatterns -contains $name) {
            $results += $proc
            continue
        }

        if ($name -in $scriptRunnerNames -and $cmd -match $projectEscaped -and ($cmd -match $cliPattern)) {
            $results += $proc
            continue
        }

        if (
            $cmd -match $projectEscaped -and (
                ($cmd -match $tauriDirEscaped -and $cmd -match "bun run tauri:dev|tauri dev|dev:with-daemon|@tauri-apps\\cli\\tauri") -or
                ($cmd -match $webDirEscaped -and $cmd -match "bun run dev|vite")
            )
        ) {
            $results += $proc
        }
    }

    return $results
}

function Wait-ForStop {
    param([int[]]$ProcessIds, [int]$TimeoutSec = 8)

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $running = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object { $ProcessIds -contains $_.ProcessId }
        if (-not $running) {
            return
        }
        Start-Sleep -Seconds 1
    }

    Write-Host "Some old debug processes still running; forcing final stop..."
    Stop-ProcessTree -ProcessIds $ProcessIds
}

function Stop-ProcessOnPort {
    param([int]$Port)

    try {
        $listeners = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    } catch {
        Write-Host "Unable to query listeners on port ${Port}: $($_.Exception.Message)"
        return
    }

    if (-not $listeners) {
        return
    }

    $processIds = @($listeners | Select-Object -ExpandProperty OwningProcess | Sort-Object -Unique)
    if ($processIds.Count -gt 0) {
        Write-Host "Port ${Port} is used by PIDs: $($processIds -join ', '). Stopping them."
        Stop-ProcessTree -ProcessIds $processIds
        $deadline = (Get-Date).AddSeconds(8)
        while ((Get-Date) -lt $deadline) {
            $running = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
                Where-Object { $processIds -contains $_.ProcessId }
            if (-not $running) {
                return
            }
            Start-Sleep -Seconds 1
        }
        Write-Host "Some listeners still running after timeout on port ${Port}."
    }
}

function Resolve-BuildTool {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [string] $PreferredPath = ""
    )

    if ($PreferredPath) {
        if (Test-Path $PreferredPath) {
            return (Resolve-Path $PreferredPath).Path
        }
        Write-Host "Specified path for '$Name' not found: $PreferredPath"
    }

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    if ($Name -ieq "cargo") {
        $cargoHome = $env:CARGO_HOME
        $candidatePaths = @()
        if ($cargoHome) {
            $candidatePaths += Join-Path (Join-Path $cargoHome "bin") "cargo.exe"
        }
        $candidatePaths += Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
        $rustupToolchains = Join-Path $env:USERPROFILE ".rustup\\toolchains"
        if (Test-Path $rustupToolchains) {
            $candidatePaths += Get-ChildItem -Path $rustupToolchains -Directory -ErrorAction SilentlyContinue |
                Where-Object { Test-Path (Join-Path $_.FullName "bin\cargo.exe") } |
                Sort-Object -Property Name |
                ForEach-Object { Join-Path $_.FullName "bin\cargo.exe" }
        }

        foreach ($candidate in $candidatePaths) {
            if ($candidate -and (Test-Path $candidate)) {
                return (Resolve-Path $candidate).Path
            }
        }
    }

    if ($Name -ieq "bun") {
        $bunInstall = $env:BUN_INSTALL
        if ($bunInstall) {
            $candidate = Join-Path (Join-Path $bunInstall "bin") "bun.exe"
            if (Test-Path $candidate) {
                return (Resolve-Path $candidate).Path
            }
        }
    }

    Write-Host "Could not find '$Name' in PATH."
    return ""
}

$project = Resolve-Path $RepoRoot | Select-Object -ExpandProperty Path
Write-Host "Stopping debug instances under: $project"

$candidates = Get-RepoProcessCandidates -ProjectPath $project
if (-not $candidates) {
    Write-Host "No matching debug/backend/UI processes found."
} else {
    $candidates | Select-Object ProcessId, Name, CommandLine | Format-Table -AutoSize | Out-String | Write-Host
    Stop-ProcessTree -ProcessIds $candidates.ProcessId
    Wait-ForStop -ProcessIds $candidates.ProcessId
}

Stop-ProcessOnPort -Port 1420

Set-Location $project

$cargoCmd = Resolve-BuildTool -Name "cargo" -PreferredPath $Cargo
$bunCmd = Resolve-BuildTool -Name "bun" -PreferredPath $Bun
if (-not $cargoCmd) {
    throw "Cargo not found. Re-run with -Cargo 'C:\\path\\to\\cargo.exe' or ensure Rust is in PATH."
}
if (-not $bunCmd) {
    throw "Bun not found. Re-run with -Bun 'C:\\path\\to\\bun.exe' or ensure Bun is in PATH."
}

$cargoDir = Split-Path $cargoCmd
$bunDir = Split-Path $bunCmd
if ($cargoDir -and ($env:PATH -notmatch [regex]::Escape($cargoDir))) {
    $env:PATH = "${cargoDir};$($env:PATH)"
}
if ($bunDir -and ($env:PATH -notmatch [regex]::Escape($bunDir))) {
    $env:PATH = "${bunDir};$($env:PATH)"
}

$cargoArgs = @(
    "build",
    "--manifest-path", (Join-Path $project "Cargo.toml"),
    "--bin", "sd-daemon"
)
if ($BuildProfile -ieq "Release") {
    $cargoArgs += "--release"
}

if (-not $SkipRebuild) {
    Write-Host "Rebuilding daemon ($BuildProfile)..."
    $buildLogPath = Join-Path $project "scripts\\restart-desktop-debug.build.log"
    & $cargoCmd @cargoArgs 2>&1 | Tee-Object -FilePath $buildLogPath
    if ($LASTEXITCODE -ne 0) {
        $tail = if (Test-Path $buildLogPath) { Get-Content $buildLogPath -Tail 40 } else { @() }
        Write-Host "Build failed, showing last 40 lines from: $buildLogPath"
        $tail | ForEach-Object { Write-Host $_ }
        throw "cargo build --bin sd-daemon failed. profile=$BuildProfile"
    }
}

Write-Host "Starting Tauri desktop UI (native app, not webui)..."
$env:HOST = "127.0.0.1"
$tauriDir = Join-Path $project "apps\tauri"
Set-Location $tauriDir
Write-Host "Running: bun run tauri:dev"
& $bunCmd run tauri:dev
