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
