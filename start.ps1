# Spacedrive unified startup script
# Default: release (production-like) instance for feature verification - no installer bundle.
#
# Disk policy:
#   - All Cargo artifacts use the main worktree target selected by build-policy.ps1.
#   - Compile calls serialize and clean registered worktree artifacts through the shared wrapper.
#
# Usage:
#   ./start.ps1                      # release build + start daemon (+ Tauri if available)
#   ./start.ps1 -Dev                 # hot-reload dev mode
#   ./start.ps1 -DaemonOnly          # release backend only (no desktop shell)
#   ./start.ps1 -Foreground          # run daemon in foreground (show logs)
#   ./start.ps1 -Clean               # clean the policy-selected target before build
#   ./start.ps1 -KeepOtherProfile    # deprecated compatibility parameter; fails clearly
#   ./start.ps1 -TargetDir D:\rust\sd  # deprecated compatibility parameter; fails clearly
#   ./start.ps1 -NoKill              # do not kill existing processes
#   ./start.ps1 -Features "ffmpeg,heif"
#   ./start.ps1 -Jobs 8

[CmdletBinding()]
param(
	[switch]$Dev,
	[switch]$DaemonOnly,
	[switch]$Foreground,
	[switch]$Clean,
	[switch]$KeepOtherProfile,
	[switch]$NoKill,
	[string]$Features = "ffmpeg,heif",
	[string]$TargetDir = "",
	[int]$Jobs = 0,
	[string]$Instance = "",
	[string]$DataDir = ""
)

$ErrorActionPreference = "Stop"

if ($PSBoundParameters.ContainsKey('TargetDir') -or $PSBoundParameters.ContainsKey('KeepOtherProfile')) {
	throw "Build policy error: -TargetDir and -KeepOtherProfile are deprecated compatibility parameters. The main worktree target is mandatory, and alternate targets/profile retention are no longer supported."
}

$Root = $PSScriptRoot
if (-not $Root) { $Root = Get-Location }

Set-Location $Root

function Write-Step($msg, $color = "Cyan") {
	Write-Host $msg -ForegroundColor $color
}

function Write-Ok($msg) {
	Write-Host "  $msg" -ForegroundColor Green
}

function Write-Warn($msg) {
	Write-Host "  $msg" -ForegroundColor Yellow
}

function Write-Info($msg) {
	Write-Host "  $msg" -ForegroundColor Gray
}

function Resolve-RepoRoot {
	# Allow running from a subdirectory
	if (Test-Path (Join-Path $Root "Cargo.toml")) {
		return $Root
	}
	foreach ($candidate in @("..", "../..", "../../..")) {
		$path = Join-Path $Root $candidate
		if (Test-Path (Join-Path $path "Cargo.toml")) {
			return (Resolve-Path $path).Path
		}
	}
	return $Root
}

$RepoRoot = Resolve-RepoRoot
Set-Location $RepoRoot

$BuildPolicyPath = Join-Path $RepoRoot "scripts\build-policy.ps1"
$CargoWrapperPath = Join-Path $RepoRoot "scripts\invoke-spacedrive-cargo.ps1"
. $BuildPolicyPath

function Get-DirSizeGB {
	param([string]$Path)
	if (-not (Test-Path $Path)) { return 0.0 }
	try {
		# Measure-Object on huge trees is slow; use robocopy /L bytes estimate when available
		$sum = (Get-ChildItem -LiteralPath $Path -Recurse -Force -File -ErrorAction SilentlyContinue |
			Measure-Object -Property Length -Sum -ErrorAction SilentlyContinue).Sum
		if (-not $sum) { return 0.0 }
		return [math]::Round(($sum / 1GB), 2)
	} catch {
		return 0.0
	}
}

function Show-DiskReport {
	param([string]$TargetRoot)

	Write-Step "Disk usage (Rust artifacts)..." "Cyan"
	$debugPath = Join-Path $TargetRoot "debug"
	$releasePath = Join-Path $TargetRoot "release"
	$incDebug = Join-Path $debugPath "incremental"
	$incRelease = Join-Path $releasePath "incremental"

	# Fast path: only top-level folder sizes when dirs exist
	foreach ($pair in @(
		@("target root", $TargetRoot),
		@("debug", $debugPath),
		@("release", $releasePath),
		@("debug/incremental", $incDebug),
		@("release/incremental", $incRelease)
	)) {
		$name = $pair[0]
		$path = $pair[1]
		if (Test-Path $path) {
			$gb = Get-DirSizeGB $path
			Write-Info ("{0,-22} {1,8:N2} GB  {2}" -f $name, $gb, $path)
		} else {
			Write-Info ("{0,-22} {1,8}     {2}" -f $name, "-", $path)
		}
	}

	$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
	$registry = Join-Path $cargoHome "registry"
	if (Test-Path $registry) {
		$gb = Get-DirSizeGB $registry
		Write-Info ("{0,-22} {1,8:N2} GB  {2}" -f "cargo registry", $gb, $registry)
	}
	Write-Host ""
}

