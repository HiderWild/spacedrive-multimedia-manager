Set-StrictMode -Version 2.0

function ConvertTo-SpacedriveAbsolutePath {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $Path
	)

	$fullPath = [IO.Path]::GetFullPath($Path)
	$pathRoot = [IO.Path]::GetPathRoot($fullPath)
	if ([string]::Equals($fullPath, $pathRoot, [StringComparison]::OrdinalIgnoreCase)) {
		return $pathRoot
	}

	return $fullPath.TrimEnd([char[]]'\/')
}

function Test-SpacedrivePathEqual {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $Left,

		[Parameter(Mandatory = $true)]
		[string] $Right
	)

	$normalizedLeft = ConvertTo-SpacedriveAbsolutePath -Path $Left
	$normalizedRight = ConvertTo-SpacedriveAbsolutePath -Path $Right
	return [string]::Equals($normalizedLeft, $normalizedRight, [StringComparison]::OrdinalIgnoreCase)
}

function Write-SpacedriveBuildPolicyEvent {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $Event,

		[string] $EventLogPath
	)

	Write-Host "[build-policy] $Event"
	if ($EventLogPath) {
		$eventParent = Split-Path -Parent $EventLogPath
		if ($eventParent -and -not (Test-Path -LiteralPath $eventParent)) {
			New-Item -ItemType Directory -Path $eventParent -Force | Out-Null
		}
		[IO.File]::AppendAllText($EventLogPath, "$Event`r`n")
	}
}

function Invoke-SpacedriveGit {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[Parameter(Mandatory = $true)]
		[string[]] $Arguments,

		[string] $GitPath = 'git'
	)

	$normalizedRepoRoot = ConvertTo-SpacedriveAbsolutePath -Path $RepoRoot
	$output = @(& $GitPath -C $normalizedRepoRoot @Arguments 2>&1)
	if ($LASTEXITCODE -ne 0) {
		throw "git failed with exit code ${LASTEXITCODE}: $($output -join [Environment]::NewLine)"
	}

	return $output
}

function Get-RegisteredWorktreeRoots {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[string] $GitPath = 'git'
	)

	$lines = @(Invoke-SpacedriveGit -RepoRoot $RepoRoot -Arguments @('worktree', 'list', '--porcelain') -GitPath $GitPath)
	$records = New-Object System.Collections.ArrayList
	$current = $null

	foreach ($lineValue in $lines) {
		$line = $lineValue.ToString()
		if ($line.StartsWith('worktree ')) {
			if ($null -ne $current) {
				[void]$records.Add($current)
			}
			$worktreePath = ConvertTo-SpacedriveAbsolutePath -Path $line.Substring('worktree '.Length)
			$current = [pscustomobject]@{
				Path = $worktreePath
				Exists = Test-Path -LiteralPath $worktreePath -PathType Container
				Prunable = $false
				PrunableReason = $null
			}
			continue
		}

		if (($null -ne $current) -and $line.StartsWith('prunable')) {
			$current.Prunable = $true
			if ($line.Length -gt 'prunable'.Length) {
				$current.PrunableReason = $line.Substring('prunable'.Length).Trim()
			}
		}
	}

	if ($null -ne $current) {
		[void]$records.Add($current)
	}

	if ($records.Count -eq 0) {
		throw "No registered worktrees were reported for repository: $RepoRoot"
	}

	return @($records)
}

function Get-MainWorktreeRoot {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[string] $GitPath = 'git'
	)

	$main = Get-RegisteredWorktreeRoots -RepoRoot $RepoRoot -GitPath $GitPath |
		Where-Object { $_.Exists -and -not $_.Prunable } |
		Select-Object -First 1
	if ($null -eq $main) {
		throw "No existing non-prunable worktree is registered for repository: $RepoRoot"
	}

	return $main.Path
}

