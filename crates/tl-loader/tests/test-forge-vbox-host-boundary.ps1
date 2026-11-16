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
  if ($hostSource -notmatch [regex]::Escape('foreach ($file in @($source,$launcher,$job,$hostPath))') -or
      $hostSource -notmatch [regex]::Escape('guest-launch-receipt.json') -or
      $hostSource -notmatch [regex]::Escape('guest-preflight-stderr.txt') -or
      $hostSource -notmatch [regex]::Escape('guest-preflight-stdout.txt') -or
      $hostSource -notmatch [regex]::Escape('GUEST_CHILD_PREFLIGHT_COMPLETED') -or
      $hostSource -notmatch [regex]::Escape('("--target-directory=$hostPathNew"),$guestPath') -or
      $hostSource -notmatch [regex]::Escape('VBox (@(''guestcontrol'',$Vm,''stat'')+$common+@($guestPath))')) {
    throw 'Guest capture receipt, diagnostic files, strict verdict or full-file copyfrom missing'
  }
  # No dependency on Guest Additions stdout/stderr wait flags: launch must
  # return independently and poll only for the child-produced receipt.
  if ($hostSource -notmatch [regex]::Escape('@(''guestcontrol'',$Vm,''start'')') -or
      $hostSource -notmatch [regex]::Escape('GuestFileExists $guestReceiptPath') -or
      $hostSource -notmatch [regex]::Escape('VBOX_GUEST_RECEIPT_NOT_FOUND') -or
      $hostSource -notmatch [regex]::Escape('if (-not $receiptPresent) { Fail') -or
      $hostSource -match [regex]::Escape('@(''guestcontrol'',$Vm,''run'')')) {
    throw 'Guest audit must use detached start and bounded receipt polling, not unsupported stdout waiting'
  }
  # The real Windows PowerShell host previously hung (WER AppHangB1).
  # The Start-Process -PassThru wrapper can also return null ExitCode when
  # streams are redirected. Use .NET Process with async pipe readers and
  # bounded wait, never synchronous native/pipeline guestcontrol start.
  if ($hostSource -notmatch [regex]::Escape('$startProcess=New-Object System.Diagnostics.Process') -or
      $hostSource -notmatch [regex]::Escape('$startInfo.RedirectStandardOutput=$true') -or
      $hostSource -notmatch [regex]::Escape('$startInfo.RedirectStandardError=$true') -or
      $hostSource -notmatch [regex]::Escape('$startProcess.StandardOutput.ReadToEndAsync()') -or
      $hostSource -notmatch [regex]::Escape('$startProcess.WaitForExit(35000)') -or
      $hostSource -notmatch [regex]::Escape('$startProcess.Kill()') -or
      $hostSource -notmatch [regex]::Escape('$startExitCode=[int]$startProcess.ExitCode') -or
      $hostSource -notmatch [regex]::Escape('VBOX_GUEST_START_TIMEOUT') -or
      $hostSource -notmatch [regex]::Escape('guest-start-stderr.txt') -or
      $hostSource -match [regex]::Escape('Start-Process -FilePath $VBoxManage') -or
      $hostSource -match [regex]::Escape('try { $guestRunOutput=@(VBox $run) }')) {
    throw 'Guest start process handle, native exit-code capture, hard timeout or async output missing'
  }
  # Real process/redirect/exit-code smoke: this never accesses the VM/Java.
  $probe=New-Object System.Diagnostics.Process
  $probeInfo=New-Object System.Diagnostics.ProcessStartInfo
  $probeInfo.FileName=Join-Path $env:SystemRoot 'System32\cmd.exe'
  $probeInfo.Arguments='/d /c exit 7'
  $probeInfo.UseShellExecute=$false
  $probeInfo.CreateNoWindow=$true
  $probeInfo.RedirectStandardOutput=$true
  $probeInfo.RedirectStandardError=$true
  $probe.StartInfo=$probeInfo
  try {
    if (-not $probe.Start()) { throw 'ProcessStartInfo probe did not start' }
    $probeStdout=$probe.StandardOutput.ReadToEndAsync()
    $probeStderr=$probe.StandardError.ReadToEndAsync()
    if (-not $probe.WaitForExit(5000)) {
      $probe.Kill()
      throw 'ProcessStartInfo probe timed out'
    }
    if ([int]$probe.ExitCode -ne 7) { throw "ProcessStartInfo lost exit code: $($probe.ExitCode)" }
  } finally { $probe.Dispose() }
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
