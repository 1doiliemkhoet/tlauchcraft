#requires -Version 5.1
<# Read-only verification of official SpecialSource 1.8.5 Java entry point.
    No third-party jar is executed. A missing binary is not treated as Verified. #>
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Jar)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Fail([string]$msg) { throw "SPECIALSOURCE_PROVENANCE_FAILED: $msg" }
$expected = 'c670c493c436a885c872fa4810bc698628b95922'
if (-not [IO.File]::Exists($Jar)) { Fail 'NeedsAction: official SpecialSource 1.8.5 JAR missing' }
if ([IO.Path]::GetFileName($Jar) -cne 'SpecialSource-1.8.5.jar') { Fail 'Unexpected artifact filename' }
if ((Get-FileHash -LiteralPath $Jar -Algorithm SHA1).Hash.ToLowerInvariant() -ne $expected) { Fail 'Executable SHA-1 differs from official Forge install_profile.json' }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($Jar))
try {
    if ($zip.Entries.Count -gt 20000) { Fail 'Excessive JAR entries' }
    $manifestEntry = $zip.GetEntry('META-INF/MANIFEST.MF')
    if ($null -eq $manifestEntry -or $manifestEntry.Length -le 0 -or $manifestEntry.Length -gt 65536) { Fail 'Missing or oversized JAR MANIFEST' }
    $stream = $manifestEntry.Open()
    try {
        $reader = New-Object IO.StreamReader($stream)
        try { $text = $reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
    # JAR manifests allow folded continuation lines, normalize them before parsing.
    $unfolded = [regex]::Replace($text, "\r?\n ", "")
    $class = $null
    foreach ($line in ($unfolded -split "\r?\n")) {
        if ($line -cmatch '^Main-Class: (.+)$') { $class = $Matches[1].Trim() }
    }
    $required = 'net.md_5.specialsource.SpecialSource'
    if ($class -cne $required) { Fail "Main-Class mismatch: $class" }
    if ($null -eq $zip.GetEntry('net/md_5/specialsource/SpecialSource.class')) { Fail 'Declared Main-Class bytecode entry missing' }
    [pscustomobject]@{
        Status='EXECUTABLE_PROVENANCE_AND_MANIFEST_VERIFIED_NOT_EXECUTED'
        Coordinate='net.md-5:SpecialSource:1.8.5'
        Sha1=$expected
        MainClass=$class
        JavaExecuted=$false
    } | Format-List
} finally { $zip.Dispose() }
