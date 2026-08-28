# Launch Spacedrive with a visible desktop GUI (Tauri).
# Unlike `sd-cli start` (backend only), this opens the Spacedrive window.
#
# Prerequisites:
#   - bun install (from repo root, under VS Dev Cmd if node-gyp fails)
#   - LLVM 15 at C:\Program Files\LLVM (for media features rebuilds)
#   - Release daemon already built in the policy-selected release target
#
# Usage:
#   .\run-gui.ps1              # backend (release daemon if present) + Tauri dev GUI
#   .\run-gui.ps1 -DaemonOnly  # only ensure release daemon is running
#   .\run-gui.ps1 -BackendOnly # alias of -DaemonOnly

[CmdletBinding()]
param(
	[switch]$DaemonOnly,
	[switch]$BackendOnly
)

$ErrorActionPreference = "Stop"
$RepoRoot = $PSScriptRoot
Set-Location $RepoRoot

$BuildPolicyPath = Join-Path $RepoRoot "scripts\build-policy.ps1"
. $BuildPolicyPath

function Write-Step($m) { Write-Host $m -ForegroundColor Cyan }
function Write-Ok($m) { Write-Host "  $m" -ForegroundColor Green }
function Write-Warn($m) { Write-Host "  $m" -ForegroundColor Yellow }
function Write-Info($m) { Write-Host "  $m" -ForegroundColor Gray }

# Native FFmpeg/HEIF DLLs - WITHOUT this, sd-cli/sd-daemon often exit immediately with no output.
$depsBin = Join-Path $RepoRoot "apps\.deps\bin"
if (Test-Path $depsBin) {
	if (-not ($env:Path -split ';' | Where-Object { $_ -eq $depsBin })) {
		$env:Path = $depsBin + [IO.Path]::PathSeparator + $env:Path
	}
	Write-Info "PATH += $depsBin"
} else {
	Write-Warn "apps\.deps\bin missing - run: cargo xtask setup"
}

if (-not $env:LIBCLANG_PATH) {
	$llvm15 = "C:\Program Files\LLVM\bin"
	if (Test-Path (Join-Path $llvm15 "libclang.dll")) {
		$env:LIBCLANG_PATH = $llvm15
	}
}

$targetRoot = Get-SpacedriveCargoTarget -RepoRoot $RepoRoot
$cli = Join-Path $targetRoot "release\sd-cli.exe"
$daemon = Join-Path $targetRoot "release\sd-daemon.exe"

function Test-RpcOpen {
	try {
		$c = New-Object System.Net.Sockets.TcpClient
		$i = $c.BeginConnect("127.0.0.1", 8488, $null, $null)
		$ok = $i.AsyncWaitHandle.WaitOne(800, $false) -and $c.Connected
		$c.Close()
		return $ok
	} catch {
		return $false
	}
}

function Ensure-ReleaseDaemon {
	if (-not (Test-Path $cli) -or -not (Test-Path $daemon)) {
		throw "Release binaries missing. Build first with start.ps1 so the shared build policy selects the main worktree target."
	}

	# Windows: Tauri externalBin may look for triple-suffixed name
	$triple = Join-Path $targetRoot "release\sd-daemon-x86_64-pc-windows-msvc.exe"
	if (-not (Test-Path $triple)) {
		Copy-Item $daemon $triple -Force
	}

	if (Test-RpcOpen) {
		Write-Ok "Daemon already listening on 127.0.0.1:8488"
		return
	}

	Write-Step "Starting release daemon (no GUI by itself)..."
	& $cli start
	Start-Sleep -Seconds 2
	if (Test-RpcOpen) {
		Write-Ok "Daemon RPC ready on 127.0.0.1:8488"
	} else {
		Write-Warn "Daemon may still be starting. Check logs: $env:USERPROFILE\.spacedrive\logs\"
	}
}

Write-Host ""
Write-Step "Spacedrive GUI launcher"
Write-Info "Note: sd-cli start/status only talk to the backend - they never open a window."
Write-Host ""

Ensure-ReleaseDaemon

if ($DaemonOnly -or $BackendOnly) {
	Write-Ok "Backend-only mode done."
	Write-Info "  $cli status"
	Write-Info "  $cli library list"
	Write-Info "For GUI: .\run-gui.ps1   (without -DaemonOnly)"
	exit 0
}

if (-not (Test-Path (Join-Path $RepoRoot "apps\tauri\package.json"))) {
	throw "apps/tauri missing"
}
if (-not (Test-Path (Join-Path $RepoRoot "node_modules"))) {
	Write-Warn "node_modules missing - run from repo root under VS Dev environment:"
	Write-Info '  cmd /c "\"C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat\" -arch=x64 && bun install"'
	throw "Run bun install first"
}

Write-Step "Starting Tauri desktop window (bun run tauri:dev)..."
Write-Info "First run compiles the Rust shell - can take several minutes."
Write-Info "A Spacedrive window should appear when ready."
Write-Host ""

Set-Location (Join-Path $RepoRoot "apps\tauri")
try {
	$tauriDevLauncher = Join-Path $RepoRoot "scripts\invoke-tauri-dev.ps1"
	$bunCommand = (Get-Command bun -ErrorAction Stop).Source
	& $tauriDevLauncher -RepoRoot $RepoRoot -BunPath $bunCommand
	exit $LASTEXITCODE
} finally {
	Set-Location $RepoRoot
}
