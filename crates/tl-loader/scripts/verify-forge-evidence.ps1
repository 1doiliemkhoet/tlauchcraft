#requires -Version 5.1
<# Verify inspector and BINPATCH evidence without re-extracting or executing Java.
   Intended for the existing Control-approved Forge 36.2.29 installer only. #>
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Installer,
    [Parameter(Mandatory=$true)][string]$InspectionReport,
    [Parameter(Mandatory=$true)][string]$MetadataDirectory,
    [Parameter(Mandatory=$true)][string]$BinpatchDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Fail([string]$reason) { throw "FORGE_EVIDENCE_FAILED: $reason" }
$expected = '7d247279cf06eed6ffcf609f36d11e34c70350f3'
if (-not (Test-Path -LiteralPath $Installer -PathType Leaf)) { Fail 'Installer missing' }
if ((Get-FileHash -LiteralPath $Installer -Algorithm SHA1).Hash.ToLowerInvariant() -ne $expected) { Fail 'Installer SHA-1 mismatch' }
if (-not (Test-Path -LiteralPath $InspectionReport -PathType Leaf)) { Fail 'Inspection report missing' }
$inspection = Get-Content -LiteralPath $InspectionReport -Raw | ConvertFrom-Json
if ($inspection.status -ne 'INSPECTED_METADATA_ONLY_NOT_INSTALLED' -or
    $inspection.sha1_verified -ne $expected -or
    $inspection.processor_count_client -ne 4 -or
    $inspection.processor_count_all -ne 4 -or
    $inspection.profile_libraries -ne 39 -or
    $inspection.derived_libraries -ne 26) { Fail 'Inspector report mismatch' }
$profileFile = Join-Path $MetadataDirectory 'install_profile.json'
$versionFile = Join-Path $MetadataDirectory 'version.json'
if (-not (Test-Path -LiteralPath $profileFile -PathType Leaf) -or -not (Test-Path -LiteralPath $versionFile -PathType Leaf)) { Fail 'Extracted manifests missing' }
$profile = Get-Content -LiteralPath $profileFile -Raw | ConvertFrom-Json
$version = Get-Content -LiteralPath $versionFile -Raw | ConvertFrom-Json
if ($profile.version -ne '1.16.5-forge-36.2.29' -or $profile.minecraft -ne '1.16.5' -or
    $version.id -ne $profile.version -or $version.inheritsFrom -ne '1.16.5' -or
    $version.mainClass -ne 'cpw.mods.modlauncher.Launcher' -or
    @($profile.processors).Count -ne 4 -or @($profile.libraries).Count -ne 39 -or @($version.libraries).Count -ne 26) {
    Fail 'Extracted official JSON metadata inconsistent'
}
$patch = Join-Path $BinpatchDirectory 'client.lzma'
$patchReport = Join-Path $BinpatchDirectory 'binpatch-provenance.json'
if (-not (Test-Path -LiteralPath $patch -PathType Leaf) -or -not (Test-Path -LiteralPath $patchReport -PathType Leaf)) { Fail 'BINPATCH or provenance report missing' }
$attestation = Get-Content -LiteralPath $patchReport -Raw | ConvertFrom-Json
$digest = (Get-FileHash -LiteralPath $patch -Algorithm SHA1).Hash.ToLowerInvariant()
if ($attestation.status -ne 'VERIFIED_INSTALLER_ENTRY_NOT_EXECUTED' -or
    $attestation.installer_sha1 -ne $expected -or
    $attestation.entry -ne 'data/client.lzma' -or
    $attestation.extracted_sha1 -ne $digest -or
    $attestation.bytes -ne (Get-Item -LiteralPath $patch).Length) {
    Fail 'BINPATCH integrity/provenance mismatch'
}
[pscustomobject]@{
    Status = 'METADATA_AND_BINPATCH_EVIDENCE_VERIFIED_NOT_INSTALLED'
    InstallerSha1 = $expected
    ProfileLibraries = @($profile.libraries).Count
    DerivedLibraries = @($version.libraries).Count
    Processors = @($profile.processors).Count
    BinpatchSha1 = $digest
    BinpatchBytes = $attestation.bytes
    JavaExecuted = $false
    InstanceReady = $false
} | Format-List
