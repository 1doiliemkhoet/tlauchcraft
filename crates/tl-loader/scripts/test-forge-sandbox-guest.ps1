#requires -Version 5.1
<#
  CONTROL-GATED: execute manually INSIDE the network-disabled Windows Sandbox.
  Benign Windows OS processes only. No Java/Forge, no network probes.
  Writes one new report under C:\ForgeEvidence. Requires mapped ForgeInputs
  and ForgeEvidence from reviewed .wsb; refuses host Windows.
#>
[CmdletBinding()]
param([string]$Report='C:\ForgeEvidence\forge-guest-smoke-report.json')
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Fail([string]$msg) { throw "FORGE_GUEST_ISOLATION_FAILED: $msg" }
if ([Environment]::UserName -ne 'WDAGUtilityAccount') { Fail 'Not a Windows Sandbox guest' }
if (-not (Test-Path -LiteralPath 'C:\ForgeInputs' -PathType Container) -or
    -not (Test-Path -LiteralPath 'C:\ForgeEvidence' -PathType Container)) { Fail 'Expected mapped mounts absent' }
if (Test-Path -LiteralPath $Report) { Fail 'Report already exists' }
$routes=@(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction Stop)
if ($routes.Count -gt 0) { Fail 'Unexpected default IPv4 route' }
Set-NetFirewallProfile -Profile Domain,Private,Public -DefaultOutboundAction Block -ErrorAction Stop
if (@(Get-NetFirewallProfile | Where-Object { $_.DefaultOutboundAction -ne 'Block' }).Count -gt 0) { Fail 'Outbound firewall not blocked' }
# Verify Windows Sandbox read-only mapped host inputs, not merely the .wsb text.
$denied=$false
try {
  $f=[IO.File]::Open('C:\ForgeInputs\_write_denial_probe.txt',[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  $f.Dispose()
} catch [UnauthorizedAccessException] { $denied=$true }
if (-not $denied) { Fail 'Read-only input mapping was writable' }
$stage='C:\ForgeEvidence\smoke-stage'
if (Test-Path $stage) { Fail 'Smoke stage already exists' }
[void][IO.Directory]::CreateDirectory($stage)
$acl=New-Object Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true,$false)
$user=[Security.Principal.WindowsIdentity]::GetCurrent().User
$rule=New-Object Security.AccessControl.FileSystemAccessRule($user,[Security.AccessControl.FileSystemRights]::FullControl,([Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)
$acl.AddAccessRule($rule)
Set-Acl -LiteralPath $stage -AclObject $acl
$effectiveAcl=Get-Acl -LiteralPath $stage
if (-not $effectiveAcl.AreAccessRulesProtected) { Fail 'Staging ACL inheritance was not disabled' }
[IO.File]::WriteAllText((Join-Path $stage 'write_test.txt'),'guest-only',(New-Object Text.UTF8Encoding($false)))
$helper='C:\ForgeInputs\ForgeJobProcess.cs'
if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) { Fail 'JobObject helper not mounted' }
Add-Type -Path $helper -ErrorAction Stop
$cmd=Join-Path $env:WINDIR 'System32\cmd.exe'
$normal=[ForgeJobProcess]::Run($cmd,([string[]]@('/d','/c','exit','0')),$stage,10000)
if ($normal -ne 0) { Fail 'Benign job did not exit zero' }
# Start a harmless child process and force parent timeout; native KILL_ON_JOB_CLOSE
# must terminate descendants before we report PASS.
$ps=Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
$marker=Join-Path $stage 'child_pid.txt'
$code='$child=Start-Process -FilePath "$env:WINDIR\System32\ping.exe" -ArgumentList ''-n 60 127.0.0.1'' -PassThru -WindowStyle Hidden; [IO.File]::WriteAllText(' + ("'"+$marker.Replace("'","''")+"'") + ',$child.Id.ToString()); Start-Sleep -Seconds 60'
$timedOut=$false
try {
  [void][ForgeJobProcess]::Run($ps,([string[]]@('-NoProfile','-NonInteractive','-Command',$code)),$stage,5000)
} catch [TimeoutException] { $timedOut=$true }
if (-not $timedOut) { Fail 'Job timeout did not trigger' }
if (-not (Test-Path $marker)) { Fail 'Benign grandchild PID was not observed before timeout' }
$childPid=[int][IO.File]::ReadAllText($marker)
Start-Sleep -Milliseconds 500
if (Get-Process -Id $childPid -ErrorAction SilentlyContinue) { Fail "Job descendant survived timeout ($childPid)" }
$summary=[ordered]@{
  status='WINDOWS_SANDBOX_GUEST_OS_ISOLATION_PASS_NO_JAVA'
  guest_account=[Environment]::UserName
  default_routes=$routes.Count
  outbound_firewall='Block'
  input_write_denied=$denied
  stage_acl_inheritance_protected=$effectiveAcl.AreAccessRulesProtected
  native_job_exit=$normal
  timeout_observed=$timedOut
  descendant_cleaned=$true
  java_executed=$false
  instance_ready=$false
}
[IO.File]::WriteAllText($Report,($summary|ConvertTo-Json -Depth 6),(New-Object Text.UTF8Encoding($false)))
$summary | ConvertTo-Json -Depth 6
