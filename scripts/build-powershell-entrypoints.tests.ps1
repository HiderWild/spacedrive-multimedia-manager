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
	'scripts\restart-desktop-debug.ps1',
	'scripts\bench-scene-embed.ps1',
	'scripts\setup.ps1'
)
$compileEntrypoints = @(
	'start.ps1',
	'clean-rust-cache.ps1',
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

$passed++
Write-Host "ALL TESTS PASSED: $passed"