function Stop-DevProcesses {
	Write-Step "Cleaning up old processes..." "Yellow"

	# Prefer Spacedrive-related processes. cargo/rust-analyzer are optional to avoid
	# killing unrelated IDE work; only force-kill if they look related later if needed.
	$processesToKill = @(
		"node",
		"bun",
		"vite",
		"sd-daemon",
		"sd-cli",
		"spacedrive",
		"spacedrive-tauri"
	)

	foreach ($procName in $processesToKill) {
		$processes = Get-Process -Name $procName -ErrorAction SilentlyContinue
		if ($processes) {
			Write-Info "Killing $($processes.Count) $procName process(es)..."
			$processes | Stop-Process -Force -ErrorAction SilentlyContinue
			Start-Sleep -Milliseconds 100
		}
	}

	$ports = @(5173, 3000, 8080, 1420, 6969)
	foreach ($port in $ports) {
		$connections = Get-NetTCPConnection -LocalPort $port -ErrorAction SilentlyContinue
		if ($connections) {
			foreach ($conn in $connections) {
				$proc = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
				if ($proc -and $proc.Id -ne $PID) {
					Write-Info "Killing process on port ${port}: $($proc.Name) (PID $($proc.Id))"
					Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
				}
			}
		}
	}

	Write-Ok "Cleanup complete"
	Write-Host ""
}

function Get-CargoFeaturesArgs {
	param([string]$FeatureList)

	$parts = @()
	foreach ($f in ($FeatureList -split "," | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
		# Accept both "ffmpeg" and "sd-core/ffmpeg"
		if ($f -like "sd-core/*") {
			$parts += $f
		} else {
			$parts += "sd-core/$f"
		}
	}
	return $parts
}

function Test-CoreAvailable {
	return (Test-Path (Join-Path $RepoRoot "core")) -or (Test-Path (Join-Path $RepoRoot "core\Cargo.toml"))
}

function Test-TauriAvailable {
	$pkg = Join-Path $RepoRoot "apps\tauri\package.json"
	$src = Join-Path $RepoRoot "apps\tauri\src-tauri"
	return (Test-Path $pkg) -and (Test-Path $src)
}

function Invoke-CargoRelease {
	param(
		[string[]]$ExtraArgs,
		[int]$JobCount
	)

	$cargoArgs = @("build", "--release") + $ExtraArgs
	if ($JobCount -gt 0) {
		$cargoArgs += @("-j", "$JobCount")
	}

	Write-Info ("cargo " + ($cargoArgs -join " "))
	$wrapperArgs = @("-RepoRoot", $RepoRoot) + @($cargoArgs)
	& $CargoWrapperPath @wrapperArgs
	return $LASTEXITCODE
}

function Invoke-ProjectCargo {
	param(
		[Parameter(Mandatory = $true)]
		[string[]]$CargoArguments,

		[string]$CargoPath = ""
	)

	$wrapperArgs = @("-RepoRoot", $RepoRoot)
	if ($CargoPath) {
		$wrapperArgs += @("-CargoPath", $CargoPath)
	}
	$wrapperArgs += @($CargoArguments)
	& $CargoWrapperPath @wrapperArgs
	return $LASTEXITCODE
}

function Invoke-ReleaseBuild {
	param(
		[string[]]$FeatureArgs,
		[int]$JobCount
	)

	Write-Step "Building release binaries (formal instance, no installer)..." "Cyan"
	Write-Info "This uses Cargo profile.release (opt-level=s, LTO, strip)."
	Write-Info "First release build can take a long time on this monorepo."

	if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
		throw "cargo not found on PATH. Install Rust toolchain first."
	}

	if (-not (Test-CoreAvailable)) {
		Write-Warn "core/ is missing - cannot compile sd-core-backed binaries in this checkout."
		Write-Warn "Restore the full monorepo (core/, crates/, apps/server, apps/tauri as needed), then re-run."
		throw "Incomplete workspace: missing core/"
	}

	$featureFlag = @()
	if ($FeatureArgs.Count -gt 0) {
		$featureFlag = @("--features", ($FeatureArgs -join ","))
	}

	# Build primary binaries used for feature verification (no installer).
	# Single cargo invocation with -p packages (avoids PowerShell array flatten bugs).
	$cargoArgs = [System.Collections.Generic.List[string]]::new()
	[void]$cargoArgs.AddRange([string[]]@(
		"build", "--release",
		"-p", "sd-core", "--bin", "sd-daemon",
		"-p", "sd-cli", "--bin", "sd-cli"
	))
	if ($FeatureArgs.Count -gt 0) {
		[void]$cargoArgs.Add("--features")
		[void]$cargoArgs.Add(($FeatureArgs -join ","))
	}
	if ($JobCount -gt 0) {
		[void]$cargoArgs.Add("-j")
		[void]$cargoArgs.Add("$JobCount")
	}
	
	# Project requires LLVM 15.x for ffmpeg-sys-next bindgen (LLVM >=16 breaks layouts).
	if (-not $env:LIBCLANG_PATH) {
		$llvm15 = "C:\Program Files\LLVM\bin"
		if (Test-Path (Join-Path $llvm15 "libclang.dll")) {
			$env:LIBCLANG_PATH = $llvm15
			Write-Info "LIBCLANG_PATH=$llvm15"
		}
	}
	
	# Native DLLs (ffmpeg, heif, onnx) must be on PATH when bins start.
	$depsBin = Join-Path $RepoRoot "apps\.deps\bin"
	if (Test-Path $depsBin) {
		if (-not ($env:Path -split ';' | Where-Object { $_ -eq $depsBin })) {
			$env:Path = $depsBin + [IO.Path]::PathSeparator + $env:Path
		}
		Write-Info "PATH prepend: $depsBin"
	}
	
	Write-Info ("cargo " + ($cargoArgs -join " "))
	$code = Invoke-ProjectCargo -CargoArguments @($cargoArgs.ToArray())
	
	if ($code -ne 0 -and $FeatureArgs.Count -gt 0) {
		Write-Warn "Build with features failed (exit $code); retrying without optional features..."
		$fallback = [System.Collections.Generic.List[string]]::new()
		[void]$fallback.AddRange([string[]]@(
			"build", "--release",
			"-p", "sd-core", "--bin", "sd-daemon",
			"-p", "sd-cli", "--bin", "sd-cli"
		))
		if ($JobCount -gt 0) {
			[void]$fallback.Add("-j")
			[void]$fallback.Add("$JobCount")
		}
		Write-Info ("cargo " + ($fallback -join " "))
		$code = Invoke-ProjectCargo -CargoArguments @($fallback.ToArray())
		if ($code -eq 0) {
			Write-Warn "Built without $($FeatureArgs -join ',') - media features may be unavailable."
		}
	}
	
	if ($code -ne 0) {
		throw "cargo build --release failed with exit code $code"
	}

	Write-Ok "Release build finished"
	Write-Host ""
}

