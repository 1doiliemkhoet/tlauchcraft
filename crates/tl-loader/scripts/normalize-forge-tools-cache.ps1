#requires -Version 5.1
<#
.SYNOPSIS
Normalize Control's flat maven-tools into a NEW canonical Maven-layout cache.
Nothing is overwritten, Java is never launched, original files are read-only.
#>
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$OfficialProfile,
 [Parameter(Mandatory=$true)][string]$FlatToolsDirectory,
 [Parameter(Mandatory=$true)][string]$NewCanonicalRoot
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Fail([string]$why) { throw "FORGE_CACHE_NORMALIZE_FAILED: $why" }
$profile = Get-Content -LiteralPath $OfficialProfile -Raw | ConvertFrom-Json
$sourceRoot=[IO.Path]::GetFullPath($FlatToolsDirectory)
$targetRoot=[IO.Path]::GetFullPath($NewCanonicalRoot)
if (-not [IO.Directory]::Exists($sourceRoot)) { Fail 'Flat tools directory absent' }
if ([IO.Directory]::Exists($targetRoot) -or [IO.File]::Exists($targetRoot)) { Fail 'Target must not already exist' }
if (-not [IO.Path]::IsPathRooted($sourceRoot) -or -not [IO.Path]::IsPathRooted($targetRoot)) { Fail 'Paths must be absolute' }
if ($targetRoot.StartsWith($sourceRoot.TrimEnd([char[]]@('\','/'))+'\',[StringComparison]::OrdinalIgnoreCase) -or
    $sourceRoot.StartsWith($targetRoot.TrimEnd([char[]]@('\','/'))+'\',[StringComparison]::OrdinalIgnoreCase)) { Fail 'Roots cannot overlap' }
foreach ($root in @($sourceRoot,$targetRoot)) {
 if ($root -split '[\\/]' | Where-Object { $_ -match '^(?i:saves|worlds|profiles|instances|\.minecraft)$' }) { Fail 'Minecraft data path forbidden' }
}
if ($profile.minecraft -ne '1.16.5' -or $profile.version -ne '1.16.5-forge-36.2.29') { Fail 'Unexpected official installer metadata' }
$tools = @('net.minecraftforge:installertools:1.2.6',
           'net.minecraftforge:jarsplitter:1.1.4',
           'net.md-5:SpecialSource:1.8.5',
           'net.minecraftforge:binarypatcher:1.0.12',
           'de.oceanlabs.mcp:mcp_config:1.16.5-20210115.111550@zip')
$manifest=@()
foreach ($coordinate in $tools) {
 $lib = @($profile.libraries | Where-Object { $_.name -ceq $coordinate })
 if ($lib.Count -ne 1) { Fail "Official metadata lacks unique entry: $coordinate" }
 $record=$lib[0].downloads.artifact
 if ([string]::IsNullOrWhiteSpace([string]$record.path) -or [string]$record.path -notmatch '^[a-zA-Z0-9_./+\-]+$' -or
     [string]$record.path -match '(^|/)\.\.?(/|$)' -or [string]$record.path -match '^/') { Fail 'Unsafe Maven relative path' }
 $relative=([string]$record.path).Replace('/',[IO.Path]::DirectorySeparatorChar)
 $filename=[IO.Path]::GetFileName($relative)
 $source=[IO.Path]::Combine($sourceRoot,$filename)
 if (-not [IO.File]::Exists($source)) { Fail "Missing flat artifact: $filename" }
 $length=([IO.FileInfo]$source).Length
 $sha=(Get-FileHash -LiteralPath $source -Algorithm SHA1).Hash.ToLowerInvariant()
 if ($sha -cne ([string]$record.sha1).ToLowerInvariant() -or $length -ne [long]$record.size) { Fail "SHA-1/size mismatch: $coordinate" }
 $url=[string]$record.url
 if ($url -cne "https://maven.minecraftforge.net/$($record.path)" -and
     $url -cne "https://repo.maven.apache.org/maven2/$($record.path)" -and
     $url -cne "https://libraries.minecraft.net/$($record.path)") { Fail "Untrusted URL: $coordinate" }
 $manifest += [pscustomobject]@{ coordinate=$coordinate; source=$source; relative=$relative; sha1=$sha; size=$length; url=$url }
}
# All source digests validated before any destination is created.
[void][IO.Directory]::CreateDirectory($targetRoot)
foreach ($item in $manifest) {
 $target=[IO.Path]::Combine($targetRoot,$item.relative)
 [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
 $from=[IO.File]::Open($item.source,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try {
  $to=[IO.File]::Open($target,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  try { $from.CopyTo($to) } finally { $to.Dispose() }
 } finally { $from.Dispose() }
 if ((Get-FileHash -LiteralPath $target -Algorithm SHA1).Hash.ToLowerInvariant() -cne $item.sha1 -or
     ([IO.FileInfo]$target).Length -ne $item.size) { Fail "Copied artifact integrity mismatch: $($item.coordinate)" }
}
$summary=[ordered]@{ status='CANONICAL_TOOLS_CACHE_VERIFIED_NO_JAVA'; root=$targetRoot; source_root=$sourceRoot; count=$manifest.Count; items=$manifest }
[IO.File]::WriteAllText([IO.Path]::Combine($targetRoot,'normalization-report.json'),($summary | ConvertTo-Json -Depth 8),(New-Object Text.UTF8Encoding($false)))
$summary | ConvertTo-Json -Depth 8
