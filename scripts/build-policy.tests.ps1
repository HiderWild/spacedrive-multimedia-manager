[CmdletBinding()]
param()

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$policyPath = Join-Path $scriptRoot 'build-policy.ps1'
$wrapperPath = Join-Path $scriptRoot 'invoke-spacedrive-cargo.ps1'
$fixtureRoot = Join-Path $env:TEMP ("spacedrive-build-policy-{0}" -f [Guid]::NewGuid().ToString('N'))
$passed = 0
$ownedJobs = New-Object System.Collections.ArrayList

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
		[AllowNull()]
		[object] $Expected,

		[AllowNull()]
		[object] $Actual,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	if ($Expected -ne $Actual) {
		throw "ASSERT EQUAL FAILED: $Message. Expected=[$Expected] Actual=[$Actual]"
	}
}

function Assert-PathExists {
	param(
		[Parameter(Mandatory = $true)]
		[string] $LiteralPath,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	Assert-True -Condition (Test-Path -LiteralPath $LiteralPath) -Message $Message
}

function Assert-PathMissing {
	param(
		[Parameter(Mandatory = $true)]
		[string] $LiteralPath,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	Assert-True -Condition (-not (Test-Path -LiteralPath $LiteralPath)) -Message $Message
}

function Assert-Throws {
	param(
		[Parameter(Mandatory = $true)]
		[scriptblock] $Action,

		[Parameter(Mandatory = $true)]
		[string] $MessagePattern,

		[Parameter(Mandatory = $true)]
		[string] $Message
	)

	$thrown = $false
	try {
		& $Action
	} catch {
		$thrown = $true
		if ($_.Exception.Message -notmatch $MessagePattern) {
			throw "ASSERT THROWS FAILED: $Message. Unexpected error: $($_.Exception.Message)"
		}
	}

	if (-not $thrown) {
		throw "ASSERT THROWS FAILED: $Message. No exception was thrown."
	}
}

function Complete-Test {
	param([Parameter(Mandatory = $true)][string] $Name)

	$script:passed++
	Write-Host "PASS: $Name"
}

function New-TextFile {
	param(
		[Parameter(Mandatory = $true)]
		[string] $LiteralPath,

		[string] $Value = 'sentinel'
	)

	$parent = Split-Path -Parent $LiteralPath
	if (-not (Test-Path -LiteralPath $parent)) {
		New-Item -ItemType Directory -Path $parent -Force | Out-Null
	}
	Set-Content -LiteralPath $LiteralPath -Value $Value -Encoding UTF8
}

function Invoke-Git {
	param(
		[Parameter(Mandatory = $true)]
		[string[]] $Arguments
	)

	$oldErrorActionPreference = $ErrorActionPreference
	try {
		$ErrorActionPreference = 'Continue'
		$output = @(& git @Arguments 2>&1)
		$exitCode = $LASTEXITCODE
	} finally {
		$ErrorActionPreference = $oldErrorActionPreference
	}
	if ($exitCode -ne 0) {
		throw "git failed (${exitCode}): git $($Arguments -join ' ')`n$($output -join "`n")"
	}
	return $output
}

function Get-NormalizedTestPath {
	param([Parameter(Mandatory = $true)][string] $LiteralPath)

	return [IO.Path]::GetFullPath($LiteralPath).TrimEnd([char[]]'\/')
}

function Wait-Until {
	param(
		[Parameter(Mandatory = $true)]
		[scriptblock] $Condition,

		[int] $TimeoutMilliseconds = 5000,

		[string] $FailureMessage = 'Timed out waiting for condition.'
	)

	$stopwatch = [Diagnostics.Stopwatch]::StartNew()
	while ($stopwatch.ElapsedMilliseconds -lt $TimeoutMilliseconds) {
		if (& $Condition) {
			return
		}
		Start-Sleep -Milliseconds 50
	}

	throw $FailureMessage
}

function New-FakeCargo {
	param([Parameter(Mandatory = $true)][string] $Directory)

	$fakeScript = Join-Path $Directory 'fake-cargo.ps1'
	$fakeCommand = Join-Path $Directory 'fake-cargo.cmd'
	$fakeBody = @'
$ErrorActionPreference = 'Stop'
$CargoArguments = @($args)
$label = 'default'
$sleepMilliseconds = 0
$exitCode = 0
for ($index = 0; $index -lt $CargoArguments.Count; $index++) {
	switch ($CargoArguments[$index]) {
		'--label' {
			$index++
			$label = $CargoArguments[$index]
		}
		'--sleep-ms' {
			$index++
			$sleepMilliseconds = [int]$CargoArguments[$index]
		}
		'--exit-code' {
			$index++
			$exitCode = [int]$CargoArguments[$index]
		}
	}
}

$eventLog = Join-Path $PSScriptRoot 'events.log'
$argumentsJson = ConvertTo-Json -InputObject @($CargoArguments) -Compress
$target = $env:CARGO_TARGET_DIR
[IO.Directory]::CreateDirectory($target) | Out-Null
Set-Content -LiteralPath (Join-Path $target "$label.in-use") -Value $label -Encoding UTF8
[IO.File]::AppendAllText($eventLog, "cargo-start|$label|$target|$argumentsJson`r`n")
Write-Output "fake-stdout|$label"
[Console]::Error.WriteLine("fake-stderr|$label")
if ($sleepMilliseconds -gt 0) {
	Start-Sleep -Milliseconds $sleepMilliseconds
}
[IO.File]::AppendAllText($eventLog, "cargo-end|$label|$target|$exitCode`r`n")
exit $exitCode
'@
	$fakeCommandBody = @'
@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0fake-cargo.ps1" %*
exit /b %ERRORLEVEL%
'@

	Set-Content -LiteralPath $fakeScript -Value $fakeBody -Encoding UTF8
	Set-Content -LiteralPath $fakeCommand -Value $fakeCommandBody -Encoding ASCII
	return $fakeCommand
}

function Start-WrapperJob {
	param(
		[Parameter(Mandatory = $true)]
		[string] $PowerShellPath,

		[Parameter(Mandatory = $true)]
		[string] $Wrapper,

		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[Parameter(Mandatory = $true)]
		[string] $CargoPath,

		[Parameter(Mandatory = $true)]
		[string] $EventLog,

		[Parameter(Mandatory = $true)]
		[string] $Label,

		[int] $SleepMilliseconds = 0
	)

	return Start-Job -ScriptBlock {
		param($Exe, $WrapperPath, $Repository, $FakeCargo, $LogPath, $InvocationLabel, $Delay)
		& $Exe -NoProfile -ExecutionPolicy Bypass -File $WrapperPath `
			-RepoRoot $Repository `
			-CargoPath $FakeCargo `
			-PolicyEventLogPath $LogPath `
			build --label $InvocationLabel --sleep-ms $Delay
		return $LASTEXITCODE
	} -ArgumentList $PowerShellPath, $Wrapper, $RepoRoot, $CargoPath, $EventLog, $Label, $SleepMilliseconds
}

try {
	if (-not (Test-Path -LiteralPath $policyPath)) {
		throw "RED: missing production policy script: $policyPath"
	}
	if (-not (Test-Path -LiteralPath $wrapperPath)) {
		throw "RED: missing production wrapper script: $wrapperPath"
	}

	. $policyPath

	$mainRoot = Join-Path $fixtureRoot 'main'
	$linkedRoot = Join-Path $fixtureRoot 'linked'
	$prunableRoot = Join-Path $fixtureRoot 'prunable'
	$externalRoot = Join-Path $fixtureRoot 'external'
	New-Item -ItemType Directory -Path $mainRoot, $externalRoot -Force | Out-Null

	Invoke-Git -Arguments @('-C', $mainRoot, 'init') | Out-Null
	Invoke-Git -Arguments @('-C', $mainRoot, 'config', 'user.email', 'build-policy-tests@example.invalid') | Out-Null
	Invoke-Git -Arguments @('-C', $mainRoot, 'config', 'user.name', 'Build Policy Tests') | Out-Null
	New-TextFile -LiteralPath (Join-Path $mainRoot 'source.txt') -Value 'source sentinel'
	Invoke-Git -Arguments @('-C', $mainRoot, 'add', 'source.txt') | Out-Null
	Invoke-Git -Arguments @('-C', $mainRoot, 'commit', '-m', 'fixture') | Out-Null
	Invoke-Git -Arguments @('-C', $mainRoot, 'worktree', 'add', '-b', 'fixture-linked', $linkedRoot) | Out-Null
	Invoke-Git -Arguments @('-C', $mainRoot, 'worktree', 'add', '-b', 'fixture-prunable', $prunableRoot) | Out-Null

	Remove-Item -LiteralPath $prunableRoot -Recurse -Force
	$porcelain = Invoke-Git -Arguments @('-C', $mainRoot, 'worktree', 'list', '--porcelain')
	Assert-True -Condition (($porcelain -join "`n") -match [regex]::Escape((Get-NormalizedTestPath -LiteralPath $prunableRoot).Replace('\', '/'))) -Message 'Deleted linked worktree remains registered.'
	Assert-True -Condition (($porcelain -join "`n") -match 'prunable') -Message 'Deleted linked worktree is reported as prunable.'
	Complete-Test 'fixture has a registered prunable worktree'

	$commonDirText = (Invoke-Git -Arguments @('-C', $mainRoot, 'rev-parse', '--git-common-dir') | Select-Object -First 1).ToString()
	if ([IO.Path]::IsPathRooted($commonDirText)) {
		$gitCommonDir = Get-NormalizedTestPath -LiteralPath $commonDirText
	} else {
		$gitCommonDir = Get-NormalizedTestPath -LiteralPath (Join-Path $mainRoot $commonDirText)
	}

	$sourceSentinel = Join-Path $mainRoot 'source.txt'
	$externalSentinel = Join-Path $externalRoot 'external-sentinel.txt'
	$commonSentinel = Join-Path $gitCommonDir 'common-sentinel.txt'
	$mainTargetSentinel = Join-Path $mainRoot 'target\main-target.txt'
	$mainTauriSentinel = Join-Path $mainRoot 'apps\tauri\src-tauri\target\main-tauri-target.txt'
	$linkedTargetSentinel = Join-Path $linkedRoot 'target\linked-target.txt'
	$linkedTauriSentinel = Join-Path $linkedRoot 'apps\tauri\src-tauri\target\linked-tauri-target.txt'
	New-TextFile -LiteralPath $externalSentinel
	New-TextFile -LiteralPath $commonSentinel
	New-TextFile -LiteralPath $mainTargetSentinel
	New-TextFile -LiteralPath $mainTauriSentinel
	New-TextFile -LiteralPath $linkedTargetSentinel
	New-TextFile -LiteralPath $linkedTauriSentinel

	$registered = @(Get-RegisteredWorktreeRoots -RepoRoot $mainRoot)
	Assert-Equal -Expected 3 -Actual $registered.Count -Message 'Every registered worktree is returned.'
	$prunable = @($registered | Where-Object { $_.Path -eq (Get-NormalizedTestPath -LiteralPath $prunableRoot) })
	Assert-Equal -Expected 1 -Actual $prunable.Count -Message 'Prunable worktree record is preserved.'
	Assert-True -Condition $prunable[0].Prunable -Message 'Prunable worktree is marked.'
	Assert-True -Condition (-not $prunable[0].Exists) -Message 'Missing prunable worktree is marked nonexistent.'
	Complete-Test 'registered worktree discovery preserves prunable records'

	$resolvedMain = Get-MainWorktreeRoot -RepoRoot $linkedRoot
	Assert-Equal -Expected (Get-NormalizedTestPath -LiteralPath $mainRoot) -Actual $resolvedMain -Message 'The first normal worktree is the main root.'
	Complete-Test 'main worktree selection is stable from a linked worktree'

	$artifactRoots = @(Get-ArtifactRoots -RepoRoot $linkedRoot)
	Assert-Equal -Expected 4 -Actual $artifactRoots.Count -Message 'Two artifact roots are returned for each existing worktree.'
	Assert-True -Condition ($artifactRoots.Path -contains (Get-NormalizedTestPath -LiteralPath (Join-Path $mainRoot 'target'))) -Message 'Main target is a candidate.'
	Assert-True -Condition ($artifactRoots.Path -contains (Get-NormalizedTestPath -LiteralPath (Join-Path $linkedRoot 'apps\tauri\src-tauri\target'))) -Message 'Linked Tauri target is a candidate.'
	Complete-Test 'artifact discovery is limited to root and Tauri targets'

	$cleanupLog = Join-Path $fixtureRoot 'cleanup.log'
	Clear-RegisteredWorktreeArtifacts -RepoRoot $linkedRoot -EventLogPath $cleanupLog
	Assert-PathMissing -LiteralPath (Join-Path $mainRoot 'target') -Message 'Main target is deleted.'
	Assert-PathMissing -LiteralPath (Join-Path $mainRoot 'apps\tauri\src-tauri\target') -Message 'Main Tauri target is deleted.'
	Assert-PathMissing -LiteralPath (Join-Path $linkedRoot 'target') -Message 'Linked target is deleted.'
	Assert-PathMissing -LiteralPath (Join-Path $linkedRoot 'apps\tauri\src-tauri\target') -Message 'Linked Tauri target is deleted.'
	Assert-PathExists -LiteralPath $sourceSentinel -Message 'Source sentinel survives cleanup.'
	Assert-PathExists -LiteralPath $externalSentinel -Message 'External sentinel survives cleanup.'
	Assert-PathExists -LiteralPath $commonSentinel -Message 'Git common-dir sentinel survives cleanup.'
	$cleanupEvents = @(Get-Content -LiteralPath $cleanupLog)
	Assert-True -Condition (($cleanupEvents -join "`n") -match 'worktree-skipped.*prunable') -Message 'Prunable nonexistent worktree is logged as skipped.'
	Complete-Test 'cleanup deletes only registered artifact candidates'

	$worktreePaths = @($registered | ForEach-Object { $_.Path })
	$allowedPaths = @($artifactRoots | ForEach-Object { $_.Path })
	$driveRoot = [IO.Path]::GetPathRoot($mainRoot)
	$linkedGitPath = Join-Path $linkedRoot '.git'
	Assert-Throws -Action { Assert-SafeArtifactPath -ArtifactPath $mainRoot -AllowedArtifactPaths @($mainRoot) -WorktreeRoots $worktreePaths -GitCommonDir $gitCommonDir } -MessagePattern 'worktree root' -Message 'Worktree root is rejected even if supplied as allowed.'
	Assert-Throws -Action { Assert-SafeArtifactPath -ArtifactPath $gitCommonDir -AllowedArtifactPaths @($gitCommonDir) -WorktreeRoots $worktreePaths -GitCommonDir $gitCommonDir } -MessagePattern 'git common' -Message 'Git common directory is rejected even if supplied as allowed.'
	Assert-Throws -Action { Assert-SafeArtifactPath -ArtifactPath $linkedGitPath -AllowedArtifactPaths @($linkedGitPath) -WorktreeRoots $worktreePaths -GitCommonDir $gitCommonDir } -MessagePattern '\.git' -Message 'Worktree .git path is rejected.'
	Assert-Throws -Action { Assert-SafeArtifactPath -ArtifactPath $driveRoot -AllowedArtifactPaths @($driveRoot) -WorktreeRoots $worktreePaths -GitCommonDir $gitCommonDir } -MessagePattern 'drive root' -Message 'Drive root is rejected even if supplied as allowed.'
	Assert-Throws -Action { Assert-SafeArtifactPath -ArtifactPath $externalRoot -AllowedArtifactPaths $allowedPaths -WorktreeRoots $worktreePaths -GitCommonDir $gitCommonDir } -MessagePattern 'registered artifact candidate' -Message 'External path is rejected.'
	if ($env:USERPROFILE) {
		Assert-Throws -Action { Assert-SafeArtifactPath -ArtifactPath $env:USERPROFILE -AllowedArtifactPaths @($env:USERPROFILE) -WorktreeRoots $worktreePaths -GitCommonDir $gitCommonDir } -MessagePattern 'user profile' -Message 'User profile is rejected even if supplied as allowed.'
	}
	Assert-PathExists -LiteralPath $sourceSentinel -Message 'Source sentinel survives rejected broad paths.'
	Assert-PathExists -LiteralPath $externalSentinel -Message 'External sentinel survives rejected external path.'
	Assert-PathExists -LiteralPath $commonSentinel -Message 'Git common-dir sentinel survives rejected Git paths.'
	Complete-Test 'broad and candidate-external paths are rejected without deletion'

	$junctionDestination = Join-Path $externalRoot 'junction-destination'
	$junctionSentinel = Join-Path $junctionDestination 'junction-sentinel.txt'
	New-TextFile -LiteralPath $junctionSentinel
	$junctionCandidate = Join-Path $mainRoot 'target'
	New-Item -ItemType Junction -Path $junctionCandidate -Target $junctionDestination | Out-Null
	Assert-Throws -Action { Assert-SafeArtifactPath -ArtifactPath $junctionCandidate -AllowedArtifactPaths @($junctionCandidate) -WorktreeRoots $worktreePaths -GitCommonDir $gitCommonDir } -MessagePattern 'reparse' -Message 'Junction artifact candidate is rejected.'
	Assert-Throws -Action { Clear-RegisteredWorktreeArtifacts -RepoRoot $mainRoot -EventLogPath $cleanupLog } -MessagePattern 'reparse' -Message 'Cleanup fails rather than swallowing a junction rejection.'
	Assert-PathExists -LiteralPath $junctionSentinel -Message 'Junction destination sentinel survives rejected cleanup.'
	[IO.Directory]::Delete($junctionCandidate)
	Assert-PathExists -LiteralPath $junctionSentinel -Message 'Removing the fixture junction does not remove its target.'
	Complete-Test 'reparse artifact candidates fail closed'

	$raceParent = Join-Path $linkedRoot 'apps\tauri\src-tauri'
	$raceBackup = Join-Path $linkedRoot 'apps\tauri\src-tauri-original'
	$raceCandidate = Join-Path $raceParent 'target'
	$raceExternal = Join-Path $externalRoot 'parent-replacement'
	$raceExternalSentinel = Join-Path $raceExternal 'target\external-sentinel.txt'
	New-TextFile -LiteralPath $raceExternalSentinel
	New-TextFile -LiteralPath (Join-Path $raceCandidate 'before-parent-replacement.txt')
	# This hook covers a deterministic replacement before the final delete check. It does not prove atomic defense after that check.
	$replaceParentAfterInitialCheck = {
		param([string] $candidatePath)
		if ((Get-NormalizedTestPath -LiteralPath $candidatePath) -ne (Get-NormalizedTestPath -LiteralPath $raceCandidate)) {
			return
		}
		Move-Item -LiteralPath $raceParent -Destination $raceBackup
		New-Item -ItemType Junction -Path $raceParent -Target $raceExternal | Out-Null
	}
	try {
		Assert-Throws -Action { Clear-RegisteredWorktreeArtifacts -RepoRoot $linkedRoot -EventLogPath $cleanupLog -BeforeArtifactDelete $replaceParentAfterInitialCheck } -MessagePattern 'reparse' -Message 'Deterministic parent replacement before delete is rejected.'
		Assert-PathExists -LiteralPath $raceExternalSentinel -Message 'Parent junction replacement does not delete the external sentinel.'
	} finally {
		if (Test-Path -LiteralPath $raceParent) {
			$raceParentItem = Get-Item -LiteralPath $raceParent -Force
			if (($raceParentItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
				[IO.Directory]::Delete($raceParent)
			}
		}
		if (Test-Path -LiteralPath $raceBackup) {
			Move-Item -LiteralPath $raceBackup -Destination $raceParent
		}
	}
	Write-Host 'INFO: parent reparse fixture covers the deterministic pre-delete window only; post-validation TOCTOU remains out of scope.'
	Complete-Test 'parent reparse replacement before delete is rejected (deterministic window only)'

	foreach ($command in @('build', 'test', 'run', 'check', 'clippy', 'bench', 'doc', 'xtask', 'dev')) {
		Assert-True -Condition (Test-SpacedriveCargoCompileCommand -CargoArguments @($command, '--locked')) -Message "$command is compile-producing."
	}
	foreach ($alias in @('ios', 'daemon', 'cli')) {
		Assert-True -Condition (Test-SpacedriveCargoCompileCommand -CargoArguments @($alias)) -Message "Cargo alias $alias is compile-producing."
	}
	Assert-True -Condition (Test-SpacedriveCargoCompileCommand -CargoArguments @('+stable', '--locked', 'test', '-p', 'sd-core')) -Message 'Toolchain and global argument variants are recognized.'
	Assert-True -Condition (-not (Test-SpacedriveCargoCompileCommand -CargoArguments @('fmt', '--check'))) -Message 'cargo fmt is not treated as compile-producing.'
	Complete-Test 'compile-producing Cargo commands and variants are detected'

	Assert-Equal -Expected (Get-NormalizedTestPath -LiteralPath (Join-Path $mainRoot 'target')) -Actual (Get-SpacedriveCargoTarget -RepoRoot $linkedRoot) -Message 'Shared Cargo target is main-root target.'
	Complete-Test 'shared Cargo target ignores linked roots and ambient target settings'

	$fakeCargo = New-FakeCargo -Directory $fixtureRoot
	$eventLog = Join-Path $fixtureRoot 'events.log'
	New-TextFile -LiteralPath (Join-Path $mainRoot 'target\before-wrapper.txt')
	New-TextFile -LiteralPath (Join-Path $linkedRoot 'target\before-wrapper.txt')
	New-TextFile -LiteralPath (Join-Path $mainRoot 'apps\tauri\src-tauri\target\before-wrapper.txt')
	$forwardedArguments = @('test', '-p', 'sd-core', '--features', 'alpha,beta', '--', '--nocapture', '--label', 'forwarded', '--exit-code', '7')
	$oldAmbientTarget = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
	$oldWrapperErrorActionPreference = $ErrorActionPreference
	try {
		$ErrorActionPreference = 'Continue'
		$env:CARGO_TARGET_DIR = $externalRoot
	$wrapperOutput = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $wrapperPath `
		-RepoRoot $linkedRoot `
		-CargoPath $fakeCargo `
		-PolicyEventLogPath $eventLog `
		@forwardedArguments 2>&1 | ForEach-Object { $_.ToString() })
	$wrapperExitCode = $LASTEXITCODE
	} finally {
		$ErrorActionPreference = $oldWrapperErrorActionPreference
		if ($null -eq $oldAmbientTarget) {
			Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
		} else {
			$env:CARGO_TARGET_DIR = $oldAmbientTarget
		}
	}
	Assert-Equal -Expected 7 -Actual $wrapperExitCode -Message 'Wrapper returns fake Cargo exit code unchanged.'
	Assert-True -Condition (($wrapperOutput -join "`n") -match 'fake-stdout\|forwarded') -Message 'Wrapper forwards Cargo stdout in real time.'
	Assert-True -Condition (($wrapperOutput -join "`n") -match 'fake-stderr\|forwarded') -Message 'Wrapper forwards Cargo stderr in real time.'
	Assert-PathExists -LiteralPath $externalSentinel -Message 'Ambient CARGO_TARGET_DIR is never used as a cleanup target.'
	$events = @(Get-Content -LiteralPath $eventLog)
	$cargoStart = @($events | Where-Object { $_ -like 'cargo-start|forwarded|*' })
	Assert-Equal -Expected 1 -Actual $cargoStart.Count -Message 'Fake Cargo is invoked exactly once.'
	$startParts = $cargoStart[0] -split '\|', 4
	Assert-Equal -Expected (Get-NormalizedTestPath -LiteralPath (Join-Path $mainRoot 'target')) -Actual (Get-NormalizedTestPath -LiteralPath $startParts[2]) -Message 'Fake Cargo receives main target through CARGO_TARGET_DIR.'
	$recordedArguments = @()
	foreach ($recordedArgument in (ConvertFrom-Json -InputObject $startParts[3])) {
		$recordedArguments += $recordedArgument.ToString()
	}
	Assert-Equal -Expected ($forwardedArguments -join "`0") -Actual ($recordedArguments -join "`0") -Message 'Cargo arguments preserve order and values.'
	$firstCleanupIndex = -1
	$cargoStartIndex = -1
	for ($index = 0; $index -lt $events.Count; $index++) {
		if (($firstCleanupIndex -lt 0) -and ($events[$index] -like 'artifact-removed|*')) {
			$firstCleanupIndex = $index
		}
		if ($events[$index] -like 'cargo-start|forwarded|*') {
			$cargoStartIndex = $index
		}
	}
	Assert-True -Condition (($firstCleanupIndex -ge 0) -and ($firstCleanupIndex -lt $cargoStartIndex)) -Message 'Cleanup is recorded before Cargo starts.'
	Complete-Test 'wrapper cleans, forwards arguments, sets target, and preserves exit code'

	Remove-Item -LiteralPath $eventLog -Force
	New-TextFile -LiteralPath (Join-Path $mainRoot 'target\before-concurrency.txt')
	$powerShellPath = (Get-Command powershell.exe).Source
	$firstJob = Start-WrapperJob -PowerShellPath $powerShellPath -Wrapper $wrapperPath -RepoRoot $linkedRoot -CargoPath $fakeCargo -EventLog $eventLog -Label 'first' -SleepMilliseconds 1800
	[void]$ownedJobs.Add($firstJob)
	Wait-Until -Condition { (Test-Path -LiteralPath $eventLog) -and ((Get-Content -LiteralPath $eventLog -Raw) -match 'cargo-start\|first\|') } -FailureMessage 'First fake Cargo did not start.'
	$firstMarker = Join-Path $mainRoot 'target\first.in-use'
	Assert-PathExists -LiteralPath $firstMarker -Message 'First fake Cargo marker exists while it runs.'

	$secondJob = Start-WrapperJob -PowerShellPath $powerShellPath -Wrapper $wrapperPath -RepoRoot $linkedRoot -CargoPath $fakeCargo -EventLog $eventLog -Label 'second' -SleepMilliseconds 0
	[void]$ownedJobs.Add($secondJob)
	Start-Sleep -Milliseconds 400
	$duringFirst = Get-Content -LiteralPath $eventLog -Raw
	Assert-True -Condition ($duringFirst -notmatch 'cargo-start\|second\|') -Message 'Second wrapper waits before starting Cargo.'
	Assert-PathExists -LiteralPath $firstMarker -Message 'Second wrapper cannot clean shared target while first Cargo runs.'

	$completedJobs = @(Wait-Job -Job $firstJob, $secondJob -Timeout 15)
	Assert-Equal -Expected 2 -Actual $completedJobs.Count -Message 'Both wrapper jobs finish.'
	$oldReceiveErrorActionPreference = $ErrorActionPreference
	try {
		$ErrorActionPreference = 'Continue'
		$firstResult = @(Receive-Job -Job $firstJob -ErrorAction SilentlyContinue)
		$secondResult = @(Receive-Job -Job $secondJob -ErrorAction SilentlyContinue)
	} finally {
		$ErrorActionPreference = $oldReceiveErrorActionPreference
	}
	Assert-Equal -Expected 0 -Actual ([int]$firstResult[-1]) -Message 'First concurrent wrapper succeeds.'
	Assert-Equal -Expected 0 -Actual ([int]$secondResult[-1]) -Message 'Second concurrent wrapper succeeds.'
	$concurrentEvents = @(Get-Content -LiteralPath $eventLog)
	$firstEndIndex = [Array]::IndexOf($concurrentEvents, (@($concurrentEvents | Where-Object { $_ -like 'cargo-end|first|*' })[0]))
	$secondStartIndex = [Array]::IndexOf($concurrentEvents, (@($concurrentEvents | Where-Object { $_ -like 'cargo-start|second|*' })[0]))
	Assert-True -Condition (($firstEndIndex -ge 0) -and ($secondStartIndex -gt $firstEndIndex)) -Message 'Second Cargo starts only after first Cargo exits.'
	Complete-Test 'cross-process lock covers cleanup through Cargo exit'

	Write-Host "ALL TESTS PASSED: $passed"
} finally {
	foreach ($ownedJob in @($ownedJobs)) {
		Stop-Job -Job $ownedJob -ErrorAction SilentlyContinue
		Remove-Job -Job $ownedJob -Force -ErrorAction SilentlyContinue
	}
	if (Test-Path -LiteralPath $fixtureRoot) {
		$resolvedFixture = Get-NormalizedTestPath -LiteralPath $fixtureRoot
		$resolvedTemp = Get-NormalizedTestPath -LiteralPath $env:TEMP
		if (-not $resolvedFixture.StartsWith($resolvedTemp + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
			throw "Refusing to remove fixture outside TEMP: $resolvedFixture"
		}
		Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
	}
}
