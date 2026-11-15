#requires -Version 5.1
<#
.SYNOPSIS
Create a new, read-only Forge cache-preflight configuration from Control's
reviewed evidence directories. Does not run Java or modify Minecraft data.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory=$true)][string[]]$MavenRoots,
  [Parameter(Mandatory=$true)][string]$ClientJar,
  [Parameter(Mandatory=$true)][string]$BinpatchDirectory,
  [Parameter(Mandatory=$true)][string]$Java8,
  [Parameter(Mandatory=$true)][string]$StageRoot,
  [Parameter(Mandatory=$true)][string]$ConfigFile
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$roots = @($MavenRoots | ForEach-Object { [IO.Path]::GetFullPath($_) })
foreach ($root in $roots) { if (-not (Test-Path -LiteralPath $root -PathType Container)) { throw "Missing Maven cache root: $root" } }
$client = [IO.Path]::GetFullPath($ClientJar)
$patchRoot = [IO.Path]::GetFullPath($BinpatchDirectory)
$staging = [IO.Path]::GetFullPath($StageRoot)
$config = [IO.Path]::GetFullPath($ConfigFile)
if (-not (Test-Path -LiteralPath $client -PathType Leaf)) { throw 'Missing Mojang client JAR' }
if (-not (Test-Path -LiteralPath $Java8 -PathType Leaf)) { throw 'Missing Java 8 executable' }
if (-not (Test-Path -LiteralPath (Join-Path $patchRoot 'client.lzma') -PathType Leaf)) { throw 'Missing BINPATCH' }
if (-not (Test-Path -LiteralPath (Join-Path $patchRoot 'binpatch-provenance.json') -PathType Leaf)) { throw 'Missing BINPATCH report' }
if (Test-Path -LiteralPath $staging) { throw 'Stage root must be fresh and absent' }
if (Test-Path -LiteralPath $config) { throw 'Config destination exists (no overwrite)' }
$parts = @($staging -split '[\\/]')
if (@($parts | Where-Object { $_ -match '^(?i:saves|worlds|profiles|instances|\.minecraft)$' }).Count -gt 0) { throw 'Stage within Minecraft data is forbidden' }
$attestation = Get-Content (Join-Path $patchRoot 'binpatch-provenance.json') -Raw | ConvertFrom-Json
$expectedInstaller = '7d247279cf06eed6ffcf609f36d11e34c70350f3'
if ($attestation.installer_sha1 -ne $expectedInstaller) { throw 'BINPATCH installer SHA-1 attestation mismatch' }
$sha = '37fd3c903861eeff3bc24b71eed48f828b5269c8'
$size = 17547153
if ((Get-FileHash -LiteralPath $client -Algorithm SHA1).Hash.ToLowerInvariant() -ne $sha -or (Get-Item -LiteralPath $client).Length -ne $size) { throw 'Mojang client SHA-1/size mismatch' }
$report = [ordered]@{
  cache_roots = $roots
  client = [ordered]@{
    path = $client
    url = "https://launcher.mojang.com/v1/objects/$sha/client.jar"
    sha1 = $sha
    size_bytes = $size
  }
  binpatch = [ordered]@{
    installer = [string]$attestation.installer
    installer_sha1 = [string]$attestation.installer_sha1
    entry = [string]$attestation.entry
    extracted = [string]$attestation.extracted
    extracted_sha1 = [string]$attestation.extracted_sha1
    report = (Join-Path $patchRoot 'binpatch-provenance.json')
  }
  java8 = [IO.Path]::GetFullPath($Java8)
  stage_root = $staging
  timeout_seconds = 120
}
$json = $report | ConvertTo-Json -Depth 12
[IO.File]::WriteAllText($config,$json,(New-Object Text.UTF8Encoding($false)))
Write-Output "FORGE_CACHE_CONFIG_ONLY_NOT_INSTALLED: $config"
