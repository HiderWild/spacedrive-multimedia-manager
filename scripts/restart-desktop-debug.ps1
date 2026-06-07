# Restart Spacedrive debug environment (backend daemon + Tauri desktop app).
param(
    [string] $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string] $Cargo = "",
    [string] $Bun = "",
    [int] $DaemonPort = 8488,
    [ValidateSet("Debug", "Release")]
    [string] $BuildProfile = "Debug",
    [switch] $SkipRebuild,
    [string[]] $KillPorts = @("1420", "6969", "8488", "12917")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$knownBinaryNames = @("Spacedrive", "Spacedrive.exe", "sd-daemon", "sd-daemon.exe", "sd-desktop", "sd-desktop.exe")

function Get-TcpExcludedPortRanges {
    param([ValidateSet("ipv4", "ipv6")][string] $Protocol = "ipv4")

    $ranges = @()
    try {
        $raw = netsh interface $Protocol show excludedportrange protocol=tcp 2>$null | Out-String
    } catch {
        return $ranges
    }

    foreach ($line in ($raw -split "`r?`n")) {
        if ($line -match "^\s*(\d+)\s+(\d+)\s*$") {
            $start = [int]$Matches[1]
            $end = [int]$Matches[2]
            if ($start -gt 0 -and $end -ge $start -and $end -le 65535) {
                $ranges += [PSCustomObject]@{ Start = $start; End = $end }
            }
        }
    }
    return $ranges
}

function Test-PortIsExcluded {
    param([int]$Port)

    if ($Port -le 0 -or $Port -gt 65535) {
        return $false
    }

    $ranges = Get-TcpExcludedPortRanges -Protocol ipv4
    $ranges += Get-TcpExcludedPortRanges -Protocol ipv6
    foreach ($range in $ranges) {
        if ($Port -ge $range.Start -and $Port -le $range.End) {
            return $true
        }
    }
    return $false
}

function Get-ListeningPids {
    param([int]$Port)

    $pids = @()
    try {
        $listeners = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
        if ($listeners) {
            $pids += $listeners | Select-Object -ExpandProperty OwningProcess
        }
    } catch {
        # Fallback for environments where Get-NetTCPConnection is unavailable.
        try {
            $netstatLines = netstat -ano -p tcp 2>$null | Select-String -Pattern '\bLISTENING\b' |
                ForEach-Object { $_.Line }
            foreach ($line in $netstatLines) {
                if ($line -notmatch ":$Port") {
                    continue
                }
                if ($line -match '^\s*TCP\s+([^\s]+)\s+([^\s]+)\s+LISTENING\s+(\d+)') {
                    $pids += [int]$Matches[3]
                }
            }
        } catch {
            Write-Host "Port probe failed for ${Port}: $($_.Exception.Message)"
        }
    }
    return ($pids | Sort-Object -Unique)
}

function Stop-ProcessTree {
    param([int[]]$ProcessIds)

    if (-not $ProcessIds -or $ProcessIds.Count -eq 0) {
        return
    }

    foreach ($id in ($ProcessIds | Sort-Object -Unique)) {
        try {
            Write-Host "Stopping PID $id..."
            taskkill.exe /PID $id /T /F | Out-Null
        } catch {
            Write-Host "Skip stop PID ${id}: $($_.Exception.Message)"
        }
    }
}

