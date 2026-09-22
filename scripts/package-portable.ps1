<#
.SYNOPSIS
  Package AxMusic as a portable green zip (docs/技术架构.md section 7).

.DESCRIPTION
  Takes the built AxMusic.exe and produces:
    dist-portable/AxMusic-x.y.z-win64-portable.zip

  Zip layout:
    AxMusic.exe
    AxMusic-portable.ini
    data/            (empty placeholder)

  Run AFTER:
    npm install
    npm run build
    cargo tauri build   (from src-tauri, or: npm run tauri build)

.EXAMPLE
  ./scripts/package-portable.ps1
  ./scripts/package-portable.ps1 -ExePath src-tauri/target/release/axmusic.exe
#>
[CmdletBinding()]
param(
    [string]$ExePath = "",
    [string]$OutDir = "dist-portable",
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

function Resolve-Exe {
    param([string]$Hint)
    $candidates = @()
    if ($Hint) { $candidates += $Hint }
    $candidates += @(
        "src-tauri\target\release\axmusic.exe",
        "src-tauri\target\release\AxMusic.exe"
    )
    foreach ($c in $candidates) {
        $p = Join-Path $root $c
        if (Test-Path $p) {
            $item = Get-Item $p
            if ($item.Name -like "*setup*") { continue }
            return $item.FullName
        }
    }
    throw "AxMusic.exe not found. Build first: npm run tauri build"
}

function Get-AppVersion {
    if ($Version) { return $Version }
    $conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
    return $conf.version
}

$exe = Resolve-Exe -Hint $ExePath
$ver = Get-AppVersion
$stage = Join-Path $root $OutDir
$stageApp = Join-Path $stage "AxMusic"
$zipName = "AxMusic-$ver-win64-portable.zip"
$zipPath = Join-Path $stage $zipName

if (Test-Path $stageApp) { Remove-Item $stageApp -Recurse -Force }
New-Item -ItemType Directory -Path $stageApp -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $stageApp "data") -Force | Out-Null

Copy-Item $exe (Join-Path $stageApp "AxMusic.exe") -Force

# Force portable mode: keep data/ next to AxMusic.exe
$ini = @(
    "# AxMusic portable mode - keep data/ next to AxMusic.exe",
    "portable=1"
) -join "`r`n"
Set-Content -Path (Join-Path $stageApp "AxMusic-portable.ini") -Value $ini -Encoding ascii

# Keep empty data/ in the zip
Set-Content -Path (Join-Path $stageApp "data\.gitkeep") -Value "" -Encoding ascii

if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Compress-Archive -Path (Join-Path $stageApp "*") -DestinationPath $zipPath

Write-Host "OK  $zipPath"
Write-Host "    Unzip and run. Data lives in <unzip>/data. Delete folder to uninstall."

# Smoke check
$check = Get-ChildItem $stageApp
if (-not ($check.Name -contains "AxMusic.exe")) { throw "package missing AxMusic.exe" }
if (-not ($check.Name -contains "AxMusic-portable.ini")) { throw "package missing portable ini" }
Write-Host "OK  smoke checks passed"
