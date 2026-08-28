[CmdletBinding()]
param(
	[string] $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
	[string] $BunPath = "",
	[switch] $NoWatch,
	[switch] $Build,
	[string] $PolicyEventLogPath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0

$wrapperPath = Join-Path $PSScriptRoot "invoke-spacedrive-cargo.ps1"
if (-not (Test-Path -LiteralPath $wrapperPath -PathType Leaf)) {
	throw "Shared Cargo policy wrapper not found: $wrapperPath"
}

$resolvedBun = $BunPath
if (-not $resolvedBun) {
	$resolvedBun = (Get-Command bun -ErrorAction SilentlyContinue).Source
}
if (-not $resolvedBun) {
	throw "Bun not found. Install Bun or pass -BunPath."
}

if ($NoWatch -and $Build) {
	throw "-NoWatch and -Build cannot be used together."
}

$tauriDir = Join-Path $RepoRoot "apps\tauri"
if (-not (Test-Path -LiteralPath (Join-Path $tauriDir "package.json") -PathType Leaf)) {
	throw "Tauri package not found: $tauriDir"
}

$exitCode = 1
Push-Location $tauriDir
try {
	$tauriArguments = if ($Build) {
		@('x', 'tauri', 'build')
	} elseif ($NoWatch) {
		@('x', 'tauri', 'dev', '--no-watch')
	} else {
		@('x', 'tauri', 'dev')
	}
	$wrapperArguments = @(
		'-RepoRoot', $RepoRoot,
		'-CargoPath', $resolvedBun
	) + $tauriArguments
	if ($PolicyEventLogPath) {
		$wrapperArguments = @('-PolicyEventLogPath', $PolicyEventLogPath) + $wrapperArguments
	}
	& $wrapperPath @wrapperArguments
	$exitCode = $LASTEXITCODE
	if ($Build -and $exitCode -eq 0) {
		$fixScript = Join-Path $tauriDir "scripts\fix-daemon-entitlements.sh"
		$bash = (Get-Command bash -ErrorAction SilentlyContinue).Source
		if ($bash -and (Test-Path -LiteralPath $fixScript -PathType Leaf)) {
			& $bash $fixScript (Join-Path $RepoRoot "target\release\bundle\macos\Spacedrive.app") | Out-Host
		}
	}
} finally {
	Pop-Location
}

exit $exitCode