function Get-ArtifactRoots {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[string] $GitPath = 'git'
	)

	$worktrees = @(Get-RegisteredWorktreeRoots -RepoRoot $RepoRoot -GitPath $GitPath)
	foreach ($worktree in $worktrees) {
		if (-not $worktree.Exists -or $worktree.Prunable) {
			continue
		}

		[pscustomobject]@{
			Path = ConvertTo-SpacedriveAbsolutePath -Path (Join-Path $worktree.Path 'target')
			WorktreeRoot = $worktree.Path
			Kind = 'workspace'
		}
		[pscustomobject]@{
			Path = ConvertTo-SpacedriveAbsolutePath -Path (Join-Path $worktree.Path 'apps\tauri\src-tauri\target')
			WorktreeRoot = $worktree.Path
			Kind = 'tauri'
		}
	}
}

function Get-SpacedriveGitCommonDir {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[string] $GitPath = 'git'
	)

	$normalizedRepoRoot = ConvertTo-SpacedriveAbsolutePath -Path $RepoRoot
	$commonDirValue = (Invoke-SpacedriveGit -RepoRoot $normalizedRepoRoot -Arguments @('rev-parse', '--git-common-dir') -GitPath $GitPath | Select-Object -First 1).ToString()
	if ([IO.Path]::IsPathRooted($commonDirValue)) {
		return ConvertTo-SpacedriveAbsolutePath -Path $commonDirValue
	}

	return ConvertTo-SpacedriveAbsolutePath -Path (Join-Path $normalizedRepoRoot $commonDirValue)
}

function Get-SpacedriveContainingWorktree {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $ArtifactPath,

		[Parameter(Mandatory = $true)]
		[string[]] $WorktreeRoots
	)

	$normalizedArtifact = ConvertTo-SpacedriveAbsolutePath -Path $ArtifactPath
	foreach ($worktreeRootValue in $WorktreeRoots) {
		$worktreeRoot = ConvertTo-SpacedriveAbsolutePath -Path $worktreeRootValue
		$prefix = $worktreeRoot + [IO.Path]::DirectorySeparatorChar
		if ($normalizedArtifact.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
			return $worktreeRoot
		}
	}

	return $null
}

function Assert-SpacedrivePathHasNoReparsePoint {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $ArtifactPath,

		[Parameter(Mandatory = $true)]
		[string] $WorktreeRoot
	)

	$current = ConvertTo-SpacedriveAbsolutePath -Path $ArtifactPath
	$boundary = ConvertTo-SpacedriveAbsolutePath -Path $WorktreeRoot
	while ($true) {
		if (Test-Path -LiteralPath $current) {
			$item = Get-Item -LiteralPath $current -Force
			if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
				throw "Artifact candidate contains a reparse point: $current"
			}
		}

		if (Test-SpacedrivePathEqual -Left $current -Right $boundary) {
			break
		}

		$parent = Split-Path -Parent $current
		if (-not $parent -or (Test-SpacedrivePathEqual -Left $parent -Right $current)) {
			throw "Artifact candidate is not contained by its registered worktree: $ArtifactPath"
		}
		$current = ConvertTo-SpacedriveAbsolutePath -Path $parent
	}
}

