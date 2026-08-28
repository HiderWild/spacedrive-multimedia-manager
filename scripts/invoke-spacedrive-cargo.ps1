Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$CargoPath = 'cargo'
$GitPath = 'git'
$PolicyEventLogPath = $null
$CargoArguments = New-Object System.Collections.ArrayList

$argumentIndex = 0
while ($argumentIndex -lt $args.Count) {
	$wrapperOption = $args[$argumentIndex]
	if (@('-RepoRoot', '-CargoPath', '-GitPath', '-PolicyEventLogPath') -notcontains $wrapperOption) {
		while ($argumentIndex -lt $args.Count) {
			[void]$CargoArguments.Add($args[$argumentIndex])
			$argumentIndex++
		}
		break
	}

	if (($argumentIndex + 1) -ge $args.Count) {
		Write-Error "Missing value for wrapper option: $wrapperOption"
		exit 1
	}
	$optionValue = $args[$argumentIndex + 1]
	switch ($wrapperOption) {
		'-RepoRoot' { $RepoRoot = $optionValue }
		'-CargoPath' { $CargoPath = $optionValue }
		'-GitPath' { $GitPath = $optionValue }
		'-PolicyEventLogPath' { $PolicyEventLogPath = $optionValue }
	}
	$argumentIndex += 2
}

. (Join-Path $PSScriptRoot 'build-policy.ps1')

try {
	$exitCode = Invoke-SpacedriveCargo `
		-RepoRoot $RepoRoot `
		-CargoPath $CargoPath `
		-GitPath $GitPath `
		-EventLogPath $PolicyEventLogPath `
		-CargoArguments @($CargoArguments)
	exit $exitCode
} catch {
	Write-Error $_
	exit 1
}
