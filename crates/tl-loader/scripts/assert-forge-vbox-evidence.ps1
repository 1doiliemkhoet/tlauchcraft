#requires -Version 5.1
# Independent fail-closed verdict validator; contains no VM operations/Java.
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$HostBefore,
 [Parameter(Mandatory=$true)][string]$GuestEvidence,
 [Parameter(Mandatory=$true)][string]$HostAfter
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Deny([string]$msg) { throw "VBOX_EVIDENCE_REJECTED: $msg" }
$before=Get-Content -LiteralPath $HostBefore -Raw | ConvertFrom-Json
$after=Get-Content -LiteralPath $HostAfter -Raw | ConvertFrom-Json
$guest=Get-Content -LiteralPath $GuestEvidence -Raw | ConvertFrom-Json
foreach($hostRecord in @($before,$after)) {
 if ($hostRecord.vm -ne 'win10' -or $hostRecord.status -ne 'VBOX_HOST_CONFIG_PASS_NO_JAVA' -or
     @($hostRecord.issues).Count -ne 0 -or @($hostRecord.shared_folder_keys).Count -ne 0) {
   Deny 'Host VirtualBox configuration failed or not independently attested both before and after'
 }
}
if ($guest.schema -ne 'tl-forge-vbox-preflight/v1' -or
    $guest.status -ne 'VM_PREFLIGHT_PASS_NO_JAVA' -or
    $guest.java_executed -ne $false -or
    $guest.forge_execution_verified -ne $false -or
    $guest.instance_ready -ne $false -or
    $null -ne $guest.error) {
  Deny 'Guest verdict is missing or inconsistent'
}
$required=@(
 'host_vm_configuration','dedicated_guest_stage_parent','guest_no_active_network',
 'guest_additions_shared_folder_inspection_available','guest_no_host_shared_mounts',
 'guest_fresh_stage','guest_stage_acl','guest_stage_write','job_helper_present',
 'job_normal_exit','job_timeout','job_descendant_observed','job_descendant_cleanup'
)
$checks=@($guest.checks)
if ($checks.Count -ne $required.Count) { Deny "Expected $($required.Count) checks; received $($checks.Count)" }
$names=@($checks | ForEach-Object { [string]$_.name })
foreach($name in $required) {
 if (@($checks | Where-Object { $_.name -ceq $name -and $_.state -ceq 'PASS' -and -not [string]::IsNullOrWhiteSpace([string]$_.detail) }).Count -ne 1) {
   Deny "Check not uniquely passed: $name"
 }
}
if (@($names | Sort-Object -Unique).Count -ne $required.Count) { Deny 'Duplicate or unexpected guest checks' }
[pscustomobject]@{schema='tl-forge-vbox-verdict/v1';status='HOST_AND_VM_PREFLIGHT_VERIFIED_NO_JAVA';checks_passed=$required.Count;java_executed=$false;forge_execution_verified=$false;instance_ready=$false} | ConvertTo-Json -Depth 4
