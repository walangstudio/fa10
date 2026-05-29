#Requires -Version 5.1
# fa10 installer for Windows. Downloads the prebuilt binary from GitHub
# Releases, verifies its SHA-256, installs it, and adds it to your user PATH.
# Re-running upgrades in place.
#   irm https://raw.githubusercontent.com/walangstudio/fa10/main/install.ps1 | iex
#   & ([scriptblock]::Create((irm .../install.ps1))) -Version v0.3.0
#   & ([scriptblock]::Create((irm .../install.ps1))) -Uninstall
[CmdletBinding()]
param(
  [switch]$Uninstall,
  [switch]$PreRelease,
  [string]$Version = ''
)

$ErrorActionPreference = 'Stop'

$Repo   = 'walangstudio/fa10'
$Binary = 'fa10.exe'

function Write-Info    { Write-Host "==> $args" -ForegroundColor Cyan }
function Write-Success { Write-Host "ok $args" -ForegroundColor Green }
function Write-Warn    { Write-Host "warning: $args" -ForegroundColor Yellow }
function Write-Fatal   { Write-Host "error: $args" -ForegroundColor Red; exit 1 }

function Get-Target {
  switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { return 'x86_64-pc-windows-msvc' }
    'ARM64' { Write-Fatal "No Windows arm64 build is published. Build from source: cargo install --git https://github.com/$Repo" }
    default { Write-Fatal "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
  }
}

function Get-InstallDir { return "$env:LOCALAPPDATA\Programs\fa10" }

function Get-TargetVersion {
  if ($Version) {
    $tag = $Version.Trim(); if (-not $tag.StartsWith('v')) { $tag = "v$tag" }
    try { $r = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/tags/$tag" -ErrorAction Stop }
    catch { Write-Fatal "Version $tag not found" }
    if (-not $r.tag_name) { Write-Fatal "Version $tag not found" }
    return $r.tag_name
  }
  if ($PreRelease) {
    $r = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases") | Where-Object { $_.prerelease } | Select-Object -First 1
    if (-not $r) { Write-Fatal "No pre-release found" }
    return $r.tag_name
  }
  return (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest").tag_name
}

function Get-InstalledVersion {
  $cmd = Get-Command fa10 -ErrorAction SilentlyContinue
  if (-not $cmd) {
    $known = Join-Path (Get-InstallDir) $Binary
    if (Test-Path $known) { $cmd = $known } else { return $null }
  }
  try {
    $bin = if ($cmd -is [string]) { $cmd } else { $cmd.Source }
    $out = & $bin --version 2>&1
    if ($out -match '(\d+\.\d+\.\d+)') { return "v$($Matches[1])" }
  } catch {}
  return $null
}

function Add-ToUserPath {
  param([string]$Dir)
  $current = [Environment]::GetEnvironmentVariable('Path', 'User')
  if ($current -split ';' -contains $Dir) { return }
  [Environment]::SetEnvironmentVariable('Path', "$current;$Dir", 'User')
  $env:PATH = "$env:PATH;$Dir"
  Write-Warn "$Dir added to your user PATH (restart the terminal to take effect)"
}

function Confirm-Checksum {
  param([string]$Archive, [string]$SumsFile)
  $name = Split-Path $Archive -Leaf
  $entry = Get-Content $SumsFile | Where-Object { $_ -match "\s$([regex]::Escape($name))$" } | Select-Object -First 1
  if (-not $entry) { Write-Warn "No checksum entry for $name, skipping verification"; return }
  $expected = ($entry -split '\s+')[0].ToLower()
  $actual   = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLower()
  if ($actual -ne $expected) { Write-Fatal "Checksum mismatch!`n  expected: $expected`n  got:      $actual" }
  Write-Success "Checksum verified"
}

function Invoke-Uninstall {
  $cmd = Get-Command fa10 -ErrorAction SilentlyContinue
  $known = Join-Path (Get-InstallDir) $Binary
  if (-not $cmd -and -not (Test-Path $known)) { Write-Warn "fa10 is not installed"; exit 0 }
  $path = if ($cmd) { $cmd.Source } else { $known }
  Write-Info "Removing $path..."
  Remove-Item $path -Force
  $dir = Split-Path $path
  if ((Get-ChildItem $dir -ErrorAction SilentlyContinue).Count -eq 0) { Remove-Item $dir -Force -ErrorAction SilentlyContinue }
  Write-Success "fa10 uninstalled"
}

function Main {
  if ($Uninstall) { Invoke-Uninstall; return }
  if ($PreRelease -and $Version) { Write-Fatal "-PreRelease and -Version cannot be combined" }

  $target = Get-Target

  Write-Info "Fetching release info..."
  $version = Get-TargetVersion
  if (-not $version) { Write-Fatal "Could not determine target version" }

  $installed = Get-InstalledVersion
  if ($installed) {
    $cmd = Get-Command fa10 -ErrorAction SilentlyContinue
    $installedPath = if ($cmd) { $cmd.Source } else { Join-Path (Get-InstallDir) $Binary }
    if ($installed -eq $version) {
      if (-not $PreRelease -and -not $Version) {
        Write-Success "fa10 $version is already installed at $installedPath - nothing to do"; exit 0
      }
      Write-Warn "fa10 $version is already installed at $installedPath; reinstalling."
    } else {
      Write-Info "Updating fa10 $installed -> $version  (at $installedPath)"
    }
  } else {
    Write-Info "Installing fa10 $version"
  }

  $asset   = "fa10-$version-$target.zip"
  $baseUrl = "https://github.com/$Repo/releases/download/$version"
  $tmpDir  = (New-Item -ItemType Directory -Path ([System.IO.Path]::GetTempPath() + [System.IO.Path]::GetRandomFileName()) -Force).FullName
  $archive = Join-Path $tmpDir $asset
  $sums    = Join-Path $tmpDir 'SHA256SUMS'

  try {
    Write-Info "Downloading $asset..."
    Invoke-WebRequest "$baseUrl/$asset"      -OutFile $archive -UseBasicParsing
    Invoke-WebRequest "$baseUrl/SHA256SUMS"  -OutFile $sums    -UseBasicParsing

    Write-Info "Verifying checksum..."
    Confirm-Checksum $archive $sums

    Write-Info "Extracting..."
    Expand-Archive $archive -DestinationPath $tmpDir -Force
    $extracted = Join-Path $tmpDir $Binary
    if (-not (Test-Path $extracted)) { Write-Fatal "Binary '$Binary' not found in archive" }

    $installDir = Get-InstallDir
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    $dest = Join-Path $installDir $Binary
    $backup = "$dest.old"; $hasBackup = $false

    if (Test-Path $dest) {
      Remove-Item $backup -Force -ErrorAction SilentlyContinue
      try { Rename-Item $dest $backup -Force; $hasBackup = $true }
      catch { Write-Fatal "Cannot replace existing binary - is fa10 running? Close it and retry." }
    }
    try {
      Copy-Item $extracted $dest -Force
      if ($hasBackup) { Remove-Item $backup -Force -ErrorAction SilentlyContinue }
    } catch {
      if ($hasBackup) { Write-Warn "Install failed, restoring previous version..."; Rename-Item $backup $dest -Force -ErrorAction SilentlyContinue }
      Write-Fatal "Installation failed: $_"
    }

    Add-ToUserPath $installDir

    if ($installed -and $installed -ne $version) { Write-Success "fa10 updated $installed -> $version" }
    else { Write-Success "fa10 $version installed successfully" }
    Write-Host ""
    & $dest --version
  } finally {
    Remove-Item $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
  }
}

Main
