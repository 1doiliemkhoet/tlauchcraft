#requires -Version 5.1
# Safe host-boundary tests, no VM access and no credentials requested.
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$dir=Join-Path ([IO.Path]::GetTempPath()) ("tl03-vbox-negative-"+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($dir)
try {
  $mock=Join-Path $dir 'fake-vbox.exe'
  # An invalid host VBoxManage must fail BEFORE prompting guest credentials,
  # writing secrets, or touching the VM.
  $script=Join-Path $PSScriptRoot '../scripts/invoke-forge-virtualbox-host.ps1'
  $denied=$false
  try { & $script -Vm 'win10' -GuestUser 'vboxuser' -VBoxManage $mock -OutputDirectory (Join-Path $dir 'never-created') | Out-Null }
  catch { $denied=$_.Exception.Message -match 'VBoxManage not found' }
  if (-not $denied) { throw 'Missing VBoxManage was not fail-closed' }
  if (Test-Path (Join-Path $dir 'never-created')) { throw 'Negative host test wrote output' }
  # VM name restriction rejects unrelated lab VMs before any guest actions.
  [IO.File]::WriteAllText($mock,'fake-no-executable')
  $denied=$false
  try { & $script -Vm 'AI-SOC-LAB' -GuestUser 'vboxuser' -VBoxManage $mock | Out-Null }
  catch { $denied=$_.Exception.Message -match 'Only the Control-reviewed win10 VM' }
  if (-not $denied) { throw 'Unapproved VM name not rejected' }
  foreach ($name in @('test-forge-virtualbox-guest.ps1','invoke-forge-virtualbox-host.ps1')) {
    $file=Join-Path $PSScriptRoot "../scripts/$name"
    $tokens=$null; $errors=$null
    [void][System.Management.Automation.Language.Parser]::ParseFile($file,[ref]$tokens,[ref]$errors)
    if (@($errors).Count -ne 0) { throw "PowerShell syntax invalid in $name : $($errors|Out-String)" }
  }
  $guest=Get-Content (Join-Path $PSScriptRoot '../scripts/test-forge-virtualbox-guest.ps1') -Raw
  if ($guest -notmatch 'VM_PREFLIGHT_FAILED_NO_JAVA' -or
      $guest -notmatch 'VM_PREFLIGHT_PASS_NO_JAVA' -or
      $guest -notmatch 'Get-NetRoute' -or
      $guest -notmatch 'Get-NetAdapter' -or
      $guest -notmatch 'Get-Acl' -or
      $guest -notmatch 'VBoxControl' -or
      $guest -notmatch 'ForgeJobProcess' -or
      $guest -notmatch 'java_executed=\$false' -or
      $guest -notmatch 'forge_execution_verified=\$false') {
    throw 'VirtualBox guest fail-closed evidence checks missing'
  }
  Write-Output 'VBOX_HOST_FAIL_CLOSED_AND_GUEST_POLICY_SOURCE_REGRESSION_PASS_NO_JAVA'
} finally {
  [IO.Directory]::Delete($dir,$true)
}
