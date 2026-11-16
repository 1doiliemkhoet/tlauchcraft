#requires -Version 5.1
<#
 Extract ONLY official Forge URL-less runtime JARs embedded in a verified
 Forge installer ZIP, if present. No Java/Forge execution or instance writes.
 Every copy must match its *own* official derived/profile SHA-1 + exact size.
 Missing embedded output is NeedsAction, never silently generated/fabricated.
#>
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$Installer,
 [Parameter(Mandatory=$true)][string]$OfficialProfile,
 [Parameter(Mandatory=$true)][string]$OfficialVersion,
 [Parameter(Mandatory=$true)][string]$NewMavenRoot
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Deny([string]$reason) { throw "FORGE_EMBEDDED_RUNTIME_DENIED: $reason" }
$installerPath=[IO.Path]::GetFullPath($Installer)
$targetRoot=[IO.Path]::GetFullPath($NewMavenRoot)
$profile=Get-Content -LiteralPath $OfficialProfile -Raw | ConvertFrom-Json
$derived=Get-Content -LiteralPath $OfficialVersion -Raw | ConvertFrom-Json
$version=[string]$profile.version
if ($version -notmatch '^(?<mc>1\.16\.5)-forge-(?<forge>36\.2\.[0-9]+)$') { Deny 'Unsupported Forge version; use exact pack manifest version' }
$mc=$Matches['mc']; $forge=$Matches['forge']
if ($derived.id -cne $version -or $derived.inheritsFrom -cne $mc) { Deny 'Profile/derived version conflict' }
$expectedName="forge-$mc-$forge-installer.jar"
if ([IO.Path]::GetFileName($installerPath) -cne $expectedName -or -not [IO.File]::Exists($installerPath)) { Deny 'Wrong Forge installer file' }
# Current audited official package; other revisions require a separately
# Control-approved installer source/hash policy, not an invented constant.
if ($version -ne '1.16.5-forge-36.2.29') { Deny "Installer digest for $version not yet audited" }
$pinned='7d247279cf06eed6ffcf609f36d11e34c70350f3'
if ((Get-FileHash -LiteralPath $installerPath -Algorithm SHA1).Hash.ToLowerInvariant() -cne $pinned) { Deny 'Official installer SHA-1 mismatch' }
if (Test-Path -LiteralPath $targetRoot) { Deny 'New Maven output root already exists' }
if ([IO.Path]::GetPathRoot($targetRoot) -eq $targetRoot) { Deny 'Cannot stage at a filesystem root' }
if (@($targetRoot -split '[\\/]'|Where-Object{$_ -match '^(?i:saves|worlds|profiles|instances|\.minecraft)$'}).Count -gt 0) { Deny 'Never write to existing world/profile or Minecraft data' }
if ($targetRoot.StartsWith(([IO.Path]::GetDirectoryName($installerPath)+'\'),[StringComparison]::OrdinalIgnoreCase) -and
    $targetRoot -match '(?i)\.minecraft') { Deny 'Unsafe output root' }
$requirements=@()
foreach($library in @($profile.libraries)+@($derived.libraries)){
 $a=$library.downloads.artifact
 if ($null -eq $a) { Deny "Missing download descriptor for $($library.name)" }
 if ([string]$a.url -ne '') { continue }
 $relative=[string]$a.path
 if ($relative -notmatch '^[A-Za-z0-9_.+/\-]+$' -or $relative -match '(^|/)\.\.?(/|$)' -or
     $relative.StartsWith('/') -or $relative.Contains('\')) { Deny 'Unsafe official archive relative path' }
 if ($a.sha1 -notmatch '^[0-9a-fA-F]{40}$' -or [long]$a.size -le 0 -or [long]$a.size -gt 67108864) { Deny 'Invalid official generated-artifact checksum/size' }
 $requirements+=@([pscustomobject]@{name=[string]$library.name;relative=$relative;sha1=([string]$a.sha1).ToLowerInvariant();size=[long]$a.size})
}
if ($requirements.Count -ne 2) { Deny "Unexpected URL-less JAR count: $($requirements.Count)" }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::OpenRead($installerPath)
try {
 if ($zip.Entries.Count -gt 8192) { Deny 'Official installer entry bound exceeded' }
 $verified=@()
 $missing=@()
 foreach($required in $requirements){
  $candidates=@($zip.Entries | Where-Object { $_.FullName -ceq ("maven/"+$required.relative) -or $_.FullName -ceq $required.relative })
  if ($candidates.Count -gt 1) { Deny "Ambiguous duplicate embedded Forge JAR: $($required.relative)" }
  if ($candidates.Count -eq 0) { $missing+=@($required.name);continue }
  $entry=$candidates[0]
  if ([long]$entry.Length -ne $required.size) { Deny "Embedded JAR official size mismatch: $($required.name)" }
  $stream=$entry.Open()
  try {
   $memory=New-Object IO.MemoryStream
   try {
    $stream.CopyTo($memory)
    if ($memory.Length -ne $required.size) { Deny 'Archive read length mismatch' }
    $hash=[Security.Cryptography.SHA1]::Create()
    try { $actual=([BitConverter]::ToString($hash.ComputeHash($memory.ToArray()))).Replace('-','').ToLowerInvariant() }
    finally { $hash.Dispose() }
    if ($actual -cne $required.sha1) { Deny "Embedded JAR official SHA-1 mismatch: $($required.name)" }
    $verified+=@([pscustomobject]@{requirement=$required;entry=$entry.FullName;bytes=$memory.ToArray()})
   } finally { $memory.Dispose() }
  } finally { $stream.Dispose() }
 }
 [void][IO.Directory]::CreateDirectory($targetRoot)
 $receipt=@()
 foreach($item in $verified) {
  $relative=$item.requirement.relative.Replace('/',[IO.Path]::DirectorySeparatorChar)
  $target=[IO.Path]::Combine($targetRoot,$relative)
  [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
  $stream=[IO.File]::Open($target,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  try { $stream.Write($item.bytes,0,$item.bytes.Length) } finally { $stream.Dispose() }
  if ((Get-FileHash -LiteralPath $target -Algorithm SHA1).Hash.ToLowerInvariant() -cne $item.requirement.sha1 -or
      ([IO.FileInfo]$target).Length -ne $item.requirement.size) { Deny 'Written JAR changed after verification' }
  $receipt+=@([pscustomobject]@{coordinate=$item.requirement.name;relative_path=$item.requirement.relative;sha1=$item.requirement.sha1;size_bytes=$item.requirement.size;path=$target;source_entry=$item.entry})
 }
 $status= if($missing.Count -eq 0){'FORGE_EMBEDDED_JARS_VERIFIED_NOT_INSTALLED'}else{'FORGE_EMBEDDED_JARS_NEEDS_ACTION'}
 $report=[ordered]@{schema='tl-forge-embedded-runtime/v1';status=$status;version_id=$version;installer_sha1=$pinned;verified=$receipt;missing=$missing;java_executed=$false;instance_ready=$false}
 [IO.File]::WriteAllText((Join-Path $targetRoot 'embedded-runtime-report.json'),($report|ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding($false)))
 $report|ConvertTo-Json -Depth 12
 if ($missing.Count -ne 0) { exit 2 }
} finally { $zip.Dispose() }
