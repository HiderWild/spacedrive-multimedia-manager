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

$launcher = Join-Path $PSScriptRoot "restart-desktop-debug.ps1"

Write-Host "Restarting Spacedrive debug in Tauri mode..."
$launcherArgs = @{
    RepoRoot    = $RepoRoot
    Cargo       = $Cargo
    Bun         = $Bun
    DaemonPort  = $DaemonPort
    BuildProfile = $BuildProfile
    SkipRebuild = $SkipRebuild
    KillPorts   = @($KillPorts)
}

& (Resolve-Path $launcher).Path @launcherArgs
