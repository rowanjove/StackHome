param(
  [string]$Version
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$packageJsonPath = Join-Path $projectRoot "package.json"
$releaseDir = Join-Path $projectRoot "release"
$productName = "StackHome"
$sourceExe = Join-Path $projectRoot "src-tauri\target\release\bundle\nsis\${productName}_${Version}_x64-setup.exe"

if (-not $Version) {
  $packageJson = Get-Content -LiteralPath $packageJsonPath -Raw | ConvertFrom-Json
  $Version = $packageJson.version
  $sourceExe = Join-Path $projectRoot "src-tauri\target\release\bundle\nsis\${productName}_${Version}_x64-setup.exe"
}

$targetExe = Join-Path $releaseDir "$productName-Setup-Windows-x64-v$Version.exe"

if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "Installer source executable not found: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force

Write-Host "Installer build copied:"
Write-Host $targetExe
