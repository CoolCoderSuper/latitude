[CmdletBinding()]
param(
    [switch]$Install
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$version = "1.8.2.4"
$archiveName = "UltraVNC_1824.zip"
$downloadUrl = "https://uvnc.eu/download/1800/UltraVNC_1824.zip"
$expectedSha256 = "8af948089626008f02edd1254afc15c814e454ec5fc9e3eaa860356f19d4f113"
$scriptRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
$architecture = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
$targetDir = Join-Path $scriptRoot "tools\ultravnc"
$winvncPath = Join-Path $targetDir "winvnc.exe"

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Invoke-WinVnc {
    param(
        [string[]]$Arguments,
        [switch]$AllowFailure
    )

    $argumentText = $Arguments -join " "
    Write-Host "Running: $winvncPath $argumentText"
    $process = Start-Process -FilePath $winvncPath -ArgumentList $Arguments -WorkingDirectory $targetDir -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0 -and -not $AllowFailure) {
        throw "UltraVNC command failed with exit code $($process.ExitCode): $winvncPath $argumentText"
    }
}

function Write-UltraVncConfig {
    $ini = @"
[admin]
UseRegistry=0
SocketConnect=1
primary=1
secondary=1
PortNumber=5900
AutoPortSelect=0
HTTPConnect=0
HTTPPortNumber=0
InputsEnabled=1
AllowLoopback=1
LoopbackOnly=1
AuthRequired=0
AuthHosts=+127.0.0.1:+::1:
QuerySetting=0
QueryAccept=1
QueryIfNoLogon=0
ConnectPriority=1
MaxViewerSetting=0
MaxViewers=128
IdleTimeout=0
IdleInputTimeout=0
KeepAliveInterval=5
LockSetting=0
AllowShutdown=0
AllowProperties=0
DisableTrayIcon=1
RemoveWallpaper=0

[poll]
PollFullScreen=1
PollForeground=1
PollUnderCursor=1
OnlyPollConsole=0
OnlyPollOnEvent=0
EnableHook=1
EnableDriver=0
EnableVirtual=0
TurboMode=1
"@

    New-Item -ItemType File -Path (Join-Path $targetDir "ultravnc.portable") -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $targetDir "ultravnc.ini") -Value $ini -Encoding ascii
}

if ($Install) {
    if ([Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
        throw "UltraVNC service installation is only supported on Windows."
    }

    if (-not (Test-IsAdministrator)) {
        throw "Run PowerShell as Administrator to install the UltraVNC service."
    }
}

$needsDownload = -not (Test-Path -LiteralPath $winvncPath)

if ($needsDownload) {
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

        $sourceDir = Join-Path $extractRoot $architecture
        $sourceWinvnc = Join-Path $sourceDir "winvnc.exe"
        if (-not (Test-Path -LiteralPath $sourceWinvnc)) {
            throw "The UltraVNC archive did not contain $architecture\winvnc.exe."
        }

        New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
        Get-ChildItem -LiteralPath $targetDir -Force |
            Where-Object { $_.Name -ne "README.md" } |
            Remove-Item -Recurse -Force

        Copy-Item -Path (Join-Path $sourceDir "*") -Destination $targetDir -Recurse -Force
        foreach ($readme in @("Readme.txt", "UltraVNC.ini_README")) {
            $sourceReadme = Join-Path $extractRoot $readme
            if (Test-Path -LiteralPath $sourceReadme) {
                Copy-Item -LiteralPath $sourceReadme -Destination $targetDir -Force
            }
        }

        New-Item -ItemType File -Path (Join-Path $targetDir "ultravnc.portable") -Force | Out-Null

        $metadata = @"
UltraVNC $version $architecture
Downloaded from: $downloadUrl
Archive SHA256: $expectedSha256
Installed by: init-ultravnc.ps1
"@
        Set-Content -LiteralPath (Join-Path $targetDir "ULTRAVNC_DOWNLOAD.txt") -Value $metadata -Encoding ascii

        Write-Host "UltraVNC portable bundle is ready at $targetDir"
    } finally {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
} else {
    New-Item -ItemType File -Path (Join-Path $targetDir "ultravnc.portable") -Force | Out-Null
    Write-Host "UltraVNC portable bundle is already present at $winvncPath."
}

if ($Install) {
    Write-UltraVncConfig
    Invoke-WinVnc -Arguments @("-stopservice") -AllowFailure
    Invoke-WinVnc -Arguments @("-install")
    Invoke-WinVnc -Arguments @("-startservice")
    Write-Host "UltraVNC service is installed and listening on 127.0.0.1:5900"
}
