#requires -Version 5.1
<#
 P0 post-processor Forge library resolver. NO Java is run.
 Consume the official installer-extraction receipt and a completed Forge stage.
 Copy only checksum-verified URL-empty official Forge artifacts into a new
 Maven-layout subtree, refusing overwrites and symlink/reparse paths.
 This DOES NOT declare the Minecraft instance installed or playable.
#>
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$InstallProfileJson,
 [Parameter(Mandatory=$true)][string]$DerivedVersionJson,
 [Parameter(Mandatory=$true)][string]$InstallerExtractionJson,
 [Parameter(Mandatory=$true)][string]$ProcessorStage,
 [Parameter(Mandatory=$true)][string]$NewEvidenceJson
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Deny([string]$why) { throw "FORGE_LIBRARY_RESOLVE_DENIED: $why" }
function SafeChild([string]$stage,[string]$relative) {
 if ([IO.Path]::IsPathRooted($relative) -or $relative.Contains(':') -or
     $relative -match '(^|[\\/])\.\.?([\\/]|$)') { Deny "Unsafe Maven relative path: $relative" }
 $full=[IO.Path]::GetFullPath((Join-Path $stage $relative))
 if (-not $full.StartsWith($stage.TrimEnd([char[]]@('\','/'))+'\',[StringComparison]::OrdinalIgnoreCase)) { Deny 'Artifact escaped stage' }
 $full
}
function CheckHash([string]$file,[string]$sha,[long]$size) {
 if (-not [IO.File]::Exists($file) -or
     ([IO.FileInfo]$file).Length -ne $size -or
     (Get-FileHash -LiteralPath $file -Algorithm SHA1).Hash.ToLowerInvariant() -cne $sha.ToLowerInvariant()) { return $false }
 $path=[IO.Path]::GetFullPath($file)
 while (-not [string]::IsNullOrWhiteSpace($path)) {
  if (([IO.File]::Exists($path) -or [IO.Directory]::Exists($path)) -and
      ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { return $false }
  $parent=[IO.Path]::GetDirectoryName($path)
  if ($parent -eq $path) { break }
  $path=$parent
 }
 $true
}
$profile=Get-Content -LiteralPath $InstallProfileJson -Raw | ConvertFrom-Json
$derived=Get-Content -LiteralPath $DerivedVersionJson -Raw | ConvertFrom-Json
$extract=Get-Content -LiteralPath $InstallerExtractionJson -Raw | ConvertFrom-Json
$stage=[IO.Path]::GetFullPath($ProcessorStage)
if ($profile.minecraft -ne '1.16.5' -or
    $derived.id -ne $profile.version -or
    $extract.schema -ne 'tl-forge-installer-extract/v1' -or
    $extract.minecraft_version -ne $profile.minecraft -or
    $profile.version -ne "$($profile.minecraft)-forge-$($extract.forge_version)" -or
    -not [IO.Directory]::Exists($stage) -or
    -not $stage.StartsWith('C:\ForgeRuns\',[StringComparison]::OrdinalIgnoreCase) -or
    [IO.File]::ReadAllText((Join-Path $stage '.tl-loader-stage')) -ne 'tl-loader-forge-v1' -or
    @($stage -split '[\\/]' | Where-Object { $_ -match '^(?i:worlds|saves|profiles|instances|\.minecraft)$' }).Count -gt 0) {
 Deny 'Not a matching approved private Forge processor stage'
}
if (Test-Path -LiteralPath $NewEvidenceJson) { Deny 'Evidence must be new, never overwrite' }
$requirements=@()
foreach($lib in @($profile.libraries)+@($derived.libraries)) {
 if ([string]$lib.downloads.artifact.url -ne '') { continue }
 $requirements+=@{
  coordinate=[string]$lib.name
  path=[string]$lib.downloads.artifact.path
  sha1=([string]$lib.downloads.artifact.sha1).ToLowerInvariant()
  size=[long]$lib.downloads.artifact.size
 }
}
if ($requirements.Count -ne 2 -or
    @($requirements | Where-Object { $_.coordinate -notmatch '^net\.minecraftforge:forge:1\.16\.5-[0-9.]+(?::universal)?$' }).Count -ne 0) {
 Deny 'Expected two official URL-empty Forge runtime libraries'
}
$observations=@()
$ready=$true
foreach($req in $requirements) {
 $target=SafeChild $stage ('libraries/'+$req.path)
 $source=@($extract.artifacts | Where-Object { $_.coordinate -ceq $req.coordinate -and $_.state -eq 'EXTRACTED_OFFICIAL_HASH_VERIFIED_NO_JAVA' })
 if (Test-Path -LiteralPath $target) {
  if (-not (CheckHash $target $req.sha1 $req.size)) { Deny "Existing generated artifact is corrupted: $($req.coordinate)" }
 } elseif ($source.Count -eq 1 -and (CheckHash ([string]$source[0].path) $req.sha1 $req.size)) {
  [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
  [IO.File]::Copy(([string]$source[0].path),$target,$false)
  if (-not (CheckHash $target $req.sha1 $req.size)) { Deny 'Copied installer artifact did not match official hash' }
 } else {
  $ready=$false
  $observations+=@{coordinate=$req.coordinate;state='NEEDS_GENERATION_OR_INSTALLER_ENTRY';relative_path=$req.path}
  continue
 }
 $observations+=@{
  coordinate=$req.coordinate
  relative_path=$req.path
  path=$target
  official_sha1=$req.sha1
  size_bytes=$req.size
  origin='VerifiedInstallerOrProcessor'
  state='OFFICIAL_HASH_VERIFIED'
 }
}
$evidence=[ordered]@{
 schema='tl-forge-generated-libraries/v1'
 status=$(if($ready){'FORGE_LIBRARIES_VERIFIED_NO_JAVA'}else{'FORGE_LIBRARIES_INCOMPLETE'})
 libraries=@($observations)
 instance_ready=$false
 game_launched=$false
 java_executed_by_this_step=$false
}
[IO.File]::WriteAllText(([IO.Path]::GetFullPath($NewEvidenceJson)),($evidence | ConvertTo-Json -Depth 8),(New-Object Text.UTF8Encoding($false)))
$evidence | ConvertTo-Json -Depth 8
if (-not $ready) { exit 2 }
