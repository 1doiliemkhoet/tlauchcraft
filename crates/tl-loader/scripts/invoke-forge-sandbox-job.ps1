#requires -Version 5.1
<#
.SYNOPSIS
Run ONE Control-approved executable INSIDE Windows Sandbox, under a native
Windows Job Object. This script cannot create a Windows Sandbox itself.
.DESCRIPTION
Before spawn: checks guest identity, explicit outbound firewall block, absent
network route, approved read-only input root and private output stage. The
CreateProcessW launcher uses CREATE_SUSPENDED -> AssignProcessToJobObject ->
ResumeThread, with KILL_ON_JOB_CLOSE for descendant cleanup and timeout.
No Java invocation happens unless Control explicitly calls this script inside
a preconfigured, inspected Windows Sandbox.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$ExpectedSha1,
    [Parameter(Mandatory=$true)][string[]]$Arguments,
    [Parameter(Mandatory=$true)][string]$FreshStage,
    [ValidateRange(1,900)][int]$TimeoutSeconds=120
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Fail([string]$reason) { throw "FORGE_SANDBOX_JOB_DENIED: $reason" }
if ([Environment]::UserName -ne 'WDAGUtilityAccount') { Fail 'Not running as a Windows Sandbox guest account' }
$src=[IO.Path]::GetFullPath($Executable)
$stage=[IO.Path]::GetFullPath($FreshStage)
if (-not $src.StartsWith('C:\ForgeInputs\',[StringComparison]::OrdinalIgnoreCase)) { Fail 'Executable outside read-only sandbox input mount' }
if (-not $stage.StartsWith('C:\ForgeEvidence\',[StringComparison]::OrdinalIgnoreCase)) { Fail 'Stage outside dedicated sandbox evidence mount' }
if (Test-Path -LiteralPath $stage) { Fail 'Stage must not exist' }
if ($ExpectedSha1 -notmatch '^[a-fA-F0-9]{40}$') { Fail 'Missing reviewed executable digest' }
if ((Get-FileHash -LiteralPath $src -Algorithm SHA1).Hash.ToLowerInvariant() -ne $ExpectedSha1.ToLowerInvariant()) { Fail 'Executable bytes differ from approved hash' }
# Never allow Java execution while the guest has a route to an external network.
$routes=@(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction Stop)
if ($routes.Count -gt 0) { Fail 'Network route present; Sandbox Networking Disable not confirmed' }
$profiles=@(Get-NetFirewallProfile -ErrorAction Stop)
if ($profiles.Count -eq 0) { Fail 'No Windows Firewall policy available' }
# Enforce OS firewall rules, not a caller boolean. Fail closed if privilege denied.
Set-NetFirewallProfile -Profile Domain,Private,Public -DefaultOutboundAction Block -ErrorAction Stop
$enabled=@(Get-NetFirewallProfile -ErrorAction Stop)
if (@($enabled | Where-Object { $_.DefaultOutboundAction -ne 'Block' }).Count -gt 0) { Fail 'Could not enforce outbound block' }
[void][IO.Directory]::CreateDirectory($stage)
$acl=New-Object Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true,$false)
$user=[Security.Principal.WindowsIdentity]::GetCurrent().User
$acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($user,[Security.AccessControl.FileSystemRights]::FullControl,([Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)))
Set-Acl -LiteralPath $stage -AclObject $acl -ErrorAction Stop
Add-Type -Path 'C:\ForgeInputs\ForgeJobProcess.cs' -ErrorAction Stop
$exit=[ForgeJobProcess]::Run($src,[string[]]$Arguments,$stage,($TimeoutSeconds*1000))
if ($exit -ne 0) { Fail "Processor exited $exit" }
Write-Output 'SANDBOX_JOB_PROCESS_EXIT_0_NOT_FORGE_INSTALLED'
