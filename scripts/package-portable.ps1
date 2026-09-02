param(
  [string]$Version
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$packageJsonPath = Join-Path $projectRoot "package.json"
$sourceExe = Join-Path $projectRoot "src-tauri\target\release\stackhome.exe"
$releaseDir = Join-Path $projectRoot "release"
$productName = "StackHome"

if (-not $Version) {
  $packageJson = Get-Content -LiteralPath $packageJsonPath -Raw | ConvertFrom-Json
  $Version = $packageJson.version
}

$targetExe = Join-Path $releaseDir "$productName-Portable-Windows-x64-v$Version.exe"

if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "Portable source executable not found: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force

Write-Host "Portable build created:"
Write-Host $targetExe
