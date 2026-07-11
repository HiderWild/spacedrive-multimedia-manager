# Compatibility wrapper — prefer ./start.ps1
# Dev (debug / hot reload) entry point.
#
# Unified entry: start.ps1 (default = release formal instance)
# This file only preserves the old "start-dev" name.

$script = Join-Path $PSScriptRoot "start.ps1"
if (-not (Test-Path $script)) {
	Write-Error "start.ps1 not found next to start-dev.ps1"
	exit 1
}

& $script -Dev @args
exit $LASTEXITCODE