function Get-RepoProcessCandidates {
    param([string]$ProjectPath)

    $projectEscaped = [Regex]::Escape($ProjectPath)
    $projectEscaped = $projectEscaped.TrimEnd("\\")
    $processes = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue

    $tauriDirEscaped = [Regex]::Escape((Join-Path $ProjectPath "apps\tauri"))
    $webDirEscaped = [Regex]::Escape((Join-Path $ProjectPath "apps\web"))
    $daemonNamePatterns = @("sd-daemon.exe", "sd-daemon", "Spacedrive.exe", "Spacedrive", "sd-desktop.exe", "sd-desktop")
    $scriptRunnerNames = @("bun.exe", "bun", "node.exe", "node", "cargo.exe", "cargo", "rustc.exe", "rustc", "pnpm.exe", "pnpm", "tauri.exe", "tauri", "vite.exe", "vite")
    $cliPattern = 'bun run tauri:dev|bun run dev:with-daemon|bun run tauri|@tauri-apps\\cli\\tauri|tauri dev|sd-daemon|cargo run --bin sd-daemon|cargo build .*--bin sd-daemon|cargo build .* --bin sd-daemon|vite dev|bun run dev|cargo build --bin sd-daemon'
    $spacedriveHintPattern = 'spacedrive|Spacedrive'

    $results = @()
    $debugNamePattern = '^((?i)(Spacedrive|sd-daemon|sd-desktop|bun|node|cargo|rustc|pnpm|tauri|vite))(\.exe)?$'

    foreach ($proc in $processes) {
        if ($proc.ProcessId -eq $PID -or (-not $proc.Name)) {
            continue
        }

        $name = $proc.Name
        $cmd = $proc.CommandLine
        $execPath = $proc.ExecutablePath

        if ($daemonNamePatterns -contains $name) {
            $results += $proc
            continue
        }

        if ($name -notmatch $debugNamePattern) {
            continue
        }

        $matchesProject = ($cmd -and ($cmd -match $projectEscaped -or $cmd -match $spacedriveHintPattern)) -or
            ($execPath -and $execPath -match $projectEscaped)

        if (($name -in $scriptRunnerNames) -and $matchesProject -and ($cmd -match $cliPattern)) {
            $results += $proc
            continue
        }

        # For lightweight helper processes that may not include a command line pattern, still allow
        # matching when project path appears in the executable path or command line.
        if ($matchesProject -and ($name -in $scriptRunnerNames)) {
            $results += $proc
            continue
        }

        if (($name -in $daemonNamePatterns) -and $matchesProject) {
            $results += $proc
            continue
        }

        if (
            $name -match $debugNamePattern -and (
                ($cmd -match $projectEscaped) -or
                ($cmd -match $tauriDirEscaped -and $cmd -match "bun run tauri:dev|tauri dev|dev:with-daemon|@tauri-apps\\cli\\tauri") -or
                ($cmd -match $webDirEscaped -and $cmd -match "bun run dev|vite")
            )
        ) {
            $results += $proc
        }

        # Keep compatibility with previous behavior where command line may exist but not match
        # any explicit pattern yet still belongs to current repo process tree.
        if (
            $matchesProject -and ($cmd -match $cliPattern)
        ) {
            $results += $proc
        }
    }

    return $results | Select-Object -Unique ProcessId, Name, CommandLine, ExecutablePath
}

function Stop-ProcessByNameCandidates {
    param([string[]]$ProcessNames)

    foreach ($name in ($ProcessNames | Sort-Object -Unique)) {
        try {
            $procs = Get-CimInstance Win32_Process -Filter "Name='$name'" -ErrorAction SilentlyContinue
            if (-not $procs) {
                continue
            }
            $ids = @($procs | Select-Object -ExpandProperty ProcessId | Sort-Object -Unique)
            if ($ids) {
                Write-Host "Stopping processes by exact name '$name': $($ids -join ', ')"
                Stop-ProcessTree -ProcessIds $ids
            }
        } catch {
            Write-Host "Unable to stop by name '$name': $($_.Exception.Message)"
        }
    }
}

function Wait-ForStop {
    param([int[]]$ProcessIds, [int]$TimeoutSec = 8)

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $running = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object { $ProcessIds -contains $_.ProcessId }
        if (-not $running) {
            return
        }
        Start-Sleep -Seconds 1
    }

    Write-Host "Some old debug processes still running; forcing final stop..."
    Stop-ProcessTree -ProcessIds $ProcessIds
}

