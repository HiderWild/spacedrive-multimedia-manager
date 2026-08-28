[CmdletBinding()]
param()

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$fixtureRoot = Join-Path $env:TEMP ("spacedrive-tauri-policy-{0}" -f [Guid]::NewGuid().ToString('N'))

function Assert-True {
	param(
		[Parameter(Mandatory = $true)][bool] $Condition,
		[Parameter(Mandatory = $true)][string] $Message
	)

	if (-not $Condition) {
		throw "ASSERT TRUE FAILED: $Message"
	}
}

function Assert-Equal {
	param(
		[Parameter(Mandatory = $true)][object] $Expected,
		[Parameter(Mandatory = $true)][object] $Actual,
		[Parameter(Mandatory = $true)][string] $Message
	)

	if ($Expected -ne $Actual) {
		throw "ASSERT EQUAL FAILED: $Message. Expected=[$Expected] Actual=[$Actual]"
	}
}

function New-TextFile {
	param(
		[Parameter(Mandatory = $true)][string] $LiteralPath,
		[Parameter(Mandatory = $true)][string] $Value
	)

	$parent = Split-Path -Parent $LiteralPath
	if (-not (Test-Path -LiteralPath $parent)) {
		New-Item -ItemType Directory -Path $parent -Force | Out-Null
	}
	Set-Content -LiteralPath $LiteralPath -Value $Value -Encoding UTF8
}

