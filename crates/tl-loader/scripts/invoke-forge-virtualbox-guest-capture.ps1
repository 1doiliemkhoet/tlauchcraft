#requires -Version 5.1
<#
  Diagnostic-only guest launcher for the isolated VirtualBox Win10 audit.
  Capture the real child PowerShell process exit and stderr to guest-local
  files: some Guest Additions builds cannot stream process stdout/stderr
  ("waiting flag not supported"). Never launch Java or Forge here.
#>
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath($PSScriptRoot)
$preflight=Join-Path $root 'test-forge-virtualbox-guest.ps1'
$hostAttestation=Join-Path $root 'host-before.json'
$job=Join-Path $root 'ForgeJobProcess.cs'
$guestEvidence=Join-Path $root 'virtualbox-preflight.json'
$stdout=Join-Path $root 'guest-preflight-stdout.txt'
$stderr=Join-Path $root 'guest-preflight-stderr.txt'
$receipt=Join-Path $root 'guest-launch-receipt.json'
$childExit=$null
$failure=$null
$started=$false
try {
  if ([IO.File]::Exists($receipt)) { throw 'Launcher receipt already exists' }
  foreach($file in @($preflight,$hostAttestation,$job)) {
    if (-not [IO.File]::Exists($file)) { throw "Required guest input absent: $file" }
  }
  $powershell='C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
  if (-not [IO.File]::Exists($powershell)) { throw 'Guest PowerShell.exe missing' }
  # Absolute private paths contain no spaces; quotes retained explicitly.
  $arguments='-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -HostAttestation "{1}" -EvidencePath "{2}" -StageParent "{3}" -JobSource "{4}"' -f $preflight,$hostAttestation,$guestEvidence,$root,$job
  $child=Start-Process -FilePath $powershell -ArgumentList $arguments -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -Wait -WindowStyle Hidden -ErrorAction Stop
  $started=$true
  $childExit=[int]$child.ExitCode
  if ($childExit -ne 0) { throw "Guest preflight process exited $childExit (see guest-preflight-stderr.txt)" }
  if (-not [IO.File]::Exists($guestEvidence)) { throw 'Guest preflight exited 0 but did not produce its required JSON' }
  $data=Get-Content -LiteralPath $guestEvidence -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  if ($data.status -ne 'VM_PREFLIGHT_PASS_NO_JAVA' -or $data.java_executed -ne $false -or $data.instance_ready -ne $false) {
    throw "Guest preflight JSON rejected: $($data.status)"
  }
} catch {
  $failure=$_.Exception.Message
} finally {
  $result=[ordered]@{
    schema='tl-forge-vbox-guest-launch/v1'
    status=$(if($null -eq $failure){'GUEST_CHILD_PREFLIGHT_COMPLETED'}else{'GUEST_CHILD_PREFLIGHT_FAILED'})
    child_launched=$started
    child_exit_code=$childExit
    evidence_exists=[IO.File]::Exists($guestEvidence)
    stdout_exists=[IO.File]::Exists($stdout)
    stderr_exists=[IO.File]::Exists($stderr)
    java_executed=$false
    forge_execution_verified=$false
    instance_ready=$false
    error=$failure
  }
  [IO.File]::WriteAllText($receipt,($result|ConvertTo-Json -Depth 7),(New-Object Text.UTF8Encoding($false)))
}
if ($null -ne $failure) {
  Write-Error -ErrorAction Continue "GUEST_CHILD_PREFLIGHT_FAILED: $failure"
  exit 2
}
Write-Output 'GUEST_CHILD_PREFLIGHT_COMPLETED_NO_JAVA'
