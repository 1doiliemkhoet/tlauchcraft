#requires -Version 5.1
<#
.SYNOPSIS
  Inspect official Forge 1.16.5-36.2.29 installer, READ-ONLY. Never launches Java.
.DESCRIPTION
  Verifies the publisher-pinned SHA1 before opening ZIP, reads ONLY bounded
  install_profile.json / version.json, and records client processor/dependency
  details. No extraction, shell invocation, modification of game/instance/world.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Installer,
    [string]$ReportPath,
    [string]$MetadataDir
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ExpectedSha1 = '7d247279cf06eed6ffcf609f36d11e34c70350f3'
$ExpectedName = 'forge-1.16.5-36.2.29-installer.jar'
function Fail([string]$reason) { throw "FORGE_INSPECT_FAILED: $reason" }
$full = [IO.Path]::GetFullPath($Installer)
if (-not [IO.File]::Exists($full)) { Fail "Installer missing: $full" }
if ([IO.Path]::GetFileName($full) -ne $ExpectedName) { Fail "Unexpected filename; expected $ExpectedName" }
$actual = (Get-FileHash -LiteralPath $full -Algorithm SHA1).Hash.ToLowerInvariant()
if ($actual -ne $ExpectedSha1) { Fail "SHA1 mismatch. Expected $ExpectedSha1; actual $actual" }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($full)
try {
    if ($zip.Entries.Count -gt 8192) { Fail "Excessive archive entries" }
    $total = [long]0
    $entryMap = @{}
    foreach ($entry in $zip.Entries) {
        if ($entry.FullName.Contains('..') -or $entry.FullName.StartsWith('/') -or $entry.FullName.Contains([char]92) -or $entry.FullName.Contains(':')) {
            Fail "Archive contains unsafe path: $($entry.FullName)"
        }
        $total += $entry.Length
        if ($total -gt 536870912) { Fail "Archive expanded size limit exceeded" }
        if ($entryMap.ContainsKey($entry.FullName)) { Fail "Duplicate ZIP entry: $($entry.FullName)" }
        $entryMap[$entry.FullName] = $entry
    }
    function Read-ManifestText([string]$name) {
        if (-not $entryMap.ContainsKey($name)) { Fail "Missing $name" }
        $entry = $entryMap[$name]
        if ($entry.Length -gt 4194304 -or $entry.Length -eq 0) { Fail "Invalid $name size" }
        $stream = $entry.Open()
        try {
            $reader = New-Object IO.StreamReader($stream, (New-Object Text.UTF8Encoding($false)), $true)
            try { $content = $reader.ReadToEnd() } finally { $reader.Dispose() }
        } finally { $stream.Dispose() }
        return $content
    }
    $profileText = Read-ManifestText 'install_profile.json'
    $versionText = Read-ManifestText 'version.json'
    $profile = $profileText | ConvertFrom-Json
    $version = $versionText | ConvertFrom-Json
    if ($profile.minecraft -ne '1.16.5') { Fail "Unexpected Minecraft profile: $($profile.minecraft)" }
    if (-not $profile.processors) { Fail "No processors in installer profile" }
    if ($profile.version -ne '1.16.5-forge-36.2.29') { Fail "Unexpected Forge version: $($profile.version)" }
    if ($version.id -ne '1.16.5-forge-36.2.29') { Fail "Unexpected derived version: $($version.id)" }
    if (@($profile.libraries).Count -ne 39) { Fail "Unexpected Forge profile library count: $(@($profile.libraries).Count)" }
    if (@($profile.processors).Count -ne 4) { Fail "Unexpected Forge processor count: $(@($profile.processors).Count)" }
    if ($version.inheritsFrom -ne '1.16.5') { Fail "Unexpected derived version parent" }
    if ($version.mainClass -ne 'cpw.mods.modlauncher.Launcher') { Fail "Unexpected derived mainClass" }
    $count = 0
    $client = @()
    foreach ($processor in @($profile.processors)) {
        $count++
        $sides = @($processor.sides | Where-Object { $null -ne $_ })
        if ($sides.Count -ne 0 -and 'client' -notin $sides) { continue }
        $outputPairs = @()
        if ($null -ne $processor.outputs) {
            foreach ($key in $processor.outputs.PSObject.Properties.Name) {
                $outputPairs += [pscustomobject]@{ token=$key; checksum=[string]$processor.outputs.$key }
            }
        }
        $client += [pscustomobject]@{
            index = $count - 1
            jar = [string]$processor.jar
            classpath = @($processor.classpath)
            arguments = @($processor.args)
            sides = $sides
            outputs = $outputPairs
        }
    }
    if ($client.Count -ne 4) { Fail "Expected four client processors; found $($client.Count)" }
    $profileLibraryDetails = @($profile.libraries | ForEach-Object {
        [pscustomobject]@{
            coordinate = [string]$_.name
            url = [string]$_.downloads.artifact.url
            sha1 = [string]$_.downloads.artifact.sha1
            size = $_.downloads.artifact.size
            path = [string]$_.downloads.artifact.path
        }
    })
    $mavenDependencies = @($client | ForEach-Object {
        @($_.jar) + @($_.classpath) + @($_.arguments | Where-Object { $_ -match '\\[[A-Za-z0-9_.:@+-]+\\]' })
    } | Where-Object { $null -ne $_ -and $_ -ne '' } | Sort-Object -Unique)
    $result = [ordered]@{
        status = 'INSPECTED_METADATA_ONLY_NOT_INSTALLED'
        official_download = 'https://maven.minecraftforge.net/net/minecraftforge/forge/1.16.5-36.2.29/forge-1.16.5-36.2.29-installer.jar'
        installer = $full
        sha1_verified = $actual
        bytes = (Get-Item -LiteralPath $full).Length
        minecraft = [string]$profile.minecraft
        profile_version = [string]$profile.version
        derived_version = [string]$version.id
        derived_main_class = [string]$version.mainClass
        profile_libraries = @($profile.libraries).Count
        derived_libraries = @($version.libraries).Count
        profile_library_details = $profileLibraryDetails
        maven_dependencies_referenced = $mavenDependencies
        data_tokens = @($profile.data.PSObject.Properties.Name)
        processor_count_all = $count
        processor_count_client = $client.Count
        processors_client = $client
        archive_entries = $zip.Entries.Count
        note = 'Read-only: no processor executed; no Minecraft version installed; G2/G3/G5 not verified'
    }
    if ($MetadataDir) {
        $metadataRoot = [IO.Path]::GetFullPath($MetadataDir)
        if ([IO.Directory]::Exists($metadataRoot) -or [IO.File]::Exists($metadataRoot)) { Fail 'Metadata destination already exists (no overwrite)' }
        [void][IO.Directory]::CreateDirectory($metadataRoot)
        [IO.File]::WriteAllText(([IO.Path]::Combine($metadataRoot, 'install_profile.json')), $profileText, (New-Object Text.UTF8Encoding($false)))
        [IO.File]::WriteAllText(([IO.Path]::Combine($metadataRoot, 'version.json')), $versionText, (New-Object Text.UTF8Encoding($false)))
    }
    $json = $result | ConvertTo-Json -Depth 25
    if ($ReportPath) {
        $target = [IO.Path]::GetFullPath($ReportPath)
        if ([IO.File]::Exists($target)) { Fail "Report destination already exists (no overwrite)" }
        if ($target -eq $full) { Fail "Report may not overwrite installer" }
        [IO.File]::WriteAllText($target, $json, (New-Object Text.UTF8Encoding($false)))
    }
    Write-Output $json
} finally {
    $zip.Dispose()
}
