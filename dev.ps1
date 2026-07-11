# Compatibility wrapper — prefer ./start.ps1
# Quick dev startup (debug / hot reload).

$script = Join-Path $PSScriptRoot "start.ps1"
if (-not (Test-Path $script)) {
	Write-Error "start.ps1 not found next to dev.ps1"
	exit 1
}

& $script -Dev @args
exit $LASTEXITCODE