function Assert-SafeArtifactPath {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $ArtifactPath,

		[Parameter(Mandatory = $true)]
		[string[]] $AllowedArtifactPaths,

		[Parameter(Mandatory = $true)]
		[string[]] $WorktreeRoots,

		[Parameter(Mandatory = $true)]
		[string] $GitCommonDir
	)

	$normalizedArtifact = ConvertTo-SpacedriveAbsolutePath -Path $ArtifactPath
	$driveRoot = [IO.Path]::GetPathRoot($normalizedArtifact)
	if (Test-SpacedrivePathEqual -Left $normalizedArtifact -Right $driveRoot) {
		throw "Refusing to remove a drive root: $normalizedArtifact"
	}

	if ($env:USERPROFILE -and (Test-SpacedrivePathEqual -Left $normalizedArtifact -Right $env:USERPROFILE)) {
		throw "Refusing to remove the user profile directory: $normalizedArtifact"
	}

	if (Test-SpacedrivePathEqual -Left $normalizedArtifact -Right $GitCommonDir) {
		throw "Refusing to remove the git common directory: $normalizedArtifact"
	}

	foreach ($worktreeRoot in $WorktreeRoots) {
		if (Test-SpacedrivePathEqual -Left $normalizedArtifact -Right $worktreeRoot) {
			throw "Refusing to remove a worktree root: $normalizedArtifact"
		}
		if (Test-SpacedrivePathEqual -Left $normalizedArtifact -Right (Join-Path $worktreeRoot '.git')) {
			throw "Refusing to remove a worktree .git path: $normalizedArtifact"
		}
	}

	$allowed = $false
	foreach ($allowedPath in $AllowedArtifactPaths) {
		if (Test-SpacedrivePathEqual -Left $normalizedArtifact -Right $allowedPath) {
			$allowed = $true
			break
		}
	}
	if (-not $allowed) {
		throw "Path is not an exact registered artifact candidate: $normalizedArtifact"
	}

	$containingWorktree = Get-SpacedriveContainingWorktree -ArtifactPath $normalizedArtifact -WorktreeRoots $WorktreeRoots
	if (-not $containingWorktree) {
		throw "Artifact candidate is outside every registered worktree: $normalizedArtifact"
	}

	Assert-SpacedrivePathHasNoReparsePoint -ArtifactPath $normalizedArtifact -WorktreeRoot $containingWorktree
	return $normalizedArtifact
}

function Clear-RegisteredWorktreeArtifacts {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[string] $GitPath = 'git',

		[string] $EventLogPath
	)

	$worktrees = @(Get-RegisteredWorktreeRoots -RepoRoot $RepoRoot -GitPath $GitPath)
	$artifactRoots = @(Get-ArtifactRoots -RepoRoot $RepoRoot -GitPath $GitPath)
	$allowedArtifactPaths = @($artifactRoots | ForEach-Object { $_.Path })
	$worktreeRoots = @($worktrees | ForEach-Object { $_.Path })
	$gitCommonDir = Get-SpacedriveGitCommonDir -RepoRoot $RepoRoot -GitPath $GitPath

	foreach ($worktree in $worktrees) {
		if (-not $worktree.Exists -or $worktree.Prunable) {
			$state = if ($worktree.Prunable) { 'prunable' } else { 'missing' }
			Write-SpacedriveBuildPolicyEvent -Event "worktree-skipped|$state|$($worktree.Path)" -EventLogPath $EventLogPath
			continue
		}

		$candidates = @($artifactRoots | Where-Object { Test-SpacedrivePathEqual -Left $_.WorktreeRoot -Right $worktree.Path })
		foreach ($candidate in $candidates) {
			Assert-SafeArtifactPath -ArtifactPath $candidate.Path -AllowedArtifactPaths $allowedArtifactPaths -WorktreeRoots $worktreeRoots -GitCommonDir $gitCommonDir | Out-Null
			if (-not (Test-Path -LiteralPath $candidate.Path)) {
				Write-SpacedriveBuildPolicyEvent -Event "artifact-skipped|missing|$($candidate.Path)" -EventLogPath $EventLogPath
				continue
			}

			Assert-SafeArtifactPath -ArtifactPath $candidate.Path -AllowedArtifactPaths $allowedArtifactPaths -WorktreeRoots $worktreeRoots -GitCommonDir $gitCommonDir | Out-Null
			Remove-Item -LiteralPath $candidate.Path -Recurse -Force -ErrorAction Stop
			Write-SpacedriveBuildPolicyEvent -Event "artifact-removed|$($candidate.Path)" -EventLogPath $EventLogPath
		}
	}
}

function Get-SpacedriveCargoTarget {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[string] $GitPath = 'git'
	)

	$mainRoot = Get-MainWorktreeRoot -RepoRoot $RepoRoot -GitPath $GitPath
	return ConvertTo-SpacedriveAbsolutePath -Path (Join-Path $mainRoot 'target')
}

