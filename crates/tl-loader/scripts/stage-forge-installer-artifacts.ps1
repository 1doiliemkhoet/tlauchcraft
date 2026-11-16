#requires -Version 5.1
<#
 Stage the Forge Universal / derived library entries from an independently
 SHA1-approved official installer JAR. Never executes Java/JARs.
 Only URL-empty artifacts declared by both verified installer metadata files
 may be extracted, and only when byte length + SHA1 match upstream exactly.
 A missing installer entry is reported as MISSING, not fabricated as installed.
#>
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$Installer,
 [Parameter(Mandatory=$true)][string]$ExpectedInstallerSha1,
 [Parameter(Mandatory=$true)][string]$InstallProfileJson,
 [Parameter(Mandatory=$true)][string]$DerivedVersionJson,
 [Parameter(Mandatory=$true)][string]$NewStageRoot
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Deny([string]$reason) { throw "FORGE_OFFICIAL_INSTALLER_STAGE_DENIED: $reason" }
if ($ExpectedInstallerSha1 -notmatch '^[a-fA-F0-9]{40}$') { Deny 'Installer must have independently approved SHA1' }
$profile=Get-Content -LiteralPath $InstallProfileJson -Raw | ConvertFrom-Json
$derived=Get-Content -LiteralPath $DerivedVersionJson -Raw | ConvertFrom-Json
$expectedId="$($profile.minecraft)-forge-$($profile.version -replace '^.*-forge-','')"
if ($profile.version -ne $expectedId -or $derived.id -ne $expectedId -or
    $derived.inheritsFrom -ne $profile.minecraft) { Deny 'Installer and derived metadata version mismatch' }
$installerFile=[IO.Path]::GetFullPath($Installer)
$root=[IO.Path]::GetFullPath($NewStageRoot)
if (-not [IO.File]::Exists($installerFile) -or (Get-FileHash -LiteralPath $installerFile -Algorithm SHA1).Hash.ToLowerInvariant() -cne $ExpectedInstallerSha1.ToLowerInvariant()) { Deny 'Installer official digest mismatch' }
if ([IO.Directory]::Exists($root) -or [IO.File]::Exists($root)) { Deny 'Stage must be new / no overwrites' }
if ([IO.Path]::GetPathRoot($root) -eq $root -or
  @($root -split '[\\/]' | Where-Object { $_ -match '^(?i:worlds|saves|profiles|instances|\.minecraft)$' }).Count -gt 0) { Deny 'Stage must not touch user Minecraft data' }
$wanted=@()
foreach ($lib in @($profile.libraries)+@($derived.libraries)) {
  $artifact=$lib.downloads.artifact
  if ([string]$artifact.url -ne '') { continue }
  $path=[string]$artifact.path
  $name=[string]$lib.name
  if ($path -notmatch '^[A-Za-z0-9_.+/-]+$' -or $path -match '(^|/)\.\.?(/|$)' -or $path.StartsWith('/') -or
      $name -notmatch '^net\.minecraftforge:forge:[A-Za-z0-9_.+-]+(?::universal)?$' -or
      $artifact.sha1 -notmatch '^[a-fA-F0-9]{40}$' -or [long]$artifact.size -le 0) { Deny "Untrusted URL-empty artifact: $name" }
  $wanted+=@{name=$name;relative=$path;sha1=([string]$artifact.sha1).ToLowerInvariant();size=[long]$artifact.size}
}
$unique=@($wanted | Group-Object -Property name)
if ($unique.Count -ne 2 -or @($unique | Where-Object Count -ne 1).Count -gt 0) { Deny 'Expected exactly two unique URL-empty Forge libraries' }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::OpenRead($installerFile)
try {
 if ($zip.Entries.Count -gt 8192) { Deny 'Too many installer archive entries' }
 $plan=@()
 foreach($entryInfo in $wanted) {
   $zipName='maven/'+$entryInfo.relative
   $matches=@($zip.Entries | Where-Object { $_.FullName -ceq $zipName })
   if ($matches.Count -eq 0) {
     $plan+=@{name=$entryInfo.name;state='NOT_EMBEDDED_NEEDS_PROCESSOR_OR_INSTALLER_GENERATION';relative=$entryInfo.relative}
     continue
   }
   if ($matches.Count -ne 1 -or $matches[0].Length -ne $entryInfo.size -or $entryInfo.size -gt 67108864) {
     Deny "Duplicate or unexpected size for $zipName"
   }
   $plan+=@{name=$entryInfo.name;state='PENDING_EXTRACTION';relative=$entryInfo.relative;sha1=$entryInfo.sha1;size=$entryInfo.size;entry=$matches[0]}
 }
 [void][IO.Directory]::CreateDirectory($root)
 $result=@()
 foreach($item in $plan) {
   if ($item.state -ne 'PENDING_EXTRACTION') {
     $result+=[ordered]@{coordinate=$item.name;state=$item.state;relative_path=$item.relative}
     continue
   }
   $target=[IO.Path]::Combine($root,$item.relative.Replace('/',[IO.Path]::DirectorySeparatorChar))
   [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
   $src=$item.entry.Open()
   try {
     $dst=[IO.File]::Open($target,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
     try { $src.CopyTo($dst) } finally { $dst.Dispose() }
   } finally { $src.Dispose() }
   if (([IO.FileInfo]$target).Length -ne $item.size -or (Get-FileHash -LiteralPath $target -Algorithm SHA1).Hash.ToLowerInvariant() -cne $item.sha1) {
     Deny "Extracted installer artifact checksum mismatch: $($item.name)"
   }
   $result+=[ordered]@{coordinate=$item.name;state='EXTRACTED_OFFICIAL_HASH_VERIFIED_NO_JAVA';relative_path=$item.relative;sha1=$item.sha1;size_bytes=$item.size;path=$target}
 }
 $report=[ordered]@{
   schema='tl-forge-installer-extract/v1'
   status=$(if(@($result|Where-Object state -ne 'EXTRACTED_OFFICIAL_HASH_VERIFIED_NO_JAVA').Count -eq 0){'INSTALLER_ENTRIES_VERIFIED_NO_JAVA'}else{'NEEDS_GENERATED_FORGE_ARTIFACTS'})
   installer=$installerFile;installer_sha1=$ExpectedInstallerSha1.ToLowerInvariant()
   minecraft_version=[string]$profile.minecraft
   forge_version=([string]$profile.version).Replace("$($profile.minecraft)-forge-",'')
   artifacts=$result
   java_executed=$false;forge_installed=$false
 }
 [IO.File]::WriteAllText((Join-Path $root 'forge-installer-extraction.json'),($report|ConvertTo-Json -Depth 10),(New-Object Text.UTF8Encoding($false)))
 $report|ConvertTo-Json -Depth 10
} finally { $zip.Dispose() }