function Invoke-Git {
	param([Parameter(Mandatory = $true)][string[]] $Arguments)

	$oldErrorActionPreference = $ErrorActionPreference
	try {
		$ErrorActionPreference = 'Continue'
		$output = @(& git @Arguments 2>&1)
		$exitCode = $LASTEXITCODE
	} finally {
		$ErrorActionPreference = $oldErrorActionPreference
	}
	if ($exitCode -ne 0) {
		throw "git failed with exit code ${exitCode}: git $($Arguments -join ' ')`n$($output -join "`n")"
	}
}

function Normalize-Path {
	param([Parameter(Mandatory = $true)][string] $Path)
	return [IO.Path]::GetFullPath($Path).TrimEnd([char[]]'\/')
}

try {
	New-Item -ItemType Directory -Path (Join-Path $fixtureRoot 'apps\tauri') -Force | Out-Null
	New-Item -ItemType Directory -Path (Join-Path $fixtureRoot 'scripts') -Force | Out-Null
	New-TextFile -LiteralPath (Join-Path $fixtureRoot 'README.md') -Value 'fixture'
	New-TextFile -LiteralPath (Join-Path $fixtureRoot 'apps\tauri\package.json') -Value '{"name":"fixture-tauri"}'
	Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\build-policy.ps1') -Destination (Join-Path $fixtureRoot 'scripts\build-policy.ps1')
	Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\invoke-spacedrive-cargo.ps1') -Destination (Join-Path $fixtureRoot 'scripts\invoke-spacedrive-cargo.ps1')
	Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\invoke-tauri-dev.ps1') -Destination (Join-Path $fixtureRoot 'scripts\invoke-tauri-dev.ps1')

	$fakeNestedCargo = Join-Path $fixtureRoot 'fake-nested-cargo.ps1'
	New-TextFile -LiteralPath $fakeNestedCargo -Value @'
$ErrorActionPreference = 'Stop'
$argumentsJson = ConvertTo-Json -InputObject @($args) -Compress
$nestedSentinelExists = Test-Path -LiteralPath (Join-Path $env:SD_TAURI_POLICY_ROOT 'target\nested-sentinel.txt')
[IO.File]::AppendAllText($env:SD_TAURI_POLICY_NESTED_LOG, "nested|$($PWD.Path)|$env:CARGO_TARGET_DIR|$env:SD_SPACEDRIVE_BUILD_POLICY_ACTIVE|$nestedSentinelExists|$argumentsJson`r`n")
exit 0
'@

	$fakeBun = Join-Path $fixtureRoot 'fake-bun.ps1'
	New-TextFile -LiteralPath $fakeBun -Value @'
$ErrorActionPreference = 'Stop'
$argumentsJson = ConvertTo-Json -InputObject @($args) -Compress
[IO.File]::AppendAllText($env:SD_TAURI_POLICY_BUN_LOG, "bun|$($PWD.Path)|$env:CARGO_TARGET_DIR|$env:SD_SPACEDRIVE_BUILD_POLICY_ACTIVE|$argumentsJson`r`n")
New-Item -ItemType Directory -Path (Join-Path $env:SD_TAURI_POLICY_ROOT 'target') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $env:SD_TAURI_POLICY_ROOT 'target\nested-sentinel.txt') -Value 'nested' -Encoding UTF8
$nestedArguments = @(
    '-NoProfile',
    '-ExecutionPolicy', 'Bypass',
    '-File', $env:SD_TAURI_POLICY_WRAPPER,
    '-RepoRoot', $env:SD_TAURI_POLICY_ROOT,
    '-CargoPath', $env:SD_TAURI_POLICY_NESTED_CARGO,
    '-PolicyEventLogPath', $env:SD_TAURI_POLICY_EVENTS,
    'build', '--bin', 'nested'
)
& powershell.exe @nestedArguments
exit $LASTEXITCODE
'@
	$fakeFailingBun = Join-Path $fixtureRoot 'fake-failing-bun.ps1'
	New-TextFile -LiteralPath $fakeFailingBun -Value @'
$ErrorActionPreference = 'Stop'
exit 17
'@

	$bunLog = Join-Path $fixtureRoot 'bun.log'
	$nestedLog = Join-Path $fixtureRoot 'nested.log'
	$eventsLog = Join-Path $fixtureRoot 'events.log'
	$env:SD_TAURI_POLICY_ROOT = $fixtureRoot
	$env:SD_TAURI_POLICY_BUN_LOG = $bunLog
	$env:SD_TAURI_POLICY_NESTED_LOG = $nestedLog
	$env:SD_TAURI_POLICY_EVENTS = $eventsLog
	$env:SD_TAURI_POLICY_WRAPPER = Join-Path $fixtureRoot 'scripts\invoke-spacedrive-cargo.ps1'
	$env:SD_TAURI_POLICY_NESTED_CARGO = $fakeNestedCargo

	Invoke-Git -Arguments @('-C', $fixtureRoot, 'init', '-q')
	Invoke-Git -Arguments @('-C', $fixtureRoot, 'config', 'user.email', 'tauri-policy-tests@example.invalid')
	Invoke-Git -Arguments @('-C', $fixtureRoot, 'config', 'user.name', 'Tauri Policy Tests')
	Invoke-Git -Arguments @('-C', $fixtureRoot, 'add', '.')
	Invoke-Git -Arguments @('-C', $fixtureRoot, 'commit', '-qm', 'fixture')

	New-TextFile -LiteralPath (Join-Path $fixtureRoot 'target\outer-sentinel.txt') -Value 'outer'
	$helper = Join-Path $fixtureRoot 'scripts\invoke-tauri-dev.ps1'
	$oldErrorActionPreference = $ErrorActionPreference
	try {
		$ErrorActionPreference = 'Continue'
		$output = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $helper -RepoRoot $fixtureRoot -BunPath $fakeBun -PolicyEventLogPath $eventsLog 2>&1 | ForEach-Object { $_.ToString() })
		$exitCode = $LASTEXITCODE
	} finally {
		$ErrorActionPreference = $oldErrorActionPreference
	}
	Assert-Equal -Expected 0 -Actual $exitCode -Message 'Tauri launcher returns the supplied fake Bun exit code'

	$bunParts = (Get-Content -LiteralPath $bunLog -First 1) -split '\|', 5
	Assert-Equal -Expected 'bun' -Actual $bunParts[0] -Message 'Wrapper invokes the supplied Bun executable'
	Assert-Equal -Expected (Normalize-Path (Join-Path $fixtureRoot 'apps\tauri')) -Actual (Normalize-Path $bunParts[1]) -Message 'Bun runs from apps/tauri'
	Assert-Equal -Expected (Normalize-Path (Join-Path $fixtureRoot 'target')) -Actual (Normalize-Path $bunParts[2]) -Message 'Bun receives the main-worktree target'
	Assert-Equal -Expected '1' -Actual $bunParts[3] -Message 'Bun inherits the active policy marker'
	Assert-Equal -Expected '["x","tauri","dev"]' -Actual $bunParts[4] -Message 'Bun receives exact dev arguments'

	$oldErrorActionPreference = $ErrorActionPreference
	try {
		$ErrorActionPreference = 'Continue'
		$noWatchOutput = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $helper -RepoRoot $fixtureRoot -BunPath $fakeBun -NoWatch -PolicyEventLogPath $eventsLog 2>&1 | ForEach-Object { $_.ToString() })
		$noWatchExitCode = $LASTEXITCODE
		$buildOutput = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $helper -RepoRoot $fixtureRoot -BunPath $fakeBun -Build -PolicyEventLogPath $eventsLog 2>&1 | ForEach-Object { $_.ToString() })
		$buildExitCode = $LASTEXITCODE
	} finally {
		$ErrorActionPreference = $oldErrorActionPreference
	}
	Assert-Equal -Expected 0 -Actual $noWatchExitCode -Message 'No-watch Tauri launcher returns the supplied fake Bun exit code'
	Assert-Equal -Expected 0 -Actual $buildExitCode -Message 'Tauri build launcher returns the supplied fake Bun exit code'
	$bunLines = @(Get-Content -LiteralPath $bunLog)
	Assert-Equal -Expected 3 -Actual $bunLines.Count -Message 'Dev, no-watch, and build each invoke Bun once'
	$noWatchParts = $bunLines[1] -split '\|', 5
	$buildParts = $bunLines[2] -split '\|', 5
	Assert-Equal -Expected '["x","tauri","dev","--no-watch"]' -Actual $noWatchParts[4] -Message 'Bun receives exact no-watch arguments'
	Assert-Equal -Expected '["x","tauri","build"]' -Actual $buildParts[4] -Message 'Bun receives exact build arguments'

	$oldErrorActionPreference = $ErrorActionPreference
	try {
		$ErrorActionPreference = 'Continue'
		$failureOutput = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $helper -RepoRoot $fixtureRoot -BunPath $fakeFailingBun -PolicyEventLogPath $eventsLog 2>&1 | ForEach-Object { $_.ToString() })
		$failureExitCode = $LASTEXITCODE
	} finally {
		$ErrorActionPreference = $oldErrorActionPreference
	}
	Assert-Equal -Expected 17 -Actual $failureExitCode -Message 'Tauri launcher preserves a failing supplied executable exit code'

$nestedParts = (Get-Content -LiteralPath $nestedLog -First 1) -split '\|', 6
	Assert-Equal -Expected 'nested' -Actual $nestedParts[0] -Message 'Nested wrapper invokes the supplied fake Cargo executable'
	Assert-Equal -Expected (Normalize-Path (Join-Path $fixtureRoot 'apps\tauri')) -Actual (Normalize-Path $nestedParts[1]) -Message 'Nested Cargo keeps the Tauri working directory'
	Assert-Equal -Expected (Normalize-Path (Join-Path $fixtureRoot 'target')) -Actual (Normalize-Path $nestedParts[2]) -Message 'Nested Cargo inherits the main-worktree target'
	Assert-Equal -Expected '1' -Actual $nestedParts[3] -Message 'Nested Cargo inherits the active policy marker'
Assert-Equal -Expected 'True' -Actual $nestedParts[4] -Message 'Nested wrapper does not clean while the outer policy invocation is active'
Assert-Equal -Expected '["build","--bin","nested"]' -Actual $nestedParts[5] -Message 'Nested wrapper preserves positional Cargo arguments'
Assert-True -Condition (-not (Test-Path -LiteralPath (Join-Path $fixtureRoot 'target\outer-sentinel.txt'))) -Message 'Outer wrapper performs the registered cleanup once'

	$events = @(Get-Content -LiteralPath $eventsLog)
	Assert-Equal -Expected 4 -Actual @($events | Where-Object { $_ -like 'lock-acquired|*' }).Count -Message 'Each outer Tauri invocation, including the failing launcher, acquires one lock'
	Assert-Equal -Expected 4 -Actual @($events | Where-Object { $_ -like 'lock-released|*' }).Count -Message 'Each outer Tauri invocation, including the failing launcher, releases its lock exactly once'

	$devSource = Get-Content -LiteralPath (Join-Path $repoRoot 'apps\tauri\scripts\dev-with-daemon.ts') -Raw
	Assert-True -Condition ($devSource -match '(?is)if\s*\(IS_WIN\).*?execFileSync\(\s*["'']powershell\.exe["'']') -Message 'Windows metadata lookup invokes PowerShell'
	Assert-True -Condition ($devSource -match '(?is)getWindowsPolicyArguments\(\[.*?metadata.*?no-deps.*?\]\)') -Message 'Windows metadata lookup passes metadata arguments to the policy wrapper'
	Assert-True -Condition ($devSource -match '(?is)function\s+getWindowsPolicyArguments.*?CARGO_WRAPPER') -Message 'Windows metadata/build arguments include the shared wrapper'
	Assert-True -Condition ($devSource -match '(?is)function\s+getCargoBuildCommand.*?if\s*\(IS_WIN\).*?command:\s*["'']powershell\.exe["'']') -Message 'Windows daemon build invokes PowerShell'

	$tauriConfig = Get-Content -LiteralPath (Join-Path $repoRoot 'apps\tauri\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
	$tauriPackage = Get-Content -LiteralPath (Join-Path $repoRoot 'apps\tauri\package.json') -Raw | ConvertFrom-Json
	Assert-True -Condition ($tauriConfig.build.beforeDevCommand -match '(?i)dev:with-daemon') -Message 'beforeDevCommand retains the daemon hook'
	Assert-True -Condition ($tauriConfig.build.beforeBuildCommand -match '(?i)build:daemon:release') -Message 'beforeBuildCommand retains the release daemon hook'
	Assert-True -Condition ($tauriPackage.scripts.'tauri:dev' -match '(?i)invoke-tauri-dev\.ps1') -Message 'tauri:dev uses the policy launcher'
	Assert-True -Condition ($tauriPackage.scripts.'tauri:dev:no-watch' -match '(?i)invoke-tauri-dev\.ps1') -Message 'tauri:dev:no-watch uses the policy launcher'
	Assert-True -Condition ($tauriPackage.scripts.'tauri:build' -match '(?i)invoke-tauri-dev\.ps1') -Message 'tauri:build uses the policy launcher'

	Write-Host 'ALL TESTS PASSED: Tauri dev policy chain'
} finally {
	foreach ($name in @('SD_TAURI_POLICY_ROOT', 'SD_TAURI_POLICY_BUN_LOG', 'SD_TAURI_POLICY_NESTED_LOG', 'SD_TAURI_POLICY_EVENTS', 'SD_TAURI_POLICY_WRAPPER', 'SD_TAURI_POLICY_NESTED_CARGO')) {
		Remove-Item "Env:$name" -ErrorAction SilentlyContinue
	}
	if (Test-Path -LiteralPath $fixtureRoot) {
		$normalizedFixture = Normalize-Path $fixtureRoot
		$normalizedTemp = Normalize-Path $env:TEMP
		if (-not $normalizedFixture.StartsWith($normalizedTemp + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
			throw "Refusing to remove fixture outside TEMP: $normalizedFixture"
		}
		Remove-Item -LiteralPath $normalizedFixture -Recurse -Force
	}
}
