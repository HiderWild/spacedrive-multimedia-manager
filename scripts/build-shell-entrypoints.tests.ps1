Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$justfile = Get-Content -Raw (Join-Path $repoRoot 'justfile')
$tauriPackage = Get-Content -Raw (Join-Path $repoRoot 'apps\tauri\package.json') | ConvertFrom-Json
$tsClientPackage = Get-Content -Raw (Join-Path $repoRoot 'packages\ts-client\package.json') | ConvertFrom-Json
$vscodeTasksSource = Get-Content -Raw (Join-Path $repoRoot '.vscode\tasks.json')
$vscodeTasks = [Regex]::Replace($vscodeTasksSource, '(?m)^\s*//.*$', '') | ConvertFrom-Json
$typeCheckScript = Get-Content -Raw (Join-Path $repoRoot 'scripts\check-ts-types.sh')
$serverScript = Get-Content -Raw (Join-Path $repoRoot 'build-server.sh')
$autoformatScript = Get-Content -Raw (Join-Path $repoRoot 'scripts\autoformat.sh')

$passed = 0

function Assert-True {
	param(
		[Parameter(Mandatory = $true)]
		[bool] $Condition,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	if (-not $Condition) {
		throw "ASSERTION FAILED: $Message"
	}
}

function Assert-Matches {
	param(
		[Parameter(Mandatory = $true)]
		[string] $Text,

		[Parameter(Mandatory = $true)]
		[string] $Pattern,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	Assert-True -Condition ($Text -match $Pattern) -Message $Message
}

function Assert-NotMatches {
	param(
		[Parameter(Mandatory = $true)]
		[string] $Text,

		[Parameter(Mandatory = $true)]
		[string] $Pattern,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	Assert-True -Condition ($Text -notmatch $Pattern) -Message $Message
}

function Complete-Test {
	param(
		[Parameter(Mandatory = $true)]
		[string] $Name
	)

	$script:passed++
	Write-Host "PASS: $Name"
}

function Get-JustRecipe {
	param(
		[Parameter(Mandatory = $true)]
		[string] $Name
	)

	$escapedName = [Regex]::Escape($Name)
	$match = [Regex]::Match($justfile, "(?ms)^$escapedName(?:\s[^\r\n]*)?:\s*\r?\n(?<body>.*?)(?=^\S.*?:|\z)")
	Assert-True -Condition $match.Success -Message "justfile contains the $Name recipe."
	return $match.Groups['body'].Value
}

$compileRecipes = @(
	'setup',
	'dev-daemon',
	'build-mobile',
	'dev-server',
	'test',
	'build',
	'build-release',
	'check',
	'generate-types',
	'cli'
)

foreach ($recipeName in $compileRecipes) {
	$recipe = Get-JustRecipe -Name $recipeName
	Assert-Matches -Text $recipe -Pattern 'invoke-spacedrive-cargo\.ps1' -Message "$recipeName routes Cargo through the shared wrapper."
	Assert-NotMatches -Text $recipe -Pattern '(?m)^\s*cargo(?:\.exe)?\s+(?:build|test|run|check|clippy|bench|doc|xtask|ios|daemon|cli)\b' -Message "$recipeName has no bare compile-capable Cargo command."
}
Complete-Test 'compile-capable just recipes use the shared Cargo wrapper'

$tauriBuildScripts = @(
	$tauriPackage.scripts.'build:daemon',
	$tauriPackage.scripts.'build:daemon:release'
)
foreach ($script in $tauriBuildScripts) {
	Assert-Matches -Text $script -Pattern '(?i)\bpowershell\.exe\b' -Message 'Tauri daemon build script requires powershell.exe.'
	Assert-Matches -Text $script -Pattern '(?i)invoke-spacedrive-cargo\.ps1' -Message 'Tauri daemon build script uses the shared wrapper.'
	Assert-Matches -Text $script -Pattern '\$cargoArguments\s*=\s*@\(' -Message 'Tauri daemon build script builds an argument array.'
	Assert-Matches -Text $script -Pattern '@cargoArguments' -Message 'Tauri daemon build script passes arguments without concatenating a command string.'
	Assert-NotMatches -Text $script -Pattern '(?i)\bbash\s+-c\b|\bcargo\s+build\b' -Message 'Tauri daemon build script has no shell Cargo fallback.'
}
Complete-Test 'Tauri daemon package scripts use safe wrapper arguments'

$generateTypesScript = $tsClientPackage.scripts.'generate-types'
Assert-Matches -Text $generateTypesScript -Pattern '(?i)\bpowershell\.exe\b' -Message 'TypeScript client type generation requires powershell.exe.'
Assert-Matches -Text $generateTypesScript -Pattern '(?i)invoke-spacedrive-cargo\.ps1' -Message 'TypeScript client type generation uses the shared wrapper.'
Assert-NotMatches -Text $generateTypesScript -Pattern '(?i)(?:^|[;&|])\s*cargo(?:\.exe)?\s+run\b' -Message 'TypeScript client type generation has no bare Cargo run.'
Complete-Test 'TypeScript client type generation uses the shared wrapper'

$clippyTask = @($vscodeTasks.tasks | Where-Object { $_.label -eq 'rust: cargo clippy' })[0]
Assert-True -Condition ($null -ne $clippyTask) -Message 'VS Code clippy task exists.'
Assert-True -Condition ($clippyTask.type -eq 'shell') -Message 'VS Code clippy task uses a shell wrapper command.'
Assert-True -Condition ($clippyTask.command -eq 'powershell.exe') -Message 'VS Code clippy task requires powershell.exe.'
Assert-True -Condition ((@($clippyTask.args) -like '*scripts/invoke-spacedrive-cargo.ps1').Count -gt 0) -Message 'VS Code clippy task uses the shared wrapper.'
Assert-True -Condition (@($clippyTask.args) -contains 'clippy') -Message 'VS Code clippy task preserves the clippy intent.'
Assert-True -Condition (-not ($vscodeTasksSource -match '(?ms)"label"\s*:\s*"rust: cargo clippy".*?"type"\s*:\s*"cargo"')) -Message 'VS Code clippy task has no direct Cargo task type.'
Complete-Test 'VS Code clippy task uses the shared wrapper'

$runTasks = @($vscodeTasks.tasks | Where-Object { $_.label -like 'rust: run spacedrive*' })
Assert-True -Condition ($runTasks.Count -eq 2) -Message 'VS Code has both spacedrive run tasks.'
foreach ($runTask in $runTasks) {
	Assert-True -Condition ($runTask.type -eq 'shell') -Message "$($runTask.label) uses a shell wrapper command."
	Assert-True -Condition ($runTask.command -eq 'powershell.exe') -Message "$($runTask.label) requires powershell.exe."
	Assert-True -Condition ((@($runTask.args) -like '*scripts/invoke-spacedrive-cargo.ps1').Count -gt 0) -Message "$($runTask.label) uses the shared wrapper."
	Assert-True -Condition (@($runTask.args) -contains 'run') -Message "$($runTask.label) preserves the run intent."
}
Assert-True -Condition (-not ($vscodeTasksSource -match '(?ms)"label"\s*:\s*"rust: run spacedrive(?: release)?".*?"type"\s*:\s*"cargo"')) -Message 'VS Code spacedrive run tasks have no direct Cargo task type.'
Complete-Test 'VS Code spacedrive run tasks use the shared wrapper'

Assert-Matches -Text $typeCheckScript -Pattern '(?m)^if ! command -v powershell\.exe' -Message 'TypeScript drift check explicitly requires powershell.exe.'
Assert-Matches -Text $typeCheckScript -Pattern '(?i)invoke-spacedrive-cargo\.ps1' -Message 'TypeScript drift check calls the shared wrapper.'
Assert-NotMatches -Text $typeCheckScript -Pattern '(?m)^\s*cargo(?:\.exe)?\s+' -Message 'TypeScript drift check has no bare Cargo executable invocation.'
Assert-NotMatches -Text $typeCheckScript -Pattern '(?i)\bcargo\s+run\b' -Message 'TypeScript drift check has no Cargo fallback command text.'
Complete-Test 'TypeScript drift check fails clearly and uses the shared wrapper'

Assert-Matches -Text $serverScript -Pattern '(?m)^if ! command -v powershell\.exe' -Message 'Server build explicitly requires powershell.exe.'
Assert-Matches -Text $serverScript -Pattern '(?i)invoke-spacedrive-cargo\.ps1' -Message 'Server build calls the shared wrapper.'
Assert-NotMatches -Text $serverScript -Pattern '(?m)^\s*cargo(?:\.exe)?\s+' -Message 'Server build has no bare Cargo executable invocation.'
Assert-NotMatches -Text $serverScript -Pattern '(?i)\bcargo\s+build\b' -Message 'Server build has no Cargo fallback command text.'
Complete-Test 'server build fails clearly and uses the shared wrapper'

Assert-Matches -Text $autoformatScript -Pattern '(?m)^if ! has powershell\.exe; then' -Message 'Autoformat explicitly requires powershell.exe.'
Assert-Matches -Text $autoformatScript -Pattern '(?i)invoke-spacedrive-cargo\.ps1' -Message 'Autoformat calls the shared wrapper for clippy.'
Assert-Matches -Text $autoformatScript -Pattern '(?i)clippy\s+--fix\s+--all\s+--all-targets\s+--all-features' -Message 'Autoformat preserves the clippy fix arguments.'
Assert-NotMatches -Text $autoformatScript -Pattern '(?m)^\s*cargo(?:\.exe)?\s+clippy\b' -Message 'Autoformat has no bare Cargo clippy invocation.'
Complete-Test 'Autoformat clippy uses the shared wrapper'

Assert-NotMatches -Text ($justfile + $typeCheckScript + $serverScript + ($tauriBuildScripts -join "`n")) -Pattern '(?i)Invoke-Expression' -Message 'Build entrypoints do not use Invoke-Expression.'
Complete-Test 'build entrypoints avoid dynamic command evaluation'

Write-Host "All build shell entrypoint tests passed: $passed"
