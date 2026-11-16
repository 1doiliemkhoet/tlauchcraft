#requires -Version 5.1
<# Builds a locked-down Windows Sandbox configuration, but DOES NOT start it.
   Control must verify .wsb before launching; mapped input is read-only, only
   a fresh evidence stage may be writable. The guest MUST also use a Job Object.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$InputDirectory,
    [Parameter(Mandatory=$true)][string]$SandboxFile,
    [Parameter(Mandatory=$true)][string]$FreshOutputDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Fail([string]$why) { throw "FORGE_SANDBOX_PREP_FAILED: $why" }
$inputRoot=[IO.Path]::GetFullPath($InputDirectory)
$outputRoot=[IO.Path]::GetFullPath($FreshOutputDirectory)
$wsb=[IO.Path]::GetFullPath($SandboxFile)
if (-not (Test-Path -LiteralPath $inputRoot -PathType Container)) { Fail 'Missing reviewed input directory' }
if (Test-Path -LiteralPath $outputRoot) { Fail 'Output directory must be new' }
if (Test-Path -LiteralPath $wsb) { Fail 'Sandbox config must be new' }
if ($outputRoot.StartsWith(($inputRoot.TrimEnd([char[]]@('\','/')) + [IO.Path]::DirectorySeparatorChar),[StringComparison]::OrdinalIgnoreCase) -or
    $inputRoot.StartsWith(($outputRoot.TrimEnd([char[]]@('\','/')) + [IO.Path]::DirectorySeparatorChar),[StringComparison]::OrdinalIgnoreCase)) {
    Fail 'Mapped input/output directories must be separate'
}
foreach ($path in @($inputRoot,$outputRoot,$wsb)) {
    $segments=@($path -split '[\\/]')
    if (@($segments | Where-Object { $_ -match '^(?i:saves|worlds|profiles|instances|\.minecraft)$' }).Count -gt 0) {
        Fail 'Never map existing Minecraft data'
    }
}
if ([IO.Path]::GetPathRoot($outputRoot) -eq $outputRoot) { Fail 'Cannot map filesystem root' }
# Host may only grant write access to the fresh evidence directory. Fail closed
# if Windows ACL setup fails: no sandbox config is created in that case.
[void][IO.Directory]::CreateDirectory($outputRoot)
$identity=[Security.Principal.WindowsIdentity]::GetCurrent().User
$acl=New-Object Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true,$false)
$rights=[Security.AccessControl.FileSystemRights]::FullControl
$inherit=[Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
$prop=[Security.AccessControl.PropagationFlags]::None
$allow=[Security.AccessControl.AccessControlType]::Allow
$acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($identity,$rights,$inherit,$prop,$allow)))
Set-Acl -LiteralPath $outputRoot -AclObject $acl
$xmlInput=[Security.SecurityElement]::Escape($inputRoot)
$xmlOutput=[Security.SecurityElement]::Escape($outputRoot)
$config=@"
<Configuration>
  <VGpu>Disable</VGpu>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <PrinterRedirection>Disable</PrinterRedirection>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <ProtectedClient>Enable</ProtectedClient>
  <MappedFolders>
    <MappedFolder><HostFolder>$xmlInput</HostFolder><SandboxFolder>C:\ForgeInputs</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$xmlOutput</HostFolder><SandboxFolder>C:\ForgeEvidence</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
</Configuration>
"@
[IO.File]::WriteAllText($wsb,$config,(New-Object Text.UTF8Encoding($false)))
Write-Output "FORGE_WINDOWS_SANDBOX_CONFIG_PREPARED_NOT_STARTED: $wsb"
Write-Output 'REQUIRES_CONTROL_APPROVAL_AND_GUEST_JOB_OBJECT_BEFORE_ANY_JAVA'
