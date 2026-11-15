#requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../scripts/forge-inspector-utils.ps1')
$profile = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'fixtures/forge_36229_official_profile.json') -Raw | ConvertFrom-Json
$derived = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'fixtures/forge_36229_official_version.json') -Raw | ConvertFrom-Json
if (@($profile.processors).Count -ne 4 -or @($profile.libraries).Count -ne 39 -or @($derived.libraries).Count -ne 26) { throw 'Real fixture counts mismatch' }
$counts = @(17,2,7,11)
$declared = @(0,2,0,1)
$coords = @('net.minecraftforge:installertools:1.2.6','net.minecraftforge:jarsplitter:1.1.4','net.md-5:SpecialSource:1.8.5','net.minecraftforge:binarypatcher:1.0.12')
for ($i=0; $i -lt 4; $i++) {
    $item = $profile.processors[$i]
    $sides = @((Get-ForgeOptionalProperty -Object $item -Name 'sides') | Where-Object { $null -ne $_ })
    $outputs = Get-ForgeOptionalProperty -Object $item -Name 'outputs'
    $classpath = @((Get-ForgeOptionalProperty -Object $item -Name 'classpath') | Where-Object { $null -ne $_ })
    $args = @((Get-ForgeOptionalProperty -Object $item -Name 'args') | Where-Object { $null -ne $_ })
    $count = if ($null -eq $outputs) { 0 } else { @($outputs.PSObject.Properties).Count }
    if ($item.jar -ne $coords[$i] -or $sides.Count -ne 0 -or $classpath.Count -ne $counts[$i] -or $count -ne $declared[$i] -or $args.Count -eq 0) {
        throw "Real processor #$i invalid optional-field handling"
    }
}
$refs = @(Get-ForgeMavenReferences -Arguments $profile.processors[0].args)
if ($refs -notcontains 'de.oceanlabs.mcp:mcp_config:1.16.5-20210115.111550@zip') { throw 'Maven @zip reference dropped' }
$noOptional = '{}' | ConvertFrom-Json
foreach ($name in @('sides','outputs','args','classpath')) {
    if ($null -ne (Get-ForgeOptionalProperty -Object $noOptional -Name $name)) { throw "Missing $name unexpectedly present" }
}
Write-Output 'FORGE_INSPECTOR_OPTIONAL_FIELDS_AND_MAVEN_ZIP: PASS'

# Tampered installer must be rejected before ZIP inspection or any extraction.
# This does not execute a JAR; it is deliberately not a valid Forge archive.
$temp = Join-Path ([IO.Path]::GetTempPath()) ("tl03-forge-ps-safety-" + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temp)
try {
    $fake = Join-Path $temp 'forge-1.16.5-36.2.29-installer.jar'
    [IO.File]::WriteAllBytes($fake, [byte[]](1,2,3,4))
    $inspector = Join-Path $PSScriptRoot '../scripts/inspect-forge-1165.ps1'
    $extractor = Join-Path $PSScriptRoot '../scripts/extract-forge-binpatch.ps1'
    $verifier = Join-Path $PSScriptRoot '../scripts/verify-forge-evidence.ps1'
    $specialTool = Join-Path $PSScriptRoot '../scripts/verify-specialsource.ps1'
    $fakeSpecial = Join-Path $temp 'SpecialSource-1.8.5.jar'
    [IO.File]::WriteAllBytes($fakeSpecial, [byte[]](1,2,3,4))
    $specialRejected = $false
    try { & $specialTool -Jar $fakeSpecial | Out-Null }
    catch { if ($_.Exception.Message -match 'SHA-1 differs') { $specialRejected = $true } }
    if (-not $specialRejected) { throw 'SpecialSource tamper verification failed open' }
    $dest = Join-Path $temp 'fresh-binpatch'
    $metadata = Join-Path $temp 'metadata'
    $report = Join-Path $temp 'inspection.json'
    foreach ($script in @('inspect','extract','verify')) {
        $rejected = $false
        try {
            switch ($script) {
                'inspect' { & $inspector -Installer $fake -MetadataDir $metadata -ReportPath $report | Out-Null }
                'extract' { & $extractor -Installer $fake -DestinationDirectory $dest | Out-Null }
                'verify' { & $verifier -Installer $fake -InspectionReport $report -MetadataDirectory $metadata -BinpatchDirectory $dest | Out-Null }
            }
        } catch {
            if ($_.Exception.Message -match 'SHA-1 mismatch|SHA1 mismatch') { $rejected = $true }
        }
        if (-not $rejected) { throw "Tampered installer not rejected by $script" }
    }
    if ((Test-Path -LiteralPath $dest) -or (Test-Path -LiteralPath $metadata) -or (Test-Path -LiteralPath $report)) {
        throw 'Tampered installer unexpectedly created output'
    }
} finally {
    [IO.Directory]::Delete($temp, $true)
}
Write-Output 'FORGE_INSPECTOR_AND_BINPATCH_TAMPER_REJECTION: PASS'

# Official-metadata flat-cache normalizer must reject missing inputs
# before creating or modifying the canonical destination.
$source = Join-Path ([IO.Path]::GetTempPath()) ('forge-flat-negative-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($source)
try {
    $dest = Join-Path ([IO.Path]::GetTempPath()) ('forge-canonical-negative-' + [guid]::NewGuid().ToString('N'))
    $normalizer = Join-Path $PSScriptRoot '../scripts/normalize-forge-tools-cache.ps1'
    $official = Join-Path $PSScriptRoot 'fixtures/forge_36229_official_profile.json'
    $rejected = $false
    try { & $normalizer -OfficialProfile $official -FlatToolsDirectory $source -NewCanonicalRoot $dest | Out-Null }
    catch { if ($_.Exception.Message -match 'Missing flat artifact') { $rejected = $true } }
    if (-not $rejected -or (Test-Path -LiteralPath $dest)) { throw 'Normalizer accepted missing flat tools or mutated target' }
} finally { [IO.Directory]::Delete($source,$true) }
Write-Output 'FORGE_FLAT_TOOLS_NORMALIZER_FAIL_CLOSED: PASS'
