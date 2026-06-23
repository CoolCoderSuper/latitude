[CmdletBinding()]
param(
    [string]$Root,
    [string]$Config,
    [string]$PublicBind = "0.0.0.0:5597",
    [string]$CommandBind = "127.0.0.1:7600",
    [switch]$Foreground,
    [switch]$WaitForHealth
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function ConvertTo-CommandApiUrl {
    param([string]$Bind)

    if ($Bind -match "^https?://") {
        return $Bind.TrimEnd("/")
    }

    return "http://$Bind"
}

function ConvertTo-StartProcessArgument {
    param([string]$Value)

    if ($Value -match '[\s"]') {
        return '"' + ($Value -replace '"', '\"') + '"'
    }

    return $Value
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($Root)) {
    $Root = (Resolve-Path (Join-Path $scriptRoot "..")).Path
}
if ([string]::IsNullOrWhiteSpace($Config)) {
    $Config = Join-Path $Root "latitude.json"
}

$logs = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logs | Out-Null

$healthUri = "$(ConvertTo-CommandApiUrl $CommandBind)/health"
try {
    $health = Invoke-RestMethod -Uri $healthUri -TimeoutSec 2
    if ($health.status -eq "ok") {
        Write-Host "Latitude is already running at public bind $($health.public_bind)."
        exit 0
    }
} catch {
    # No healthy local command API is reachable yet.
}

$latitudeExe = @(
    (Join-Path $Root "target\release\latitude.exe"),
    (Join-Path $Root "target\debug\latitude.exe")
) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1

if (-not $latitudeExe) {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        throw "Latitude executable was not found under target\release or target\debug, and cargo is not on PATH."
    }

    & $cargo.Source build --manifest-path (Join-Path $Root "Cargo.toml")
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE."
    }

    $latitudeExe = Join-Path $Root "target\debug\latitude.exe"
}

if (-not (Test-Path -LiteralPath $Config)) {
    throw "Latitude config was not found at $Config."
}

$arguments = @(
    "--config", $Config,
    "--public-bind", $PublicBind,
    "--command-bind", $CommandBind
)

if ($Foreground) {
    & $latitudeExe @arguments
    exit $LASTEXITCODE
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$process = Start-Process `
    -FilePath $latitudeExe `
    -ArgumentList ($arguments | ForEach-Object { ConvertTo-StartProcessArgument $_ }) `
    -WorkingDirectory $Root `
    -RedirectStandardOutput (Join-Path $logs "latitude-$stamp.out.log") `
    -RedirectStandardError (Join-Path $logs "latitude-$stamp.err.log") `
    -WindowStyle Hidden `
    -PassThru

Write-Host "Started Latitude process $($process.Id) with public bind $PublicBind."

if ($WaitForHealth) {
    $deadline = (Get-Date).AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 500
        if ($process.HasExited) {
            throw "Latitude exited early with code $($process.ExitCode). Check logs in $logs."
        }

        try {
            $health = Invoke-RestMethod -Uri $healthUri -TimeoutSec 2
            if ($health.status -eq "ok") {
                Write-Host "Latitude health check passed at $healthUri."
                exit 0
            }
        } catch {
            # Keep waiting until the deadline.
        }
    } while ((Get-Date) -lt $deadline)

    throw "Latitude did not become healthy within 15 seconds. Check logs in $logs."
}
