[CmdletBinding()]
param(
    [string]$OutputRoot = "",
    [switch]$Include100K,
    [switch]$IncludeSparse10GB
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $OutputRoot = Join-Path ([IO.Path]::GetTempPath()) "WindowsEasyBackup-Perf-$stamp"
}

$root = [IO.Path]::GetFullPath($OutputRoot)
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
if (-not $root.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "为避免误写用户目录，性能样本目录必须位于系统临时目录：$tempRoot"
}

New-Item -ItemType Directory -Path $root -Force | Out-Null

function New-FileSample {
    param([string]$Name, [int]$Count)

    $sampleRoot = Join-Path $root $Name
    New-Item -ItemType Directory -Path $sampleRoot -Force | Out-Null
    $started = Get-Date
    for ($index = 1; $index -le $Count; $index++) {
        $bucket = Join-Path $sampleRoot ("bucket-{0:D3}" -f (($index - 1) % 100))
        if (-not (Test-Path -LiteralPath $bucket)) {
            New-Item -ItemType Directory -Path $bucket | Out-Null
        }
        New-Item -ItemType File -Path (Join-Path $bucket ("asset-{0:D6}.dat" -f $index)) | Out-Null
    }
    $elapsed = ((Get-Date) - $started).TotalSeconds
    [PSCustomObject]@{ Sample = $Name; Files = $Count; Seconds = [Math]::Round($elapsed, 2); Path = $sampleRoot }
}

$results = @(
    (New-FileSample -Name "sample-10000" -Count 10000)
)
if ($Include100K) {
    $results += New-FileSample -Name "sample-100000" -Count 100000
}
if ($IncludeSparse10GB) {
    $sparsePath = Join-Path $root "sample-10GB.dat"
    $stream = [IO.File]::Create($sparsePath)
    try {
        $stream.SetLength(10GB)
    } finally {
        $stream.Dispose()
    }
    $results += [PSCustomObject]@{ Sample = "sample-10GB"; Files = 1; Seconds = 0; Path = $sparsePath }
}

$manifest = [PSCustomObject]@{
    createdAt = (Get-Date).ToUniversalTime().ToString("o")
    root = $root
    samples = $results
    purpose = "Use these local samples with the StackHome Files and Organizer pages. Record RAM, CPU, UI responsiveness and task-progress event rate separately."
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $root "manifest.json") -Encoding UTF8
$results | Format-Table -AutoSize
Write-Host "样本已准备：$root"
