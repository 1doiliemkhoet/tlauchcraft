#requires -Version 5.1
# Pure Windows evidence verdict regression: no VM, no Java, no guest access.
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$root=Join-Path ([IO.Path]::GetTempPath()) ("tl-vbox-json-"+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
try {
 $hostEvidence=[ordered]@{vm='win10';status='VBOX_HOST_CONFIG_PASS_NO_JAVA';issues=@();shared_folder_keys=@();checked_utc='2026-11-16T00:00:00Z'}
 $names=@('host_vm_configuration','dedicated_guest_stage_parent','guest_no_active_network',
 'guest_additions_shared_folder_inspection_available','guest_no_host_shared_mounts',
 'guest_fresh_stage','guest_stage_acl','guest_stage_write','job_helper_present',
 'job_normal_exit','job_timeout','job_descendant_observed','job_descendant_cleanup')
 $guest=[ordered]@{schema='tl-forge-vbox-preflight/v1';status='VM_PREFLIGHT_PASS_NO_JAVA';checks=@($names | ForEach-Object { [ordered]@{name=$_;state='PASS';detail='verified-for-test'} });error=$null;java_executed=$false;forge_execution_verified=$false;instance_ready=$false}
 $before=Join-Path $root 'before.json'
 $after=Join-Path $root 'after.json'
 $report=Join-Path $root 'guest.json'
 $utf8=New-Object Text.UTF8Encoding($false)
 function SaveJson([string]$path,$value) { [IO.File]::WriteAllText($path,($value|ConvertTo-Json -Depth 12),$utf8) }
 SaveJson $before $hostEvidence; SaveJson $after $hostEvidence; SaveJson $report $guest
 $check=Join-Path $PSScriptRoot '../scripts/assert-forge-vbox-evidence.ps1'
 $pass=& $check -HostBefore $before -HostAfter $after -GuestEvidence $report | ConvertFrom-Json
 if ($pass.status -ne 'HOST_AND_VM_PREFLIGHT_VERIFIED_NO_JAVA' -or $pass.checks_passed -ne 13) { throw 'Full verified mock JSON rejected' }
 foreach($name in @('guest_no_active_network','guest_no_host_shared_mounts','guest_stage_acl','job_timeout','job_descendant_cleanup')) {
   $guest.checks=@($names | ForEach-Object { [ordered]@{name=$_;state=$(if($_ -eq $name){'FAIL'}else{'PASS'});detail='verified-for-test'} })
   SaveJson $report $guest
   $refused=$false
   try { & $check -HostBefore $before -HostAfter $after -GuestEvidence $report | Out-Null }
   catch { if ($_.Exception.Message -match 'VBOX_EVIDENCE_REJECTED') { $refused=$true } }
   if (-not $refused) { throw "False PASS despite failed $name" }
 }
 $guest.checks=@($names | ForEach-Object { [ordered]@{name=$_;state='PASS';detail='verified-for-test'} })
 $guest.status='VM_PREFLIGHT_FAILED_NO_JAVA'
 SaveJson $report $guest
 $refused=$false
 try { & $check -HostBefore $before -HostAfter $after -GuestEvidence $report | Out-Null } catch { $refused=$true }
 if (-not $refused) { throw 'False PASS with failed overall guest verdict' }
 Write-Output 'VBOX_EVIDENCE_FAIL_CLOSED_NETWORK_SHARE_ACL_TIMEOUT_CHILD_CLEANUP_PASS'
} finally {
 [IO.Directory]::Delete($root,$true)
}
