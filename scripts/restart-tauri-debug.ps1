param(
    [string] $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string] $Cargo = "",
    [string] $Bun = "",
    [ValidateSet("Debug", "Release")]
    [string] $BuildProfile = "Debug",
    [switch] $SkipRebuild,
    [int[]] $KillPorts = @(1420, 6969, 12917)
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$launcher = Join-Path $PSScriptRoot "restart-desktop-debug.ps1"

Write-Host "Restarting Spacedrive debug in Tauri mode..."
& (Resolve-Path $launcher).Path `
    -RepoRoot $RepoRoot `
    -Cargo $Cargo `
    -Bun $Bun `
    -BuildProfile $BuildProfile `
    -SkipRebuild:$SkipRebuild `
    -KillPorts $KillPorts

