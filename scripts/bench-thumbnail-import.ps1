<#
.SYNOPSIS
  Lightweight before/after bench for bulk photo import friendliness.

.DESCRIPTION
  Generates N JPEG test images (if needed), runs an offline decode+resize loop
  similar to thumbnail work, and reports wall time + peak working set for this
  process. Use it to validate SD_THUMB_* style throttling assumptions without a
  full Spacedrive library.

  This does NOT start the daemon; it measures CPU decode pressure only.

.EXAMPLE
  ./scripts/bench-thumbnail-import.ps1 -Count 100
  ./scripts/bench-thumbnail-import.ps1 -Count 500 -Concurrency 2 -Size 256
#>
[CmdletBinding()]
param(
    [int]$Count = 100,
    [int]$Concurrency = 2,
    [int]$Size = 256,
    [string]$WorkDir = "",
    [switch]$KeepImages
)

$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($WorkDir)) {
    $root = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
    if (-not $root) { $root = (Get-Location).Path }
    $WorkDir = Join-Path $root "..\.bench-import"
}
$WorkDir = [IO.Path]::GetFullPath($WorkDir)
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
$imgDir = Join-Path $WorkDir "images"
New-Item -ItemType Directory -Force -Path $imgDir | Out-Null

function New-TestJpeg {
    param([string]$Path, [int]$W = 2000, [int]$H = 1500)
    Add-Type -AssemblyName System.Drawing
    $bmp = New-Object System.Drawing.Bitmap $W, $H
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear([System.Drawing.Color]::FromArgb(30, 90, 160))
    $brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::White)
    $font = New-Object System.Drawing.Font "Arial", 48
    $g.DrawString([IO.Path]::GetFileName($Path), $font, $brush, 40, 40)
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Jpeg)
    $g.Dispose(); $bmp.Dispose(); $brush.Dispose(); $font.Dispose()
}

function Process-OneImage {
    param([string]$Path, [int]$TargetSize)
    $img = [System.Drawing.Image]::FromFile($Path)
    try {
        $ratio = [Math]::Min($TargetSize / $img.Width, $TargetSize / $img.Height)
        $nw = [Math]::Max(1, [int]($img.Width * $ratio))
        $nh = [Math]::Max(1, [int]($img.Height * $ratio))
        $thumb = New-Object System.Drawing.Bitmap $nw, $nh
        $g = [System.Drawing.Graphics]::FromImage($thumb)
        try {
            $g.DrawImage($img, 0, 0, $nw, $nh)
        } finally {
            $g.Dispose()
            $thumb.Dispose()
        }
    } finally {
        $img.Dispose()
    }
}

Write-Host "Preparing $Count test JPEGs in $imgDir ..."
$files = New-Object System.Collections.Generic.List[string]
for ($i = 0; $i -lt $Count; $i++) {
    $f = Join-Path $imgDir ("img_{0:D5}.jpg" -f $i)
    if (-not (Test-Path $f)) {
        New-TestJpeg -Path $f
    }
    $files.Add($f) | Out-Null
}

Write-Host "Benchmark: count=$Count concurrency=$Concurrency target=$Size"
Add-Type -AssemblyName System.Drawing

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$peak = 0L
$errorCount = [ref]0
$conc = [Math]::Max(1, $Concurrency)

# Thread-safe bag of paths
$bag = [System.Collections.Concurrent.ConcurrentBag[string]]::new()
foreach ($f in $files) { $bag.Add($f) | Out-Null }

$runspacePool = [runspacefactory]::CreateRunspacePool(1, $conc)
$runspacePool.Open()
$workers = @()

$script = {
    param($Bag, $TargetSize, $ErrorRef)
    Add-Type -AssemblyName System.Drawing
    $localErr = 0
    $item = $null
    while ($Bag.TryTake([ref]$item)) {
        try {
            $img = [System.Drawing.Image]::FromFile($item)
            try {
                $ratio = [Math]::Min($TargetSize / $img.Width, $TargetSize / $img.Height)
                $nw = [Math]::Max(1, [int]($img.Width * $ratio))
                $nh = [Math]::Max(1, [int]($img.Height * $ratio))
                $thumb = New-Object System.Drawing.Bitmap $nw, $nh
                $g = [System.Drawing.Graphics]::FromImage($thumb)
                $g.DrawImage($img, 0, 0, $nw, $nh)
                $g.Dispose(); $thumb.Dispose()
            } finally {
                $img.Dispose()
            }
        } catch {
            $localErr++
        }
    }
    return $localErr
}

for ($w = 0; $w -lt $conc; $w++) {
    $ps = [powershell]::Create()
    $ps.RunspacePool = $runspacePool
    [void]$ps.AddScript($script).AddArgument($bag).AddArgument($Size).AddArgument($errorCount)
    $workers += [pscustomobject]@{
        PowerShell = $ps
        Handle     = $ps.BeginInvoke()
    }
}

$proc = Get-Process -Id $PID
while ($true) {
    $done = $true
    foreach ($worker in $workers) {
        if (-not $worker.Handle.IsCompleted) { $done = $false; break }
    }
    $ws = $proc.WorkingSet64
    if ($ws -gt $peak) { $peak = $ws }
    if ($done) { break }
    Start-Sleep -Milliseconds 100
}

foreach ($worker in $workers) {
    try {
        $result = $worker.PowerShell.EndInvoke($worker.Handle)
        if ($result) { $errorCount.Value += [int]$result[0] }
    } catch {
        $errorCount.Value++
    } finally {
        $worker.PowerShell.Dispose()
    }
}
$runspacePool.Close()
$runspacePool.Dispose()

$ws = (Get-Process -Id $PID).WorkingSet64
if ($ws -gt $peak) { $peak = $ws }
$sw.Stop()

$sec = [Math]::Max(0.001, $sw.Elapsed.TotalSeconds)
$rate = $Count / $sec
Write-Host ""
Write-Host "=== Results ==="
Write-Host ("Duration      : {0:N2}s" -f $sec)
Write-Host ("Throughput    : {0:N1} images/s" -f $rate)
Write-Host ("Peak WS (this): {0:N0} MB" -f ($peak / 1MB))
Write-Host ("Errors        : {0}" -f $errorCount.Value)
Write-Host ("Concurrency   : $Concurrency")
Write-Host ""
Write-Host "Tips:"
Write-Host "  - Compare Concurrency=1 vs 4 vs 8 on the same machine"
Write-Host "  - Docker: set SD_THUMB_MAX_CONCURRENT to match a non-freezing value"
Write-Host "  - Real import also does hashing + DB; this is decode/resize only"

if (-not $KeepImages) {
    # Reuse generated set across runs by default.
}
