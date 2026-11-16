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
  # VBox 7.2: file sources cannot be copied with the pre-existing guest
  # directory as the CopyToGuest destination (VBOX_E_IPRT_ERROR).
  # Each operation must supply a fresh, explicit guest *file* path.
  $hostSource=Get-Content -LiteralPath $script -Raw
  if ($hostSource -notmatch [regex]::Escape('$guestFile=Join-Path $guest ([IO.Path]::GetFileName($file))') -or
      $hostSource -notmatch [regex]::Escape('("--target-directory=$guestFile"),$file')) {
    throw 'copyto must use the new fully-qualified guest filename for each source file'
  }
  if ($hostSource -match [regex]::Escape('("--target-directory=$guest"),$file')) {
    throw 'copyto must never use an already-existing guest directory as the file destination'
  }
  # Real VirtualBox 7.2 Guest Additions may suppress the PowerShell stderr
  # stream while returning VBoxManage exit 0. The in-guest capture wrapper
  # must return a separate child receipt + captured stdout/stderr.
  if ($hostSource -notmatch [regex]::Escape("foreach ($file in @($source,$launcher,$job,$hostPath))") -or
      $hostSource -notmatch [regex]::Escape('guest-launch-receipt.json') -or
      $hostSource -notmatch [regex]::Escape('guest-preflight-stderr.txt') -or
      $hostSource -notmatch [regex]::Escape('guest-preflight-stdout.txt') -or
      $hostSource -notmatch [regex]::Escape('GUEST_CHILD_PREFLIGHT_COMPLETED') -or
      $hostSource -notmatch [regex]::Escape('("--target-directory=$hostPathNew"),$guestPath') -or
      $hostSource -notmatch [regex]::Escape('VBox (@(''guestcontrol'',$Vm,''stat'')+$common+@($guestPath))')) {
    throw 'Guest capture receipt, diagnostic files, strict verdict or full-file copyfrom missing'
  }
  $capture=Get-Content -LiteralPath (Join-Path $PSScriptRoot '../scripts/invoke-forge-virtualbox-guest-capture.ps1') -Raw
  if ($capture -notmatch [regex]::Escape('Start-Process -FilePath $powershell') -or
      $capture -notmatch [regex]::Escape('-RedirectStandardError $stderr') -or
      $capture -notmatch [regex]::Escape('-RedirectStandardOutput $stdout') -or
      $capture -notmatch [regex]::Escape("schema='tl-forge-vbox-guest-launch/v1'") -or
      $capture -notmatch [regex]::Escape('child_exit_code=$childExit') -or
      $capture -notmatch [regex]::Escape('evidence_exists=[IO.File]::Exists($guestEvidence)') -or
      $capture -notmatch [regex]::Escape('instance_ready=$false')) {
    throw 'Guest wrapper must capture child evidence and deny false PASS'
  }
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
  foreach ($name in @('test-forge-virtualbox-guest.ps1','invoke-forge-virtualbox-guest-capture.ps1','invoke-forge-virtualbox-host.ps1','invoke-forge-virtualbox-approved-processors.ps1')) {
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
  $runner=Join-Path $PSScriptRoot '../scripts/invoke-forge-virtualbox-approved-processors.ps1'
  $source=Get-Content -LiteralPath $runner -Raw
  if ($source -notmatch 'CONTROL_EXPLICITLY_APPROVED_FORGE_VM_EXECUTION' -or
      $source -notmatch 'ProcessorInputsVerified' -or
      $source -notmatch 'FORGE_EXECUTION_FAILED_NO_INSTANCE' -or
      $source -notmatch 'ForgeJobProcess' -or
      $source -notmatch 'File-Sha1') { throw 'Processor execution approval and output checks missing' }
  $report=Join-Path $dir 'no-report.json'
  & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $runner -PreflightJson (Join-Path $dir 'missing.json') -VmEvidenceJson (Join-Path $dir 'no-guest.json') -ControlApprovalJson (Join-Path $dir 'no-approval.json') -ReportNewJson $report | Out-Null
  if ($LASTEXITCODE -ne 2) { throw "Missing guest/approval must exit 2, got $LASTEXITCODE" }
  if (Test-Path -LiteralPath $report) {
    $body=Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
    if ($body.status -ne 'FORGE_EXECUTION_FAILED_NO_INSTANCE' -or $body.java_executed -ne $false) { throw 'Missing approval produced false Forge PASS' }
  }
  $global:LASTEXITCODE=0
  Write-Output 'VBOX_HOST_FAIL_CLOSED_AND_GUEST_POLICY_SOURCE_REGRESSION_PASS_NO_JAVA'
} finally {
  [IO.Directory]::Delete($dir,$true)
}