function Get-ReleaseBinPath {
	param([string]$Name)

	$targetRoot = Get-SpacedriveCargoTarget -RepoRoot $RepoRoot
	$candidates = @(
		(Join-Path $targetRoot "release\$Name.exe"),
		(Join-Path $targetRoot "release\$Name")
	)
	foreach ($c in $candidates) {
		if (Test-Path $c) { return $c }
	}
	return $null
}

function Start-ReleaseDaemon {
	param(
		[switch]$Fg,
		[string]$Inst,
		[string]$Data
	)

	$cli = Get-ReleaseBinPath "sd-cli"
	$daemon = Get-ReleaseBinPath "sd-daemon"

	if (-not $cli) {
		throw "sd-cli not found under the policy-selected release target. Build failed or incomplete workspace."
	}

	if (-not $daemon) {
		Write-Warn "sd-daemon binary not found under the policy-selected release target."
		Write-Warn "Workspace may be incomplete (missing core/). Cannot start daemon."
		Write-Info "CLI is available: $cli"
		Write-Info "When core is present, re-run: ./start.ps1"
		return $false
	}

	Write-Step "Starting release daemon..." "Cyan"
	Write-Info "daemon: $daemon"
	Write-Info "cli:    $cli"

		# FFmpeg/HEIF/ONNX native DLLs live under apps/.deps/bin
		$depsBin = Join-Path $RepoRoot "apps\.deps\bin"
		if (Test-Path $depsBin) {
			if (-not ($env:Path -split ';' | Where-Object { $_ -eq $depsBin })) {
				$env:Path = $depsBin + [IO.Path]::PathSeparator + $env:Path
			}
			Write-Info "PATH prepend: $depsBin"
		}

	$cliArgs = @("start")
	if ($Fg) { $cliArgs += "--foreground" }
	if ($Data) { $cliArgs = @("--data-dir", $Data) + $cliArgs }
	if ($Inst) { $cliArgs = @("--instance", $Inst) + $cliArgs }

	Write-Info ("& `"$cli`" " + ($cliArgs -join " "))
	& $cli @cliArgs
	if ($LASTEXITCODE -ne 0) {
		# Fallback: start daemon binary directly if CLI start failed
		Write-Warn "sd-cli start failed; trying sd-daemon directly..."
		$daemonArgs = @()
		if ($Data) { $daemonArgs += @("--data-dir", $Data) }
		if ($Inst) { $daemonArgs += @("--instance", $Inst) }

		if ($Fg) {
			& $daemon @daemonArgs
		} else {
			$logDir = Join-Path $RepoRoot "logs"
			if (-not (Test-Path $logDir)) { New-Item -ItemType Directory -Path $logDir | Out-Null }
			$outLog = Join-Path $logDir "sd-daemon.out.log"
			$errLog = Join-Path $logDir "sd-daemon.err.log"
			Start-Process -FilePath $daemon -ArgumentList $daemonArgs -WorkingDirectory $RepoRoot `
				-RedirectStandardOutput $outLog -RedirectStandardError $errLog -WindowStyle Hidden | Out-Null
			Start-Sleep -Seconds 1
			Write-Ok "Daemon launched in background"
			Write-Info "logs: $outLog"
			Write-Info "      $errLog"
		}
	}

	Write-Host ""
	Write-Ok "Release instance ready for feature verification"
	Write-Info "Useful commands:"
	Write-Info "  $cli status"
	Write-Info "  $cli library list"
	Write-Info "  $cli logs follow"
	Write-Info "  $cli stop"
	return $true
}