function Test-SpacedriveCargoCompileCommand {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[AllowEmptyCollection()]
		[string[]] $CargoArguments
	)

	$compileCommands = @('build', 'test', 'run', 'check', 'clippy', 'bench', 'doc', 'xtask', 'ios', 'daemon', 'cli')
	foreach ($argument in $CargoArguments) {
		if ($compileCommands -contains $argument.ToLowerInvariant()) {
			return $true
		}
	}

	return $false
}

function Get-SpacedriveBuildLockName {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[string] $GitPath = 'git'
	)

	$commonDir = (Get-SpacedriveGitCommonDir -RepoRoot $RepoRoot -GitPath $GitPath).ToUpperInvariant()
	$sha256 = [Security.Cryptography.SHA256]::Create()
	try {
		$hashBytes = $sha256.ComputeHash([Text.Encoding]::UTF8.GetBytes($commonDir))
	} finally {
		$sha256.Dispose()
	}
	$hash = ([BitConverter]::ToString($hashBytes)).Replace('-', '').ToLowerInvariant()
	return "Local\SpacedriveCargoBuildPolicy-$hash"
}

function Enter-SpacedriveBuildLock {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[string] $GitPath = 'git',

		[int] $TimeoutMilliseconds = -1,

		[string] $EventLogPath
	)

	$lockName = Get-SpacedriveBuildLockName -RepoRoot $RepoRoot -GitPath $GitPath
	$mutex = New-Object Threading.Mutex($false, $lockName)
	Write-SpacedriveBuildPolicyEvent -Event "lock-wait|$lockName" -EventLogPath $EventLogPath
	$acquired = $false
	try {
		try {
			$acquired = $mutex.WaitOne($TimeoutMilliseconds)
		} catch [Threading.AbandonedMutexException] {
			$acquired = $true
		}
		if (-not $acquired) {
			throw "Timed out waiting for the Spacedrive Cargo build lock: $lockName"
		}
		Write-SpacedriveBuildPolicyEvent -Event "lock-acquired|$lockName" -EventLogPath $EventLogPath
		return [pscustomobject]@{
			Name = $lockName
			Mutex = $mutex
			Acquired = $true
		}
	} catch {
		$mutex.Dispose()
		throw
	}
}

function Exit-SpacedriveBuildLock {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[object] $Lock,

		[string] $EventLogPath
	)

	if ($Lock.Acquired) {
		$Lock.Mutex.ReleaseMutex()
		$Lock.Acquired = $false
	}
	$Lock.Mutex.Dispose()
	Write-SpacedriveBuildPolicyEvent -Event "lock-released|$($Lock.Name)" -EventLogPath $EventLogPath
}

function Invoke-SpacedriveCargo {
	[CmdletBinding()]
	param(
		[Parameter(Mandatory = $true)]
		[string] $RepoRoot,

		[Parameter(Mandatory = $true)]
		[AllowEmptyCollection()]
		[string[]] $CargoArguments,

		[string] $CargoPath = 'cargo',

		[string] $GitPath = 'git',

		[string] $EventLogPath
	)

	$target = Get-SpacedriveCargoTarget -RepoRoot $RepoRoot -GitPath $GitPath
	$compileCommand = Test-SpacedriveCargoCompileCommand -CargoArguments $CargoArguments
	$lock = $null
	$oldTarget = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
	try {
		if ($compileCommand) {
			$lock = Enter-SpacedriveBuildLock -RepoRoot $RepoRoot -GitPath $GitPath -EventLogPath $EventLogPath
			Clear-RegisteredWorktreeArtifacts -RepoRoot $RepoRoot -GitPath $GitPath -EventLogPath $EventLogPath
		}

		$env:CARGO_TARGET_DIR = $target
		Write-SpacedriveBuildPolicyEvent -Event "cargo-target|$target" -EventLogPath $EventLogPath
		& $CargoPath @CargoArguments
		return $LASTEXITCODE
	} finally {
		if ($null -eq $oldTarget) {
			Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
		} else {
			$env:CARGO_TARGET_DIR = $oldTarget
		}
		if ($null -ne $lock) {
			Exit-SpacedriveBuildLock -Lock $lock -EventLogPath $EventLogPath
		}
	}
}
