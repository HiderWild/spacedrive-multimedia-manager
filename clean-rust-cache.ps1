# Spacedrive Rust disk cleanup (safe, selective)
# Implements the recommended low-risk cache controls:
#   1) Drop unused build profiles / leftover trees under target/
#   2) Optional: prune cargo registry (downloads) via cargo-cache when installed
#   3) Optional: cargo-sweep time-based target pruning when installed
#
# Usage:
#   ./clean-rust-cache.ps1                 # report + prune debug (keep release)
#   ./clean-rust-cache.ps1 -KeepDebug      # report + prune release (keep debug)
#   ./clean-rust-cache.ps1 -AllTarget      # remove entire target dir
#   ./clean-rust-cache.ps1 -Registry       # also clean cargo registry cache
#   ./clean-rust-cache.ps1 -SweepDays 14   # cargo-sweep artifacts older than N days
#   ./clean-rust-cache.ps1 -DryRun         # show what would be deleted
#   ./clean-rust-cache.ps1 -ReportOnly     # only print sizes

[CmdletBinding()]
param(
	[switch]$KeepDebug,
	[switch]$AllTarget,
	[switch]$Registry,
	[switch]$DryRun,
	[switch]$ReportOnly,
	[int]$SweepDays = 0,
	[string]$TargetDir = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = $PSScriptRoot
if (-not $RepoRoot) { $RepoRoot = (Get-Location).Path }
Set-Location $RepoRoot

function Write-Step($msg, $color = "Cyan") { Write-Host $msg -ForegroundColor $color }
function Write-Ok($msg) { Write-Host "  $msg" -ForegroundColor Green }
function Write-Warn($msg) { Write-Host "  $msg" -ForegroundColor Yellow }
function Write-Info($msg) { Write-Host "  $msg" -ForegroundColor Gray }

function Get-DirSizeGB {
	param([string]$Path)
	if (-not (Test-Path $Path)) { return 0.0 }
	try {
		$sum = (Get-ChildItem -LiteralPath $Path -Recurse -Force -File -ErrorAction SilentlyContinue |
			Measure-Object -Property Length -Sum -ErrorAction SilentlyContinue).Sum
		if (-not $sum) { return 0.0 }
		return [math]::Round(($sum / 1GB), 2)
	} catch {
		return 0.0
	}
}

function Get-TargetRoot {
	if ($TargetDir) { return $TargetDir }
	if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
	return (Join-Path $RepoRoot "target")
}

function Show-Report {
	param([string]$TargetRoot)

	Write-Step "Rust artifact sizes" "Cyan"
	$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }

	$rows = @(
		@("target root", $TargetRoot),
		@("debug", (Join-Path $TargetRoot "debug")),
		@("release", (Join-Path $TargetRoot "release")),
		@("debug/incremental", (Join-Path $TargetRoot "debug\incremental")),
		@("release/incremental", (Join-Path $TargetRoot "release\incremental")),
		@("cargo registry", (Join-Path $cargoHome "registry")),
		@("cargo git", (Join-Path $cargoHome "git"))
	)

	$total = 0.0
	foreach ($row in $rows) {
		$name = $row[0]
		$path = $row[1]
		if (Test-Path $path) {
			$gb = Get-DirSizeGB $path
			$total += $gb
			Write-Info ("{0,-22} {1,8:N2} GB  {2}" -f $name, $gb, $path)
		} else {
			Write-Info ("{0,-22} {1,8}     {2}" -f $name, "-", $path)
		}
	}
	Write-Info ("{0,-22} {1,8:N2} GB  (sum of existing rows; overlaps possible)" -f "approx total", $total)
	Write-Host ""
}

function Remove-Path {
	param([string]$Path, [string]$Label)

	if (-not (Test-Path $Path)) {
		Write-Info "Skip (missing): $Label"
		return
	}
	$gb = Get-DirSizeGB $Path
	if ($DryRun) {
		Write-Warn "[DryRun] would remove $Label (~$gb GB): $Path"
		return
	}
	Write-Info "Removing $Label (~$gb GB)..."
	Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop
	Write-Ok "Removed $Label"
}