function Start-DevMode {
	Write-Step "Starting Spacedrive Development (debug / hot reload)..." "Cyan"
	Write-Host ""

	if (-not (Test-TauriAvailable)) {
		Write-Warn "apps/tauri is not available in this workspace checkout."
		Write-Info "Falling back to cargo daemon (debug) if core exists."

		if (-not (Test-CoreAvailable)) {
			throw "Neither apps/tauri nor core/ is present. Cannot start dev mode in this incomplete checkout."
		}

		$feat = (Get-CargoFeaturesArgs -FeatureList $Features) -join ","
		$cargoArgs = @("run", "--features", $feat, "--bin", "sd-daemon")
		if ($Jobs -gt 0) { $cargoArgs += @("-j", "$Jobs") }
		Write-Info ("cargo " + ($cargoArgs -join " "))
		Invoke-ProjectCargo -CargoArguments $cargoArgs | Out-Host
		return
	}

	# Prefer package-local script, then filter from monorepo root
	$tauriDir = Join-Path $RepoRoot "apps\tauri"
	if (Test-Path (Join-Path $tauriDir "package.json")) {
		Push-Location $tauriDir
		try {
			if (Get-Command bun -ErrorAction SilentlyContinue) {
				Write-Info "bun run tauri:dev"
				bun run tauri:dev
			} else {
				throw "bun is not installed or not on PATH"
			}
		} finally {
			Pop-Location
		}
		return
	}

	Write-Info "bun run --filter @sd/tauri tauri:dev"
	bun run --filter @sd/tauri tauri:dev
}

