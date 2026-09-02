param(
  [string]$Version
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$packageJsonPath = Join-Path $projectRoot "package.json"
$sourceExe = Join-Path $projectRoot "src-tauri\target\release\guizhan.exe"
$releaseDir = Join-Path $projectRoot "release"
$productName = -join [char[]](0x5F52, 0x6808)
$portableLabel = -join [char[]](0x4FBF, 0x643A, 0x7248)

if (-not $Version) {
  $packageJson = Get-Content -LiteralPath $packageJsonPath -Raw | ConvertFrom-Json
  $Version = $packageJson.version
}

$targetExe = Join-Path $releaseDir "$productName - $portableLabel (Win10-11) v$Version.exe"

if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "Portable source executable not found: $sourceExe"
}

New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force

Write-Host "Portable build created:"
Write-Host $targetExe
