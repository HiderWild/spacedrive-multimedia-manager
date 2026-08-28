[CmdletBinding()]
param()

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$entrypoints = @(
	'clean-rust-cache.ps1',
	'start.ps1',
	'run-cli.ps1',
	'run-gui.ps1',
	'scripts\invoke-tauri-dev.ps1',
	'scripts\restart-desktop-debug.ps1',
	'scripts\bench-scene-embed.ps1',
	'scripts\setup.ps1'
)
$compileEntrypoints = @(
	'start.ps1',
	'clean-rust-cache.ps1',
	'scripts\invoke-tauri-dev.ps1',
	'scripts\restart-desktop-debug.ps1',
	'scripts\bench-scene-embed.ps1',
	'scripts\setup.ps1'
)
$policyEntrypoints = @(
	'start.ps1',
	'clean-rust-cache.ps1',
	'run-cli.ps1',
	'run-gui.ps1',
	'scripts\restart-desktop-debug.ps1'
)
$passed = 0

function Assert-True {
	param(
		[Parameter(Mandatory = $true)]
		[bool] $Condition,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	if (-not $Condition) {
		throw "ASSERT TRUE FAILED: $Message"
	}
}

function Assert-Equal {
	param(
		[Parameter(Mandatory = $true)]
		[object] $Expected,

		[Parameter(Mandatory = $true)]
		[object] $Actual,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	if ($Expected -ne $Actual) {
		throw "ASSERT EQUAL FAILED: $Message. Expected=[$Expected] Actual=[$Actual]"
	}
}

function Get-ScriptAst {
	param([Parameter(Mandatory = $true)][string] $Path)

	$tokens = $null
	$parseErrors = $null
	$ast = [System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$parseErrors)
	Assert-Equal -Expected 0 -Actual @($parseErrors).Count -Message "PowerShell parses $Path"
	return $ast
}

foreach ($relativePath in $entrypoints) {
	$path = Join-Path $repoRoot $relativePath
	Assert-True -Condition (Test-Path -LiteralPath $path -PathType Leaf) -Message "Allowed entrypoint exists: $relativePath"
	$ast = Get-ScriptAst -Path $path
	$source = Get-Content -LiteralPath $path -Raw

	$directCargoCommands = @($ast.FindAll({
		param($node)
		if ($node -isnot [System.Management.Automation.Language.CommandAst]) {
			return $false
		}
		$commandName = $node.GetCommandName()
		if ($commandName -notin @('cargo', 'cargo.exe')) {
			return $false
		}
		return $node.Extent.Text -notmatch '(?i)^\s*(?:&\s*)?cargo(?:\.exe)?\s+install\s+cargo-watch\b'
	}, $true))
	Assert-Equal -Expected 0 -Actual $directCargoCommands.Count -Message "$relativePath has no direct Cargo build/run/clean command"

	if ($compileEntrypoints -contains $relativePath) {
		Assert-True -Condition ($source -match '(?i)invoke-spacedrive-cargo\.ps1') -Message "$relativePath routes compile-capable work through the shared wrapper"
	}

	if ($policyEntrypoints -contains $relativePath) {
		Assert-True -Condition ($source -match '(?i)build-policy\.ps1') -Message "$relativePath loads the shared build policy"
		Assert-True -Condition ($source -match '(?i)Get-SpacedriveCargoTarget') -Message "$relativePath resolves the main-root Cargo target through policy"
		Assert-True -Condition ($source -notmatch '(?m)\$env:CARGO_TARGET_DIR\s*=') -Message "$relativePath does not select an ambient or alternate Cargo target"
	}
}

$startSource = Get-Content -LiteralPath (Join-Path $repoRoot 'start.ps1') -Raw
Assert-True -Condition ($startSource -match '(?is)PSBoundParameters\.ContainsKey\(\s*[''\"]TargetDir[''\"]\s*\).*?PSBoundParameters\.ContainsKey\(\s*[''\"]KeepOtherProfile[''\"]\s*\).*?(?:throw|Write-Error).*?(?:deprecated|policy)') -Message 'start.ps1 rejects explicitly supplied alternate-target compatibility parameters'
Assert-True -Condition ($startSource -notmatch '(?i)Invoke-ProfilePrune|Remove-DirSafe\s+-Path\s+.*(?:target|TargetRoot)') -Message 'start.ps1 does not retain private profile-prune or target deletion behavior'

$cleanSource = Get-Content -LiteralPath (Join-Path $repoRoot 'clean-rust-cache.ps1') -Raw
Assert-True -Condition ($cleanSource -match '(?is)PSBoundParameters\.ContainsKey\(\s*[''\"]TargetDir[''\"]\s*\).*?(?:throw|Write-Error).*?(?:deprecated|policy)') -Message 'clean-rust-cache.ps1 rejects an alternate target parameter'

foreach ($relativePath in @('run-cli.ps1', 'run-gui.ps1', 'scripts\restart-desktop-debug.ps1')) {
	$path = Join-Path $repoRoot $relativePath
	$ast = Get-ScriptAst -Path $path
	$hardcodedTargetStrings = @($ast.FindAll({
		param($node)
		return ($node -is [System.Management.Automation.Language.StringConstantExpressionAst]) -and ($node.Value -match '(?i)target[\\/]release|target[\\/]debug')
	}, $true))
	Assert-Equal -Expected 0 -Actual $hardcodedTargetStrings.Count -Message "$relativePath resolves binaries without hardcoded target profile paths"
}

$benchSource = Get-Content -LiteralPath (Join-Path $repoRoot 'scripts\bench-scene-embed.ps1') -Raw
Assert-True -Condition ($benchSource -notmatch '(?i)Invoke-Expression') -Message 'Scene benchmark forwards a complete argument array without Invoke-Expression'

$tauriConfig = Get-Content -LiteralPath (Join-Path $repoRoot 'apps\tauri\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$tauriDevSource = Get-Content -LiteralPath (Join-Path $repoRoot 'apps\tauri\scripts\dev-with-daemon.ts') -Raw
$tauriDevLauncherPath = Join-Path $repoRoot 'scripts\invoke-tauri-dev.ps1'
$tauriDevLauncherSource = if (Test-Path -LiteralPath $tauriDevLauncherPath) {
	Get-Content -LiteralPath $tauriDevLauncherPath -Raw
} else {
	''
}
$policySource = Get-Content -LiteralPath (Join-Path $repoRoot 'scripts\build-policy.ps1') -Raw

Assert-True -Condition ($tauriConfig.build.beforeDevCommand -match '(?i)dev:with-daemon') -Message 'Tauri development still uses the daemon/Vite development hook'
Assert-True -Condition (Test-Path -LiteralPath $tauriDevLauncherPath -PathType Leaf) -Message 'Windows Tauri development has a policy launcher'
Assert-True -Condition ($tauriDevLauncherSource -match '(?i)invoke-spacedrive-cargo\.ps1') -Message 'Tauri development launcher uses the shared wrapper'
Assert-True -Condition ($tauriDevLauncherSource -match '(?i)-CargoPath') -Message 'Tauri development launcher forwards its executable through the wrapper'
Assert-True -Condition ($tauriDevSource -match '(?i)invoke-spacedrive-cargo\.ps1') -Message 'Tauri daemon hook uses the shared wrapper on Windows'
Assert-True -Condition ($tauriDevSource -match '(?i)powershell\.exe') -Message 'Tauri daemon hook invokes PowerShell policy on Windows'
Assert-True -Condition ($tauriDevSource -match '(?is)if\s*\(IS_WIN\).*?execFileSync\(\s*["'']powershell\.exe["'']') -Message 'Tauri metadata invokes PowerShell policy on Windows'
Assert-True -Condition ($tauriDevSource -match '(?is)getWindowsPolicyArguments\(\[.*?metadata.*?no-deps.*?\]\)') -Message 'Tauri metadata passes arguments to the policy wrapper'
Assert-True -Condition ($tauriDevSource -match '(?is)function\s+getWindowsPolicyArguments.*?CARGO_WRAPPER') -Message 'Tauri metadata/build arguments include the shared wrapper'
Assert-True -Condition ($tauriDevSource -match '(?is)function\s+getCargoBuildCommand.*?if\s*\(IS_WIN\).*?command:\s*["'']powershell\.exe["'']') -Message 'Tauri daemon builds invoke PowerShell policy on Windows'
Assert-True -Condition ($tauriDevSource -notmatch '(?is)function\s+getCargoBuildCommand.*?if\s*\(IS_WIN\).*?spawn\(\s*["'']cargo["'']') -Message 'Tauri Windows build branch has no direct Cargo spawn'
Assert-True -Condition ($policySource -match '(?i)SD_SPACEDRIVE_BUILD_POLICY_ACTIVE') -Message 'Shared policy supports nested Tauri development wrapper calls without releasing the outer lock'

$tauriDevConfigSource = Get-Content -LiteralPath (Join-Path $repoRoot 'apps\tauri\src-tauri\tauri.conf.json') -Raw
Assert-True -Condition ($tauriDevConfigSource -match '(?i)beforeDevCommand') -Message 'Tauri config declares a development hook'
Assert-True -Condition ($tauriDevConfigSource -match '(?i)beforeBuildCommand') -Message 'Tauri config declares a production build hook'
Assert-True -Condition ($tauriDevConfigSource -notmatch '(?i)\bcargo(?:\.exe)?\s+(?:build|run|metadata)\b') -Message 'Tauri config has no direct Cargo compile command'

$justSource = Get-Content -LiteralPath (Join-Path $repoRoot 'justfile') -Raw
Assert-True -Condition ($justSource -match '(?im)^dev-desktop:\r?\n[ \t]+[^\r\n]*invoke-tauri-dev\.ps1') -Message 'just dev-desktop routes Tauri development through the policy launcher'

$tauriPackage = Get-Content -LiteralPath (Join-Path $repoRoot 'apps\tauri\package.json') -Raw | ConvertFrom-Json
foreach ($scriptName in @('tauri:dev', 'tauri:dev:no-watch', 'tauri:build')) {
	$scriptValue = $tauriPackage.scripts.$scriptName
	Assert-True -Condition ($scriptValue -match '(?i)invoke-tauri-dev\.ps1') -Message "$scriptName uses the policy launcher"
	Assert-True -Condition ($scriptValue -notmatch '(?i)\btauri\s+(?:dev|build)\b') -Message "$scriptName has no direct Tauri compile command"
}

Assert-True -Condition ($tauriConfig.build.beforeDevCommand -match '(?i)dev:with-daemon') -Message 'Tauri beforeDevCommand retains daemon/Vite startup behavior'
Assert-True -Condition ($tauriConfig.build.beforeBuildCommand -match '(?i)build:daemon:release') -Message 'Tauri beforeBuildCommand retains release daemon build behavior'

foreach ($relativePath in @('start.ps1', 'run-gui.ps1', 'scripts\restart-desktop-debug.ps1')) {
	$source = Get-Content -LiteralPath (Join-Path $repoRoot $relativePath) -Raw
	Assert-True -Condition ($source -match '(?i)invoke-tauri-dev\.ps1') -Message "$relativePath routes Tauri development through the policy launcher"
}

$passed++
Write-Host "ALL TESTS PASSED: $passed"
