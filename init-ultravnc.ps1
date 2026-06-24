[CmdletBinding()]
param(
    [ValidateSet("x64", "x86")]
    [string]$Architecture = $(if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }),

    [string]$TargetDir,

    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$version = "1.8.2.4"
$archiveName = "UltraVNC_1824.zip"
$downloadUrl = "https://uvnc.eu/download/1800/UltraVNC_1824.zip"
$expectedSha256 = "8af948089626008f02edd1254afc15c814e454ec5fc9e3eaa860356f19d4f113"
$scriptRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }

if ([string]::IsNullOrWhiteSpace($TargetDir)) {
    $TargetDir = Join-Path $scriptRoot "tools\ultravnc"
}

function Resolve-FullPath {
    param([string]$Path)

    [System.IO.Path]::GetFullPath($Path)
}

$targetFullPath = Resolve-FullPath $TargetDir
$targetRoot = [System.IO.Path]::GetPathRoot($targetFullPath)
if ($targetFullPath.TrimEnd("\") -eq $targetRoot.TrimEnd("\")) {
    throw "Refusing to use drive root as UltraVNC target: $targetFullPath"
}

$winvncPath = Join-Path $targetFullPath "winvnc.exe"
if ((Test-Path -LiteralPath $winvncPath) -and -not $Force) {
    New-Item -ItemType File -Path (Join-Path $targetFullPath "ultravnc.portable") -Force | Out-Null
    Write-Host "UltraVNC is already present at $winvncPath. Use -Force to refresh it."
    exit 0
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "latitude-ultravnc-$([System.Guid]::NewGuid())"
$archivePath = Join-Path $tempRoot $archiveName
$extractRoot = Join-Path $tempRoot "extract"

try {
    New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null

    Write-Host "Downloading UltraVNC $version from $downloadUrl"
    Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath -UserAgent "Latitude init-ultravnc"

    $actualSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    if ($actualSha256 -ne $expectedSha256) {
        throw "UltraVNC archive hash mismatch. Expected $expectedSha256 but got $actualSha256."
    }

    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractRoot -Force

    $sourceDir = Join-Path $extractRoot $Architecture
    $sourceWinvnc = Join-Path $sourceDir "winvnc.exe"
    if (-not (Test-Path -LiteralPath $sourceWinvnc)) {
        throw "The UltraVNC archive did not contain $Architecture\winvnc.exe."
    }

    New-Item -ItemType Directory -Path $targetFullPath -Force | Out-Null
    Get-ChildItem -LiteralPath $targetFullPath -Force |
        Where-Object { $_.Name -ne "README.md" } |
        Remove-Item -Recurse -Force

    Copy-Item -Path (Join-Path $sourceDir "*") -Destination $targetFullPath -Recurse -Force
    foreach ($readme in @("Readme.txt", "UltraVNC.ini_README")) {
        $sourceReadme = Join-Path $extractRoot $readme
        if (Test-Path -LiteralPath $sourceReadme) {
            Copy-Item -LiteralPath $sourceReadme -Destination $targetFullPath -Force
        }
    }

    New-Item -ItemType File -Path (Join-Path $targetFullPath "ultravnc.portable") -Force | Out-Null

    $metadata = @"
UltraVNC $version $Architecture
Downloaded from: $downloadUrl
Archive SHA256: $expectedSha256
Installed by: init-ultravnc.ps1
"@
    Set-Content -LiteralPath (Join-Path $targetFullPath "ULTRAVNC_DOWNLOAD.txt") -Value $metadata -Encoding ascii

    Write-Host "UltraVNC $version ($Architecture) is ready at $targetFullPath"
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
