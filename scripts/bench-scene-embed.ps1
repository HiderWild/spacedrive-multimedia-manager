#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Horizontal evaluation of scene-embedding backends (quality vs perf).

.DESCRIPTION
  Compares OpenCLIP ViT-B/32, DINOv2 ViT-B/14, and histogram baseline
  on a user-provided image directory. Reports per-backend:
  - mean / p95 latency
  - execution device (CUDA / CPU / baseline)
  - cluster count (cosine DBSCAN)
  - nearest-neighbor label accuracy (when labels.csv provided)

  The benchmark invokes sd-cli through scripts/invoke-spacedrive-cargo.ps1,
  which selects the main worktree target and serializes compile-producing work.

.PARAMETER Images
  Directory of images to embed (jpg/png/heic).

.PARAMETER Labels
  Optional CSV: filename,label_id  (for NN accuracy metric)

.PARAMETER DataDir
  Spacedrive data dir (models live under {DataDir}/models/image_embedding/)

.PARAMETER Output
  Output JSON report path (default: ./scene-embed-eval-report.json)

.PARAMETER Backend
  Comma-separated backend ids to evaluate (default: all).
  Options: openclip-vit-b-32, dinov2-vit-b-14, histogram-baseline

.EXAMPLE
  ./bench-scene-embed.ps1 -Images ./test-photos -DataDir ~/.spacedrive
#>

param(
    [Parameter(Mandatory)][string]$Images,
    [string]$Labels,
    [string]$DataDir = "$env:USERPROFILE/.spacedrive",
    [string]$Output = "./scene-embed-eval-report.json",
    [string]$Backend = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoWrapperPath = Join-Path $PSScriptRoot 'invoke-spacedrive-cargo.ps1'

Write-Host "=== Scene Embedding Backend Evaluation ===" -ForegroundColor Cyan
Write-Host "Images:   $Images"
Write-Host "DataDir:  $DataDir"
Write-Host "Output:   $Output"
Write-Host ""

# Check model weights
$modelsDir = Join-Path $DataDir "models/image_embedding"
Write-Host "Model weights dir: $modelsDir"
if (Test-Path $modelsDir) {
    Get-ChildItem $modelsDir -Filter *.onnx | ForEach-Object {
        $sizeMB = [math]::Round($_.Length / 1MB, 1)
        Write-Host "  Found: $($_.Name) ($sizeMB MB)" -ForegroundColor Green
    }
} else {
    Write-Host "  (directory does not exist yet)" -ForegroundColor Yellow
}
Write-Host ""

# List image files
$imageFiles = @()
foreach ($ext in @("*.jpg", "*.jpeg", "*.png", "*.heic", "*.heif", "*.webp")) {
    $imageFiles += Get-ChildItem $Images -Filter $ext -Recurse -File -ErrorAction SilentlyContinue
}

if ($imageFiles.Count -eq 0) {
    Write-Host "No images found in $Images" -ForegroundColor Red
    exit 1
}

Write-Host "Found $($imageFiles.Count) images"
Write-Host ""

# Build the eval command
$backends = if ($Backend) { $Backend } else { "openclip-vit-b-32,dinov2-vit-b-14,histogram-baseline" }

Write-Host "Backends to evaluate: $backends"
Write-Host ""

# Run via sd-cli (when scene-embed feature compiled in)
$cliArgs = @(
    "scene-embed-eval",
    "--images", ($imageFiles | ForEach-Object { $_.FullName }) -join ";",
    "--data-dir", $DataDir,
    "--backends", $backends,
    "--output", $Output
)

if ($Labels -and (Test-Path $Labels)) {
    $cliArgs += @("--labels", $Labels)
}

Write-Host "Running: sd-cli $($cliArgs -join ' ')"
Write-Host ""

# Try to run sd-cli through the shared build policy wrapper.
$cargoArgs = @(
    "run",
    "--bin", "sd-cli",
    "--features", "scene-embed",
    "--"
) + @($cliArgs)
try {
    & $cargoWrapperPath -RepoRoot $repoRoot @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "sd-cli scene-embed-eval failed with exit code $LASTEXITCODE"
    }
} catch {
    Write-Host ""
    Write-Host "sd-cli scene-embed-eval not available yet." -ForegroundColor Yellow
    Write-Host "The eval harness is implemented in core::ops::media::scene_embed::eval." -ForegroundColor Yellow
    Write-Host "Wire it to a CLI subcommand to run this benchmark." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Evaluation complete ===" -ForegroundColor Cyan
Write-Host "Report: $Output"
