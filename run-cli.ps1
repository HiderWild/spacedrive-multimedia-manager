# Wrapper so release CLI works without silent DLL failures on Windows.
# Always prepends apps\.deps\bin (ffmpeg/heif/onnx DLLs) to PATH.
#
# Examples:
#   .\run-cli.ps1 start
#   .\run-cli.ps1 status
#   .\run-cli.ps1 library list
#   .\run-cli.ps1 stop

$ErrorActionPreference = "Stop"
$RepoRoot = $PSScriptRoot
$depsBin = Join-Path $RepoRoot "apps\.deps\bin"
if (Test-Path $depsBin) {
	$env:Path = $depsBin + [IO.Path]::PathSeparator + $env:Path
}

$cli = Join-Path $RepoRoot "target\release\sd-cli.exe"
if (-not (Test-Path $cli)) {
	Write-Error "Missing $cli - build release binaries first."
	exit 1
}

if ($args.Count -eq 0) {
	& $cli --help
	exit $LASTEXITCODE
}

& $cli @args
exit $LASTEXITCODE
