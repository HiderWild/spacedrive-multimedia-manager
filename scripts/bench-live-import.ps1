<#
.SYNOPSIS
  Operator checklist + timer wrapper for a live Spacedrive bulk import.

.DESCRIPTION
  Does not drive the UI or daemon automatically (indexing needs a running
  daemon + library). It:
    1) records start time
    2) prints the CLI/API steps to add a location / start index
    3) samples docker stats / process RSS until you press Enter
    4) writes a small JSON/text report

  Use this for the plan's "100/500/1000 against live daemon" criterion.

.EXAMPLE
  ./scripts/bench-live-import.ps1 -Label "100-jpeg-grid1x" -Container spacedrive-server
#>
[CmdletBinding()]
param(
    [string]$Label = "import-run",
    [string]$Container = "spacedrive-server",
    [string]$OutDir = "$PSScriptRoot/../.bench-import/live",
    [int]$SampleSeconds = 5
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$report = Join-Path $OutDir "${Label}-$stamp.txt"

function Write-Both([string]$msg) {
    Write-Host $msg
    Add-Content -Path $report -Value $msg
}

Write-Both "=== Spacedrive live import bench: $Label ==="
Write-Both ("Started : {0:o}" -f (Get-Date).ToUniversalTime())
Write-Both "Container: $Container"
Write-Both "Thumb knobs (from container env if running):"

$dockerOk = $false
try {
    $null = docker inspect $Container 2>$null
    if ($LASTEXITCODE -eq 0) { $dockerOk = $true }
} catch { $dockerOk = $false }

if ($dockerOk) {
    docker exec $Container sh -c 'echo SD_THUMB_MAX_CONCURRENT=$SD_THUMB_MAX_CONCURRENT; echo SD_THUMB_BATCH_SIZE=$SD_THUMB_BATCH_SIZE; echo FFMPEG_PATH=$FFMPEG_PATH' 2>$null |
        ForEach-Object { Write-Both $_ }
} else {
    Write-Both "(container not found; fill knobs manually from compose env)"
}

Write-Both ""
Write-Both "Manual steps (run in another terminal):"
Write-Both "  1. Ensure server is up (apps/server docker compose)."
Write-Both "  2. Mount a folder with a known image count (e.g. 100/500/1000 JPEGs)."
Write-Both "  3. Create/open library and add the location (UI or CLI)."
Write-Both "  4. Start indexing if it does not auto-start."
Write-Both "  5. Wait until jobs finish / UI is responsive."
Write-Both ""
Write-Both "Sampling docker stats every ${SampleSeconds}s. Press Enter when import is done..."

$start = Get-Date
$samples = New-Object System.Collections.Generic.List[string]
$samples.Add("time_utc,mem_usage,mem_perc,cpu_perc")

# Background-ish sampling loop on main thread with key check
while (-not [Console]::KeyAvailable) {
    $now = (Get-Date).ToUniversalTime().ToString("o")
    if ($dockerOk) {
        $line = docker stats $Container --no-stream --format "{{.MemUsage}},{{.MemPerc}},{{.CPUPerc}}" 2>$null
        if ($line) {
            $samples.Add("$now,$line")
            Write-Host ("  sample {0}  {1}" -f $now, $line)
        }
    } else {
        $ws = (Get-Process -Id $PID).WorkingSet64
        $samples.Add(("$now,{0:N0}B,-,-" -f $ws))
    }
    Start-Sleep -Seconds $SampleSeconds
}
# drain Enter
while ([Console]::KeyAvailable) { [void][Console]::ReadKey($true) }

$end = Get-Date
$dur = $end - $start

Write-Both ""
Write-Both ("Finished : {0:o}" -f $end.ToUniversalTime())
Write-Both ("Duration : {0:N1}s ({1})" -f $dur.TotalSeconds, $dur.ToString())
Write-Both "Samples  : $($samples.Count - 1)"
$csvPath = Join-Path $OutDir "${Label}-$stamp.csv"
$samples | Set-Content -Path $csvPath -Encoding utf8
Write-Both "CSV      : $csvPath"
Write-Both "Report   : $report"
Write-Both ""
Write-Both "Record for the plan checklist:"
Write-Both "  - image count / set name:"
Write-Both "  - duration (s): $([int]$dur.TotalSeconds)"
Write-Both "  - peak mem (from CSV max):"
Write-Both "  - froze? y/n:"
Write-Both "  - SD_THUMB_MAX_CONCURRENT:"
Write-Both "  - notes:"
