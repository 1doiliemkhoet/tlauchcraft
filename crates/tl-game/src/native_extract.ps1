# Vanilla native extractor. Only SHA1-verified official Mojang archives are accepted by caller.
# Uses .NET ZipArchive, extracts Windows DLL files only and refuses unsafe/duplicate entries.
param(
    [Parameter(Mandatory = $true)][string]$Destination,
    [Parameter(Mandatory = $true)][string]$ArchiveListPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$Archives = @(Get-Content -Raw -LiteralPath $ArchiveListPath | ConvertFrom-Json)
if ($Archives.Count -eq 0) { throw 'No native archives specified' }

$seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
$allowedName = '^[A-Za-z0-9._-]+\.dll$'
$maxFiles = 256
$maxFileBytes = 104857600L
$maxTotalBytes = 536870912L
$files = 0
$total = 0L
$destRoot = [System.IO.Path]::GetFullPath($Destination)
if (-not [System.IO.Directory]::Exists($destRoot)) {
    throw 'Destination must be a pre-created empty directory'
}
if ([System.IO.Directory]::EnumerateFileSystemEntries($destRoot).GetEnumerator().MoveNext()) {
    throw 'Native destination must be empty'
}
$zipFiles = [System.Collections.Generic.List[object]]::new()
try {
    # Phase 1: inspect every archive and every entry before writing any DLL.
    foreach ($archivePath in $Archives) {
        $zip = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
        $zipFiles.Add($zip)
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\', '/')
            if ([string]::IsNullOrWhiteSpace($name) -or $name.StartsWith('/') -or
                $name.Contains(':') -or $name.Contains([char]0) -or
                @($name.Split('/') | Where-Object { $_ -eq '.' -or $_ -eq '..' }).Count -gt 0) {
                throw "Unsafe archive entry name"
            }
            $mode = ($entry.ExternalAttributes -shr 16) -band 61440
            if ($mode -eq 40960 -or $mode -eq 24576 -or $mode -eq 8192) {
                throw 'Archive link/device entry rejected'
            }
            if (-not $name.EndsWith('.dll', [System.StringComparison]::OrdinalIgnoreCase)) {
                continue
            }
            $leaf = [System.IO.Path]::GetFileName($name)
            if ($leaf -cnotmatch $allowedName -or $leaf -eq '.' -or $leaf -eq '..') {
                throw 'Unsafe DLL file name'
            }
            if (-not $seen.Add($leaf)) {
                throw 'Duplicate DLL name across archives'
            }
            if ($entry.Length -gt $maxFileBytes -or $entry.Length -lt 0) {
                throw 'Native DLL exceeds size limit'
            }
            $files++
            $total += $entry.Length
            if ($files -gt $maxFiles -or $total -gt $maxTotalBytes) {
                throw 'Native extraction limit exceeded'
            }
        }
    }
    # Phase 2: create DLL files exclusively. Never expand archive paths to disk.
    foreach ($zip in $zipFiles) {
        foreach ($entry in $zip.Entries) {
            if (-not $entry.FullName.EndsWith('.dll', [System.StringComparison]::OrdinalIgnoreCase)) {
                continue
            }
            $leaf = [System.IO.Path]::GetFileName($entry.FullName.Replace('\', '/'))
            $target = [System.IO.Path]::Combine($destRoot, $leaf)
            $inputStream = $entry.Open()
            $outputStream = $null
            try {
                $outputStream = [System.IO.File]::Open($target,
                    [System.IO.FileMode]::CreateNew,
                    [System.IO.FileAccess]::Write,
                    [System.IO.FileShare]::None)
                $buffer = [byte[]]::new(65536)
                $written = 0L
                while (($read = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    $written += $read
                    if ($written -gt $maxFileBytes -or $written -gt $entry.Length) {
                        throw 'Uncompressed native DLL exceeds size bound'
                    }
                    $outputStream.Write($buffer, 0, $read)
                }
                if ($written -ne $entry.Length) {
                    throw 'Native DLL length mismatch'
                }
                $outputStream.Flush()
            } finally {
                if ($null -ne $outputStream) { $outputStream.Dispose() }
                $inputStream.Dispose()
            }
        }
    }
    if ($files -eq 0) {
        throw 'No Windows DLL natives found'
    }
} finally {
    foreach ($zip in $zipFiles) { $zip.Dispose() }
}
