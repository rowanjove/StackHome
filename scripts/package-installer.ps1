param(
  [string]$Version
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$packageJsonPath = Join-Path $projectRoot "package.json"
$releaseDir = Join-Path $projectRoot "release"
$productName = -join [char[]](0x5F52, 0x6808)
$installerLabel = -join [char[]](0x5B89, 0x88C5, 0x7248)
$compatibilityLabel = -join [char[]](0x517C, 0x5BB9)
$sourceExe = Join-Path $projectRoot "src-tauri\target\release\bundle\nsis\${productName}_${Version}_x64-setup.exe"

if (-not $Version) {
  $packageJson = Get-Content -LiteralPath $packageJsonPath -Raw | ConvertFrom-Json
  $Version = $packageJson.version
  $sourceExe = Join-Path $projectRoot "src-tauri\target\release\bundle\nsis\${productName}_${Version}_x64-setup.exe"
}

$targetExe = Join-Path $releaseDir "$productName - $installerLabel ($compatibilityLabel) v$Version.exe"

if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "Installer source executable not found: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force

Write-Host "Installer build copied:"
Write-Host $targetExe