$targetRoot = Get-TargetRoot
Write-Host ""
Write-Step "Spacedrive clean-rust-cache.ps1" "Cyan"
Write-Info "Repo: $RepoRoot"
Write-Info "Target: $targetRoot"
if ($DryRun) { Write-Warn "DryRun enabled - no deletions" }
Write-Host ""

Show-Report -TargetRoot $targetRoot

if ($ReportOnly) {
	Write-Ok "Report only. Done."
	Write-Info "Examples:"
	Write-Info "  ./clean-rust-cache.ps1              # drop debug, keep release"
	Write-Info "  ./clean-rust-cache.ps1 -KeepDebug   # drop release, keep debug"
	Write-Info "  ./clean-rust-cache.ps1 -AllTarget   # wipe all build artifacts"
	Write-Info "  ./clean-rust-cache.ps1 -Registry    # also prune cargo registry"
	exit 0
}

Write-Step "Applying recommended cleanup..." "Yellow"

if ($AllTarget) {
	Remove-Path -Path $targetRoot -Label "entire target"
} elseif ($KeepDebug) {
	Remove-Path -Path (Join-Path $targetRoot "release") -Label "target/release"
} else {
	# Default: formal/release-first workflow keeps release, drops debug
	Remove-Path -Path (Join-Path $targetRoot "debug") -Label "target/debug"
}

# Drop other stray profile dirs that sometimes accumulate (mobile-dev, etc.)
if (-not $AllTarget -and (Test-Path $targetRoot)) {
	$known = @("debug", "release", "tmp", "doc", "package", "flycheck0", ".rustc_info.json", "CACHEDIR.TAG")
	Get-ChildItem -LiteralPath $targetRoot -Force -ErrorAction SilentlyContinue | ForEach-Object {
		if ($_.PSIsContainer -and ($known -notcontains $_.Name) -and ($_.Name -notmatch '^[A-Z]')) {
			# leave triple dirs (x86_64-..., aarch64-...) unless explicitly cleaning all
			if ($_.Name -match '^(x86_64|aarch64|wasm32|i686|armv7)') {
				Write-Info "Cross-target present (kept): $($_.Name) - remove manually if unused"
			}
		}
	}
}

if ($SweepDays -gt 0) {
	if (Get-Command cargo-sweep -ErrorAction SilentlyContinue) {
		Write-Step "cargo-sweep (-t $SweepDays)..." "Cyan"
		if ($DryRun) {
			Write-Warn "[DryRun] would run: cargo sweep -t $SweepDays"
		} else {
			Push-Location $RepoRoot
			try {
				cargo sweep -s 2>$null | Out-Null
				cargo sweep -t $SweepDays
				Write-Ok "cargo-sweep done"
			} finally {
				Pop-Location
			}
		}
	} else {
		Write-Warn "cargo-sweep not installed. Install: cargo install cargo-sweep"
	}
}

if ($Registry) {
	if (Get-Command cargo-cache -ErrorAction SilentlyContinue) {
		Write-Step "cargo-cache autoclean..." "Cyan"
		if ($DryRun) {
			Write-Warn "[DryRun] would run: cargo cache --autoclean"
		} else {
			cargo cache --autoclean
			Write-Ok "Registry cache cleaned"
		}
	} else {
		Write-Warn "cargo-cache not installed. Install: cargo install cargo-cache"
		Write-Info "Or manually: Remove-Item -Recurse `$env:USERPROFILE\.cargo\registry\src (sources re-extract)"
	}
}

Write-Host ""
Show-Report -TargetRoot $targetRoot
Write-Step "Done." "Green"
Write-Info "Recommended install (once): cargo install cargo-sweep cargo-cache"
Write-Info "Optional env for large disk: `$env:CARGO_TARGET_DIR = 'D:\rust-targets\spacedrive'"
