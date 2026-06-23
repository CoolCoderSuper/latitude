$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

$javaHomeCandidates = @(@(
  $env:JAVA_HOME,
  "C:\Progra~1\Java\jdk-19",
  "C:\Program Files\Java\jdk-19",
  "C:\Program Files\Java\latest",
  "C:\Program Files\Microsoft\jdk-17.0.15.6-hotspot",
  "C:\Program Files\Eclipse Adoptium\jdk-17"
) | Where-Object { $_ -and (Test-Path (Join-Path $_ "bin\java.exe")) })

if (-not $javaHomeCandidates) {
  throw "No suitable JDK found. Install JDK 17+ or set JAVA_HOME."
}

$env:JAVA_HOME = $javaHomeCandidates[0]
$androidSdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA "Android\Sdk" }

if (-not (Test-Path $androidSdk)) {
  throw "Android SDK not found at '$androidSdk'. Install Android Studio or set ANDROID_HOME."
}

$env:ANDROID_HOME = $androidSdk
$env:ANDROID_SDK_ROOT = $androidSdk
$env:NODE_ENV = if ($env:NODE_ENV) { $env:NODE_ENV } else { "development" }
$env:Path = "$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;$env:Path"

if (-not (Test-Path (Join-Path $ProjectRoot "android\gradlew.bat"))) {
  & npx expo prebuild --platform android --no-install
  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }
}

Push-Location (Join-Path $ProjectRoot "android")
try {
  & .\gradlew.bat assembleRelease
  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }
} finally {
  Pop-Location
}

$apkPath = Join-Path $ProjectRoot "android\app\build\outputs\apk\release\app-release.apk"
if (-not (Test-Path $apkPath)) {
  throw "Gradle finished, but APK was not found at '$apkPath'."
}

Write-Host "APK built: $apkPath"
