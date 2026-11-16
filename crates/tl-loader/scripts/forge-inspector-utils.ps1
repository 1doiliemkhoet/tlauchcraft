# Small pure helpers shared with the SHA-1-gated read-only Forge inspector.
# Keep Set-StrictMode enabled in consumers; missing JSON fields are normal.
function Get-ForgeOptionalProperty {
    param([Parameter(Mandatory=$true)]$Object, [Parameter(Mandatory=$true)][string]$Name)
    if ($null -eq $Object) { return $null }
    $member = $Object.PSObject.Properties[$Name]
    if ($null -eq $member) { return $null }
    return $member.Value
}

function Get-ForgeMavenReferences {
    param([object[]]$Arguments)
    foreach ($argument in $Arguments) {
        foreach ($match in [regex]::Matches([string]$argument, '\[([^\[\]]+)\]')) {
            $coordinate = $match.Groups[1].Value
            if ($coordinate -match '^[A-Za-z0-9_.-]+:[A-Za-z0-9_.-]+:[A-Za-z0-9_.+\-]+(?::[A-Za-z0-9_.-]+)?(?:@(zip|txt))?$') {
                Write-Output $coordinate
            }
        }
    }
}
