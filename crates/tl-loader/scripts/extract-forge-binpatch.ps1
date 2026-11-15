#requires -Version 5.1
<#
.SYNOPSIS
  Read-only-to-installer extraction of the single Forge 36.2.29 BINPATCH entry.
.DESCRIPTION
  Verifies the official SHA-1 before ZIP access; writes only to a NEW dedicated
  evidence directory, never into .minecraft, profiles, worlds, or a launcher cache.
  Does not start Java, execute any JAR, or mark Forge installed.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Installer,
    [Parameter(Mandatory=$true)][string]$DestinationDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Fail([string]$reason) { throw "FORGE_BINPATCH_FAILED: $reason" }
$official = '7d247279cf06eed6ffcf609f36d11e34c70350f3'
$inputPath = [IO.Path]::GetFullPath($Installer)
if ([IO.Path]::GetFileName($inputPath) -cne 'forge-1.16.5-36.2.29-installer.jar') { Fail 'Wrong installer name' }
if (-not [IO.File]::Exists($inputPath)) { Fail 'Installer absent' }
if ((Get-FileHash -LiteralPath $inputPath -Algorithm SHA1).Hash.ToLowerInvariant() -ne $official) { Fail 'Installer SHA-1 mismatch' }
$targetRoot = [IO.Path]::GetFullPath($DestinationDirectory)
if ([IO.Directory]::Exists($targetRoot) -or [IO.File]::Exists($targetRoot)) { Fail 'Destination must not exist' }
if ([IO.Path]::GetPathRoot($targetRoot) -eq $targetRoot) { Fail 'Destination cannot be filesystem root' }
$segments = @($targetRoot.Split([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar))
if ($segments | Where-Object { $_ -match '^(?i:\.minecraft|saves|worlds)$' }) { Fail 'Destination may not be inside user Minecraft data' }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($inputPath)
try {
    if ($archive.Entries.Count -gt 8192) { Fail 'Archive entry limit exceeded' }
    $patches = @($archive.Entries | Where-Object { $_.FullName -ceq 'data/client.lzma' })
    if ($patches.Count -ne 1) { Fail 'Expected exactly one official data/client.lzma entry' }
    $entry = $patches[0]
    if ($entry.Length -le 0 -or $entry.Length -gt 67108864) { Fail 'Unreasonable BINPATCH size' }
    [void][IO.Directory]::CreateDirectory($targetRoot)
    $target = [IO.Path]::Combine($targetRoot, 'client.lzma')
    $source = $entry.Open()
    try {
        $dest = New-Object IO.FileStream($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try {
            $buffer = New-Object byte[] 65536
            [long]$bytes = 0
            while (($n = $source.Read($buffer,0,$buffer.Length)) -gt 0) {
                $bytes += $n
                if ($bytes -gt 67108864) { Fail 'BINPATCH decompression size limit exceeded' }
                $dest.Write($buffer,0,$n)
            }
        } finally { $dest.Dispose() }
    } finally { $source.Dispose() }
    if ($bytes -ne $entry.Length) { Fail 'BINPATCH extracted byte count mismatch' }
    $digest = (Get-FileHash -LiteralPath $target -Algorithm SHA1).Hash.ToLowerInvariant()
    $result = [ordered]@{
        status='VERIFIED_INSTALLER_ENTRY_NOT_EXECUTED'
        installer=$inputPath
        installer_sha1=$official
        entry='data/client.lzma'
        extracted=$target
        extracted_sha1=$digest
        bytes=$bytes
        note='Inspector extraction only; not permission to run Java or mark Minecraft Ready'
    }
    $json = $result | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText(([IO.Path]::Combine($targetRoot,'binpatch-provenance.json')), $json, (New-Object Text.UTF8Encoding($false)))
    Write-Output $json
} finally {
    $archive.Dispose()
}