function Stop-ProcessOnPort {
    param([int]$Port)

    try {
        $processIds = Get-ListeningPids -Port $Port
    } catch {
        Write-Host "Unable to query listeners on port ${Port}: $($_.Exception.Message)"
        return
    }

    if (-not $processIds) {
        return
    }

    if ($processIds.Count -gt 0) {
        Write-Host "Port ${Port} is used by PIDs: $($processIds -join ', '). Stopping them."
        Stop-ProcessTree -ProcessIds $processIds
        $deadline = (Get-Date).AddSeconds(8)
        while ((Get-Date) -lt $deadline) {
            $running = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
                Where-Object { $processIds -contains $_.ProcessId }
            if (-not $running) {
                return
            }
            Start-Sleep -Seconds 1
        }
        Write-Host "Some listeners still running after timeout on port ${Port}."
    }
}

function Convert-ToPortList {
    param(
        [Parameter(Mandatory)]
        [string[]] $Values
    )

    $ports = [System.Collections.Generic.List[int]]::new()
    foreach ($value in $Values) {
        if ([string]::IsNullOrWhiteSpace($value)) {
            continue
        }

        $normalizedValue = $value.Trim().Trim("'", '"')
        $parts = ($normalizedValue -split "[,;\\s]+") | ForEach-Object { $_.Trim() } | Where-Object { $_ }
        foreach ($part in $parts) {
            $normalizedPart = ($part -replace "[^0-9]", "")
            if ([string]::IsNullOrWhiteSpace($normalizedPart)) {
                Write-Host "Ignoring invalid port value '$part' in KillPorts."
                continue
            }

            $portParsed = 0
            if (-not [int]::TryParse($normalizedPart, [ref]$portParsed)) {
                Write-Host "Ignoring invalid port value '$part' in KillPorts."
                continue
            }

            if ($portParsed -lt 1 -or $portParsed -gt 65535) {
                Write-Host "Ignoring out-of-range port '$part' in KillPorts."
                continue
            }
            $ports.Add($portParsed)
        }
    }

    $uniquePorts = $ports | Sort-Object -Unique
    if (-not $uniquePorts -or $uniquePorts.Count -eq 0) {
        Write-Host "No valid ports in KillPorts; using default fallback: 1420, 6969, 8488, 12917."
        return @(1420, 6969, 8488, 12917)
    }

    return $uniquePorts
}

function Resolve-BuildTool {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [string] $PreferredPath = ""
    )

    if ($PreferredPath) {
        if (Test-Path $PreferredPath) {
            return (Resolve-Path $PreferredPath).Path
        }
        Write-Host "Specified path for '$Name' not found: $PreferredPath"
    }

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    if ($Name -ieq "cargo") {
        $cargoHome = $env:CARGO_HOME
        $candidatePaths = @()
        if ($cargoHome) {
            $candidatePaths += Join-Path (Join-Path $cargoHome "bin") "cargo.exe"
        }
        $candidatePaths += Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
        $rustupToolchains = Join-Path $env:USERPROFILE ".rustup\\toolchains"
        if (Test-Path $rustupToolchains) {
            $candidatePaths += Get-ChildItem -Path $rustupToolchains -Directory -ErrorAction SilentlyContinue |
                Where-Object { Test-Path (Join-Path $_.FullName "bin\cargo.exe") } |
                Sort-Object -Property Name |
                ForEach-Object { Join-Path $_.FullName "bin\cargo.exe" }
        }

        foreach ($candidate in $candidatePaths) {
            if ($candidate -and (Test-Path $candidate)) {
                return (Resolve-Path $candidate).Path
            }
        }
    }

    if ($Name -ieq "bun") {
        $bunInstall = $env:BUN_INSTALL
        if ($bunInstall) {
            $candidate = Join-Path (Join-Path $bunInstall "bin") "bun.exe"
            if (Test-Path $candidate) {
                return (Resolve-Path $candidate).Path
            }
        }
    }

    Write-Host "Could not find '$Name' in PATH."
    return ""
}