function Start-TauriReleaseNoBundle {
	if (-not (Test-TauriAvailable)) {
		Write-Warn "apps/tauri not present - skipping desktop shell."
		Write-Info "Backend-only release instance is still usable via sd-cli."
		return
	}

	Write-Step "Building Tauri app in release (no installer bundle)..." "Cyan"
	$tauriDir = Join-Path $RepoRoot "apps\tauri"
	Push-Location $tauriDir
	try {
		if (-not (Get-Command bun -ErrorAction SilentlyContinue)) {
			Write-Warn "bun not found; skip Tauri release shell."
			return
		}

		# Frontend dist first if script exists
		$pkg = Get-Content "package.json" -Raw | ConvertFrom-Json
		$scripts = @()
		if ($pkg.scripts) {
			$scripts = $pkg.scripts.PSObject.Properties.Name
		}

		if ($scripts -contains "build") {
			Write-Info "bun run build (frontend dist)"
			bun run build
			if ($LASTEXITCODE -ne 0) { throw "Frontend build failed" }
		}

		# Prefer explicit no-bundle if available; otherwise tauri:build may still package.
		# Using cargo/tauri CLI with --no-bundle avoids MSI/NSIS installers.
		$tauriCli = $null
		if (Get-Command cargo-tauri -ErrorAction SilentlyContinue) {
			$tauriCli = "cargo-tauri"
		}

		if ($scripts -contains "tauri:build") {
			# Pass through no-bundle if the script forwards args
			Write-Info "bun run tauri:build -- --no-bundle"
			bun run tauri:build -- --no-bundle
		} elseif ($tauriCli) {
			Write-Info "cargo tauri build --no-bundle"
			$tauriArgs = @("tauri", "build", "--no-bundle")
			$tauriCode = Invoke-ProjectCargo -CargoPath $tauriCli -CargoArguments $tauriArgs
			if ($tauriCode -ne 0) {
				$global:LASTEXITCODE = $tauriCode
			}
		} else {
			Write-Warn "No tauri:build script and no cargo-tauri CLI; desktop shell not started."
			return
		}

		if ($LASTEXITCODE -ne 0) {
			Write-Warn "Tauri release build failed (exit $LASTEXITCODE). Daemon may still be running."
			return
		}

		# Try to launch built binary (Windows paths vary by target triple)
		$targetRoot = Get-SpacedriveCargoTarget -RepoRoot $RepoRoot
		$searchRoots = @(Join-Path $targetRoot "release")
		$exe = $null
		foreach ($dir in $searchRoots) {
			if (-not (Test-Path $dir)) { continue }
			$candidates = Get-ChildItem $dir -Filter "*.exe" -ErrorAction SilentlyContinue |
				Where-Object { $_.Name -match "spacedrive|tauri" -and $_.Name -notmatch "deps" }
			if ($candidates) {
				$exe = $candidates | Select-Object -First 1
				break
			}
		}

		if ($exe) {
			Write-Ok "Launching $($exe.FullName)"
			Start-Process -FilePath $exe.FullName -WorkingDirectory $RepoRoot
		} else {
			Write-Warn "Built Tauri binary not found automatically under the policy-selected release target."
		}
	} finally {
		Pop-Location
	}
}

# ─── Main ─────────────────────────────────────────────────────────────

$modeLabel = if ($Dev) { "DEV (debug)" } else { "RELEASE (formal, no installer)" }
Write-Host ""
Write-Step "Spacedrive start.ps1 - mode: $modeLabel" "Cyan"
Write-Info "Repo: $RepoRoot"
Write-Host ""

$effectiveTarget = Get-SpacedriveCargoTarget -RepoRoot $RepoRoot
Write-Info "Policy target: $effectiveTarget"
Show-DiskReport -TargetRoot $effectiveTarget

if (-not $NoKill) {
	Stop-DevProcesses
}

if ($Clean) {
	Write-Step "Cleaning the policy-selected Cargo target..." "Yellow"
	$cleanCode = Invoke-ProjectCargo -CargoArguments @("clean")
	if ($cleanCode -ne 0) {
		throw "Cargo clean failed with exit code $cleanCode"
	} else {
		Write-Ok "Clean complete"
	}
	Write-Host ""
}

# Workspace completeness check (this checkout may be sparse)
$hasCore = Test-CoreAvailable
$hasTauri = Test-TauriAvailable
if (-not $hasCore) {
	Write-Warn "core/ is missing - full backend build may fail until the full repo is checked out."
}
if (-not $hasTauri) {
	Write-Warn "apps/tauri is missing - desktop shell will be skipped."
}
Write-Host ""

try {
	if ($Dev) {
		Start-DevMode
		Show-DiskReport -TargetRoot $effectiveTarget
		exit 0
	}

	# Default path: formal release instance for feature verification
	if (-not $hasCore -and -not (Test-Path (Join-Path $RepoRoot "apps\cli"))) {
		throw "Incomplete workspace: need at least apps/cli and preferably core/."
	}

	$featureArgs = Get-CargoFeaturesArgs -FeatureList $Features
	Invoke-ReleaseBuild -FeatureArgs $featureArgs -JobCount $Jobs

	$started = Start-ReleaseDaemon -Fg:$Foreground -Inst $Instance -Data $DataDir

	if (-not $DaemonOnly -and $started) {
		Start-TauriReleaseNoBundle
	} elseif ($DaemonOnly) {
		Write-Info "DaemonOnly: skipping desktop shell."
	}

	Show-DiskReport -TargetRoot $effectiveTarget

	Write-Host ""
	Write-Step "Done. Default mode is release (not debug)." "Green"
	Write-Info "Dev hot-reload:     ./start.ps1 -Dev"
	Write-Info "Backend only:       ./start.ps1 -DaemonOnly"
	Write-Info "Build policy:       main worktree target only"
	Write-Info "Periodic cleanup:   ./clean-rust-cache.ps1"
} catch {
	Write-Host ""
	Write-Host "Error: $($_.Exception.Message)" -ForegroundColor Red
	exit 1
}
