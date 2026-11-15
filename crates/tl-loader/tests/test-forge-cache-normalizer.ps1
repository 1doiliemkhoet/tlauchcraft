#requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$temp=Join-Path ([IO.Path]::GetTempPath()) ('tl03-flat-normalizer-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temp)
try {
 $flat=Join-Path $temp 'flat'
 [void][IO.Directory]::CreateDirectory($flat)
 $profile=Join-Path $temp 'synthetic-five-tools.json'
 $items=@(
  'net.minecraftforge:installertools:1.2.6',
  'net.minecraftforge:jarsplitter:1.1.4',
  'net.md-5:SpecialSource:1.8.5',
  'net.minecraftforge:binarypatcher:1.0.12',
  'de.oceanlabs.mcp:mcp_config:1.16.5-20210115.111550@zip'
 )
 $libs=@()
 foreach ($coordinate in $items) {
  $parts=$coordinate.Split(':')
  $group=$parts[0].Replace('.','/')
  $name=$parts[1]
  $version=$parts[2]
  $extension='jar'
  if ($version.Contains('@')) { $v=$version.Split('@'); $version=$v[0]; $extension=$v[1] }
  $filename="$name-$version.$extension"
  $rel="$group/$name/$version/$filename"
  $file=Join-Path $flat $filename
  [IO.File]::WriteAllText($file,("SYNTHETIC TEST BYTES ONLY $coordinate"),(New-Object Text.UTF8Encoding($false)))
  $sha=(Get-FileHash -LiteralPath $file -Algorithm SHA1).Hash.ToLowerInvariant()
  $libs+=@{ name=$coordinate; downloads=@{ artifact=@{path=$rel;sha1=$sha;size=([IO.FileInfo]$file).Length;url="https://maven.minecraftforge.net/$rel"} } }
 }
 $p=@{ minecraft='1.16.5'; version='1.16.5-forge-36.2.29'; libraries=$libs }
 [IO.File]::WriteAllText($profile,($p|ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding($false)))
 $dest=Join-Path $temp 'canonical'
 $normalizer=Join-Path $PSScriptRoot '../scripts/normalize-forge-tools-cache.ps1'
 & $normalizer -OfficialProfile $profile -FlatToolsDirectory $flat -NewCanonicalRoot $dest | Out-Null
 $report=Get-Content -LiteralPath (Join-Path $dest 'normalization-report.json') -Raw | ConvertFrom-Json
 if ($report.count -ne 5) { throw 'Expected five officially described synthetic cache files' }
 foreach ($i in $libs) {
  $target=Join-Path $dest ($i.downloads.artifact.path.Replace('/',[IO.Path]::DirectorySeparatorChar))
  if ((Get-FileHash -LiteralPath $target -Algorithm SHA1).Hash.ToLowerInvariant() -ne $i.downloads.artifact.sha1) { throw 'Normalization copy SHA mismatch' }
 }
 $refused=$false
 try { & $normalizer -OfficialProfile $profile -FlatToolsDirectory $flat -NewCanonicalRoot $dest | Out-Null }
 catch { if ($_.Exception.Message -match 'must not already exist') { $refused=$true } }
 if (-not $refused) { throw 'Existing destination overwrite not rejected' }
 $tamper=Join-Path $flat 'jarsplitter-1.1.4.jar'
 [IO.File]::WriteAllText($tamper,'TAMPERED')
 $bad=Join-Path $temp 'bad-canonical'
 $refused=$false
 try { & $normalizer -OfficialProfile $profile -FlatToolsDirectory $flat -NewCanonicalRoot $bad | Out-Null }
 catch { if ($_.Exception.Message -match 'SHA-1/size mismatch') { $refused=$true } }
 if (-not $refused -or (Test-Path $bad)) { throw 'Tampered cache not rejected before writing destination' }
 Write-Output 'WINDOWS_FLAT_MAVEN_NORMALIZER_5_FILES_PASS_NO_JAVA'
} finally { [IO.Directory]::Delete($temp,$true) }
