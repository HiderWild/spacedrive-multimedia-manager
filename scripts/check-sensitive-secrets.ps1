param(
    [string] $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [switch] $FailOnFindings,
    [switch] $ScanAllFiles
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Get-TrackedChangedFiles {
    param([string] $ProjectRoot, [switch] $ScanAllFiles)

    $files = New-Object 'System.Collections.Generic.List[string]'

    if (-not (Test-Path $ProjectRoot -PathType Container)) {
        throw "Invalid RepoRoot '$ProjectRoot'"
    }

    if ($ScanAllFiles) {
        $tracked = git -C $ProjectRoot ls-files 2>$null
        if ($LASTEXITCODE -eq 0) {
            foreach ($line in $tracked) {
                $trimmed = $line.Trim()
                if ($trimmed) {
                    [void]$files.Add($trimmed)
                }
            }
        }
    } else {
        # Compare to HEAD to capture both staged and unstaged tracked changes.
        $diffOutput = git -C $ProjectRoot diff --name-only HEAD -- 2>$null
        if ($LASTEXITCODE -eq 0) {
            foreach ($line in $diffOutput) {
                $trimmed = $line.Trim()
                if ($trimmed) {
                    [void]$files.Add($trimmed)
                }
            }
        }
    }

    # Newly added local files that are not tracked yet.
    $untracked = git -C $ProjectRoot ls-files --others --exclude-standard -- . 2>$null
    if ($LASTEXITCODE -eq 0) {
        foreach ($line in $untracked) {
            $trimmed = $line.Trim()
            if ($trimmed) {
                [void]$files.Add($trimmed)
            }
        }
    }

    return $files | Sort-Object -Unique
}

function Get-ScannableFileList {
    param([string] $ProjectRoot, [switch] $ScanAllFiles)

    $skipDirSegments = @(
        '.git',
        '.github',
        '.next',
        '.turbo',
        '.vite',
        '.cache',
        'node_modules',
        'target',
        'dist',
        'build',
        'out',
        'coverage',
        'tmp',
        'temp',
        'logs'
    )
    $skipExtensions = @(
        '.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.ico',
        '.mp4', '.mov', '.mkv', '.webm', '.mp3', '.wav', '.m4a',
        '.zip', '.tar', '.gz', '.7z', '.exe', '.dll', '.so', '.dmg', '.iso', '.bin',
        '.woff', '.woff2', '.ttf', '.otf', '.png', '.jpg', '.jpeg', '.gif',
        '.pdf', '.ico', '.jar', '.apk',
        '.sqlite', '.db'
    )

    $results = @()
    $candidateFiles = Get-TrackedChangedFiles -ProjectRoot $ProjectRoot -ScanAllFiles:$ScanAllFiles

    foreach ($relative in $candidateFiles) {
        if ([string]::IsNullOrWhiteSpace($relative)) {
            continue
        }

        $fullPath = Join-Path $ProjectRoot $relative
        if (-not (Test-Path $fullPath -PathType Leaf)) {
            continue
        }

        $normalized = $fullPath.ToLowerInvariant().Replace('/', '\')
        $skip = $false

        foreach ($segment in $skipDirSegments) {
            if ($normalized -match "\\$([regex]::Escape($segment))(\\|$)") {
                $skip = $true
                break
            }
        }
        if ($skip) {
            continue
        }

        $ext = [IO.Path]::GetExtension($fullPath).ToLowerInvariant()
        if ($skipExtensions -contains $ext) {
            continue
        }

        if ((Get-Item $fullPath).Length -gt 2MB) {
            continue
        }

        # Skip likely binary payloads containing NUL bytes.
        $stream = [System.IO.File]::OpenRead($fullPath)
        try {
            $buffer = New-Object byte[] 4096
            $read = $stream.Read($buffer, 0, $buffer.Length)
            $containsNull = $false
            for ($i = 0; $i -lt $read; $i++) {
                if ($buffer[$i] -eq 0) {
                    $containsNull = $true
                    break
                }
            }
            if ($containsNull) {
                continue
            }
        } finally {
            $stream.Dispose()
        }

        $results += [PSCustomObject]@{
            Relative = $relative
            FullPath = $fullPath
        }
    }

    return $results
}

function Get-SecretFindings {
    param([string] $FilePath)

    $rules = @(
        @{
            Name = "AWS access key"
            Severity = "High"
            Pattern = '(?i)\b(?:AKIA|A3T|ASIA|AGPA|AIDA|AROA)[0-9A-Z]{16}\b'
        },
        @{
            Name = "OpenSSH / RSA private key header"
            Severity = "High"
            Pattern = '(?i)-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----'
        },
        @{
            Name = "GitHub token"
            Severity = "High"
            Pattern = '(?i)\bgh[pousr]_[A-Za-z0-9]{36,}\b'
        },
        @{
            Name = "Long bearer/JWT token"
            Severity = "Medium"
            Pattern = '(?i)\bey[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b'
        },
        @{
            Name = "Credential-style env assignment"
            Severity = "High"
            Pattern = '^[ \t]*(?:export[ \t]+)?[A-Z][A-Z0-9_]*(?:SECRET|KEY|TOKEN|PASSWORD|CLIENT_SECRET|PRIVATE_KEY)\s*=\s*"[^"\r\n#]{16,}"\s*(?:#.*)?$'
        },
        @{
            Name = "DB / API credential URI"
            Severity = "Medium"
            Pattern = '(?i)\b(?:postgres|mongodb|mysql|redis|sqlserver)://[^:]+\:[^@\s]+@'
        }
    )

    $findings = New-Object System.Collections.Generic.List[object]
    $lines = @(Get-Content -Path $FilePath -Encoding UTF8 -ErrorAction Stop)

    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        foreach ($rule in $rules) {
            if ($line -match $rule.Pattern) {
                $matchText = $Matches[0]
                if ([string]::IsNullOrWhiteSpace($matchText)) {
                    continue
                }

                if ($rule.Name -eq "AWS access key" -and $matchText -match "EXAMPLE") {
                    continue
                }

                if ($rule.Name -eq "Credential-style env assignment" -and $matchText -match "\$\(") {
                    continue
                }

                if ($matchText.Length -gt 64) {
                    $matchText = $matchText.Substring(0, 28) + "..." + $matchText.Substring($matchText.Length - 8)
                }

                $findings.Add([PSCustomObject]@{
                        Line      = $i + 1
                        Rule      = $rule.Name
                        Severity  = $rule.Severity
                        Snippet   = $matchText
                        FullMatch = $Matches[0]
                    })
            }
        }
    }

    return $findings
}

$projectRoot = Resolve-Path -LiteralPath $RepoRoot -ErrorAction SilentlyContinue
if (-not $projectRoot) {
    throw "RepoRoot '$RepoRoot' does not exist."
}
$rootPath = $projectRoot.Path

$scopeLabel = if ($ScanAllFiles) { "all tracked + untracked files" } else { "changed and untracked files" }
Write-Host "Scanning $scopeLabel under: $rootPath"

$files = Get-ScannableFileList -ProjectRoot $rootPath -ScanAllFiles:$ScanAllFiles
if (-not $files -or $files.Count -eq 0) {
    Write-Host "No changed/untracked source files to scan."
    exit 0
}

$allFindings = New-Object System.Collections.Generic.List[object]
foreach ($item in $files) {
    try {
        $findings = Get-SecretFindings -FilePath $item.FullPath
        foreach ($itemFinding in $findings) {
            $allFindings.Add([PSCustomObject]@{
                    File     = $item.Relative
                    Line     = $itemFinding.Line
                    Severity = $itemFinding.Severity
                    Rule     = $itemFinding.Rule
                    Snippet  = $itemFinding.Snippet
                })
        }
    } catch {
        Write-Host "Unable to scan '$($item.Relative)': $($_.Exception.Message)"
    }
}

if (-not $allFindings -or $allFindings.Count -eq 0) {
    Write-Host "No potential sensitive data found."
    exit 0
}

Write-Host ""
Write-Host ("Found {0} potential sensitive pattern(s)." -f $allFindings.Count) -ForegroundColor Red
$allFindings | Sort-Object Severity, File, Line | Format-Table -AutoSize | Out-String | Write-Host
Write-Host ""
Write-Host "建议脱敏计划："
Write-Host "1) 将该类值从仓库文件移到环境变量或本地开发配置。"
Write-Host "2) 对历史提交进行清洗（git filter-repo/rebase）后，再旋转/失效凭证。"
Write-Host "3) 将密钥文件改为模板文件（xxx.example）并加入 .gitignore。"

if ($FailOnFindings) {
    exit 1
}

exit 0
