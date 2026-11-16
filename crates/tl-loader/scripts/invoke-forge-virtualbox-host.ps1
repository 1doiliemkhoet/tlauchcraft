#requires -Version 5.1
<#
  One host command: check VirtualBox win10 isolation, securely prompt credentials,
  copy 2 signed-by-repo text harness files via Guest Additions guestcontrol,
  run NO-JAVA preflight, retrieve the JSON, recheck host isolation.
  Does NOT change VM settings/snapshots or network/clipboard/shared folders.
  Guest password exists only in a restricted short-lived temporary passwordfile
  required by VBoxManage, never in command line/log/repository; file is deleted
  in finally even when guestcontrol fails.
#>
[CmdletBinding()]
param(
  [string]$Vm='win10',
  [Parameter(Mandatory=$true)][string]$GuestUser,
  [string]$OutputDirectory,
  [string]$VBoxManage='C:\Program Files\Oracle\VirtualBox\VBoxManage.exe'
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Fail([string]$msg) { throw "VBOX_HOST_PREFLIGHT_FAILED: $msg" }
if (-not [IO.File]::Exists($VBoxManage)) { Fail "VBoxManage not found: $VBoxManage" }
if ($Vm -cne 'win10') { Fail 'Only the Control-reviewed win10 VM is approved by this harness' }
function HostSnapshot {
  $text=(& $VBoxManage showvminfo $Vm --machinereadable 2>&1 | Out-String)
  if ($LASTEXITCODE -ne 0) { Fail 'VBoxManage showvminfo failed' }
  $fields=@{}
  foreach($line in ($text -split "\r?\n")) {
    if ($line -match '^([^=]+)=(.*)$') { $fields[$Matches[1]]=$Matches[2].Trim('"') }
  }
  $issues=@()
  foreach($i in 1..8) {
    if (-not $fields.ContainsKey("nic$i") -or $fields["nic$i"] -ne 'none') { $issues+="nic$i" }
  }
  foreach($pair in @(@('clipboard','disabled'),@('draganddrop','disabled'),@('usb','off'),@('vrde','off'))) {
    if (-not $fields.ContainsKey($pair[0]) -or $fields[$pair[0]] -ne $pair[1]) { $issues+=$pair[0] }
  }
  if (-not $fields.ContainsKey('VMState') -or $fields['VMState'] -ne 'running') { $issues+='VMState' }
  $shared=@($fields.Keys | Where-Object { $_ -match '^(?i:SharedFolderName|SharedFolderPath|SharedFolderWritable|SharedFolderAutoMount|SharedFolderSymlink)' })
  if ($shared.Count -gt 0) { $issues+='shared-folders' }
  [pscustomobject]@{ status=$(if($issues.Count -eq 0){'VBOX_HOST_CONFIG_PASS_NO_JAVA'}else{'VBOX_HOST_CONFIG_FAILED'});vm=$Vm;issues=$issues;shared_folder_keys=$shared;checked_utc=[DateTime]::UtcNow.ToString('o') }
}
$hostBefore=HostSnapshot
if ($hostBefore.status -ne 'VBOX_HOST_CONFIG_PASS_NO_JAVA') { Fail "Host unsafe: $($hostBefore.issues -join ',')" }
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) { $OutputDirectory=Join-Path $PWD ("forge-vbox-evidence-"+[guid]::NewGuid().ToString('N')) }
$out=[IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $out) { Fail 'OutputDirectory must not exist; no overwrite' }
$cred=Get-Credential -UserName $GuestUser -Message 'VirtualBox guestcontrol password; never put it in the command line'
if ($null -eq $cred -or $cred.UserName -cne $GuestUser) { Fail 'Guest authentication cancelled or mismatched' }
$source=Join-Path $PSScriptRoot 'test-forge-virtualbox-guest.ps1'
$job=Join-Path $PSScriptRoot 'ForgeJobProcess.cs'
foreach($file in @($source,$job)) { if (-not [IO.File]::Exists($file)) { Fail "Missing repo harness: $file" } }
$runtime=Join-Path ([IO.Path]::GetTempPath()) ('forge-vbox-auth-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($runtime)
$who=[Security.Principal.WindowsIdentity]::GetCurrent().User
$acl=New-Object Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true,$false)
$acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($who,[Security.AccessControl.FileSystemRights]::FullControl,([Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)))
Set-Acl -LiteralPath $runtime -AclObject $acl
$pw=Join-Path $runtime 'guest-pass'
$plain=[IntPtr]::Zero
try {
  # VBoxManage 7.2 documents --passwordfile but not SecureString input.
  # The plaintext file has an owner-only ACL, is outside repository,
  # is never displayed, and is disposed/deleted in the same invocation.
  $plain=[Runtime.InteropServices.Marshal]::SecureStringToGlobalAllocUnicode($cred.Password)
  $secret=[Runtime.InteropServices.Marshal]::PtrToStringUni($plain)
  [IO.File]::WriteAllText($pw,$secret,(New-Object Text.UTF8Encoding($false)))
  $secret=$null
  [Runtime.InteropServices.Marshal]::ZeroFreeGlobalAllocUnicode($plain)
  $plain=[IntPtr]::Zero
  [void][IO.Directory]::CreateDirectory($out)
  $hostPath=Join-Path $out 'host-before.json'
  [IO.File]::WriteAllText($hostPath,($hostBefore|ConvertTo-Json -Depth 5),(New-Object Text.UTF8Encoding($false)))
  $guest='C:\ForgeAudit\run-'+[guid]::NewGuid().ToString('N')
  $common=@("--username=$GuestUser","--passwordfile=$pw")
  function VBox([string[]]$commandArgs) {
    & $VBoxManage @commandArgs 2>&1 | ForEach-Object { if ($_ -is [System.Management.Automation.ErrorRecord]) { throw $_ } else { Write-Output $_ } }
    if ($LASTEXITCODE -ne 0) { Fail "VBoxManage guestcontrol command failed: $($commandArgs[2]) (exit $LASTEXITCODE)" }
  }
  VBox (@('guestcontrol',$Vm,'mkdir')+$common+@('--parents',$guest)) | Out-Null
  foreach ($file in @($source,$job,$hostPath)) {
    VBox (@('guestcontrol',$Vm,'copyto')+$common+@('--no-replace',$file,$guest+'\')) | Out-Null
  }
  $command='C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
  $run=@('guestcontrol',$Vm,'run')+$common+@("--exe=$command",'--wait-stdout','--wait-stderr','--timeout=120000','--','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',"$guest\test-forge-virtualbox-guest.ps1",'-HostAttestation',"$guest\host-before.json",'-EvidencePath',"$guest\virtualbox-preflight.json",'-StageParent',$guest,'-JobSource',"$guest\ForgeJobProcess.cs")
  # Failed preflight must remain a failure even if JSON can be retrieved.
  $guestError=$null
  try { VBox $run | Out-Null } catch { $guestError=$_.Exception.Message }
  $guestReport=Join-Path $out 'virtualbox-preflight.json'
  try { VBox (@('guestcontrol',$Vm,'copyfrom')+$common+@('--no-replace',"$guest\virtualbox-preflight.json",$guestReport)) | Out-Null }
  catch { Fail "Could not retrieve guest JSON evidence; guest run error: $guestError" }
  if (-not [IO.File]::Exists($guestReport)) { Fail 'Guest JSON evidence not retrieved' }
  $body=Get-Content -LiteralPath $guestReport -Raw | ConvertFrom-Json
  $after=HostSnapshot
  [IO.File]::WriteAllText((Join-Path $out 'host-after.json'),($after|ConvertTo-Json -Depth 5),(New-Object Text.UTF8Encoding($false)))
  if ($after.status -ne 'VBOX_HOST_CONFIG_PASS_NO_JAVA' -or $guestError -or $body.status -ne 'VM_PREFLIGHT_PASS_NO_JAVA' -or
      @($body.checks | Where-Object { $_.state -ne 'PASS' }).Count -ne 0 -or $body.java_executed -ne $false -or $body.forge_execution_verified -ne $false) {
    Fail "Guest/host evidence rejected; result=$($body.status); guestError=$guestError. Review $out"
  }
  $verifier=Join-Path $PSScriptRoot 'assert-forge-vbox-evidence.ps1'
  $verified=& $verifier -HostBefore $hostPath -GuestEvidence $guestReport -HostAfter (Join-Path $out 'host-after.json') | ConvertFrom-Json
  if ($verified.status -ne 'HOST_AND_VM_PREFLIGHT_VERIFIED_NO_JAVA' -or $verified.checks_passed -ne 13) {
    Fail 'Host/guest independent verdict gate was not satisfied'
  }
  Write-Output "VBOX_HOST_AND_GUEST_PREFLIGHT_PASS_NO_JAVA: $out"
} finally {
  if ($plain -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeGlobalAllocUnicode($plain) }
  $cred=$null
  if ([IO.Directory]::Exists($runtime)) { [IO.Directory]::Delete($runtime,$true) }
}