$project = Resolve-Path -LiteralPath $RepoRoot -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Path
if (-not $project) {
    Write-Host "Invalid RepoRoot '$RepoRoot'. Falling back to script parent directory."
    $project = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}
Write-Host "Resolved repo root: $project"

if (Test-PortIsExcluded -Port $DaemonPort) {
    Write-Host "Warning: TCP port ${DaemonPort} is in an OS excluded range. Falling back to 8488."
    $DaemonPort = 8488
    if (Test-PortIsExcluded -Port $DaemonPort) {
        throw "Port 8488 is excluded on this machine. Pick another non-reserved port, e.g. -DaemonPort 8580."
    }
    $env:SD_SOCKET_ADDR = "127.0.0.1:${DaemonPort}"
}

$killPortsList = Convert-ToPortList -Values $KillPorts
Write-Host "Configured kill ports: $($killPortsList -join ', ')"

Write-Host "Stopping debug instances under: $project"

$candidates = Get-RepoProcessCandidates -ProjectPath $project
if (-not $candidates) {
    Write-Host "No matching debug/backend/UI processes found."
} else {
    $candidates | Select-Object ProcessId, Name, CommandLine | Format-Table -AutoSize | Out-String | Write-Host
    Stop-ProcessTree -ProcessIds $candidates.ProcessId
    Wait-ForStop -ProcessIds $candidates.ProcessId
}

Stop-ProcessByNameCandidates -ProcessNames $knownBinaryNames

Stop-ProcessOnPort -Port 1420
foreach ($port in $killPortsList) {
    if ($port -ne 1420) {
        Stop-ProcessOnPort -Port $port
    }
}

Set-Location $project

$cargoCmd = Resolve-BuildTool -Name "cargo" -PreferredPath $Cargo
$bunCmd = Resolve-BuildTool -Name "bun" -PreferredPath $Bun
if (-not $cargoCmd) {
    throw "Cargo not found. Re-run with -Cargo 'C:\\path\\to\\cargo.exe' or ensure Rust is in PATH."
}
if (-not $bunCmd) {
    throw "Bun not found. Re-run with -Bun 'C:\\path\\to\\bun.exe' or ensure Bun is in PATH."
}

$cargoDir = Split-Path $cargoCmd
$bunDir = Split-Path $bunCmd
if ($cargoDir -and ($env:PATH -notmatch [regex]::Escape($cargoDir))) {
    $env:PATH = "${cargoDir};$($env:PATH)"
}
if ($bunDir -and ($env:PATH -notmatch [regex]::Escape($bunDir))) {
    $env:PATH = "${bunDir};$($env:PATH)"
}

$cargoArgs = @(
    "build",
    "--manifest-path", (Join-Path $project "Cargo.toml"),
    "--bin", "sd-daemon"
)
if ($BuildProfile -ieq "Release") {
    $cargoArgs += "--release"
}

if (-not $SkipRebuild) {
    Write-Host "Rebuilding daemon ($BuildProfile)..."
    $buildLogPath = Join-Path $project "scripts\\restart-desktop-debug.build.log"
    $buildSucceeded = $true
    & $cargoCmd @cargoArgs 2>&1 | Tee-Object -FilePath $buildLogPath
    if ($LASTEXITCODE -ne 0) {
        $buildSucceeded = $false
    }
    if (-not $buildSucceeded) {
        $tail = if (Test-Path $buildLogPath) { Get-Content $buildLogPath -Tail 40 } else { @() }
        Write-Host "Build failed, showing last 40 lines from: $buildLogPath"
        $tail | ForEach-Object { Write-Host $_ }
        throw "cargo build --bin sd-daemon failed. profile=$BuildProfile"
    }
}

Write-Host "Starting Tauri desktop UI (native app, not webui)..."
$env:HOST = "127.0.0.1"
$env:SD_SOCKET_ADDR = "127.0.0.1:${DaemonPort}"
$tauriDir = Join-Path $project "apps\tauri"
Set-Location $tauriDir
Write-Host "Running: bun run tauri:dev"
& $bunCmd run tauri:dev
