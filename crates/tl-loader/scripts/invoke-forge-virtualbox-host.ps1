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
$launcher=Join-Path $PSScriptRoot 'invoke-forge-virtualbox-guest-capture.ps1'
$job=Join-Path $PSScriptRoot 'ForgeJobProcess.cs'
foreach($file in @($source,$launcher,$job)) { if (-not [IO.File]::Exists($file)) { Fail "Missing repo harness: $file" } }
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
    # Capture VBoxManage stdout AND stderr without PowerShell 5.1 prematurely
    # converting native stderr to a terminating ErrorRecord. Never print
    # commandArgs: those include the restricted temporary passwordfile path.
    $operation=[string]$commandArgs[2]
    $previous=$ErrorActionPreference
    try {
      $ErrorActionPreference='Continue'
      $output=@(& $VBoxManage @commandArgs 2>&1 | ForEach-Object { [string]$_ })
      $code=$LASTEXITCODE
    } finally { $ErrorActionPreference=$previous }
    if ($code -ne 0) { Fail "GuestControl $operation failed (exit $code): $($output -join ' | ')" }
    $output | ForEach-Object { Write-Output $_ }
  }
  VBox (@('guestcontrol',$Vm,'mkdir')+$common+@('--parents',$guest)) | Out-Null
  # A successful mkdir command is not proof that guest file permissions work.
  VBox (@('guestcontrol',$Vm,'stat')+$common+@($guest)) | Out-Null
  foreach ($file in @($source,$launcher,$job,$hostPath)) {
    if (-not [IO.File]::Exists($file)) { Fail "Host input absent: $([IO.Path]::GetFileName($file))" }
    Write-Output "VBOX_COPY_TO: $([IO.Path]::GetFileName($file))"
    # VBoxManage's CopyToGuest forwards an existing destination directory to
    # the file-copy API without CopyIntoExisting for file sources; this fails
    # "destination ... already exists and is a directory" even without -n.
    # Pass a *new full guest filename*, not the pre-existing directory.
    # The parent is a fresh per-run UUID directory created above.
    $guestFile=Join-Path $guest ([IO.Path]::GetFileName($file))
    VBox (@('guestcontrol',$Vm,'copyto')+$common+@('--verbose','--no-replace',("--target-directory=$guestFile"),$file)) | Out-Null
    VBox (@('guestcontrol',$Vm,'stat')+$common+@($guestFile)) | Out-Null
  }
  $command='C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
  # Guest Additions 7.2 may not support the stdout/stderr wait flags.
  # Oracle documents 'start' as returning once the process has started,
  # independently of stream capture. The guest-side launcher writes an
  # exclusive receipt after the child preflight exits; never trust 'start'
  # as the actual verdict.
  $run=@('guestcontrol',$Vm,'start')+$common+@("--exe=$command",'--timeout=25000','--','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',"$guest\invoke-forge-virtualbox-guest-capture.ps1")
  # Host Windows Error Reporting recorded powershell.exe AppHangB1 around
  # guestcontrol start. Do not call this native command synchronously again.
  # This child-only watchdog protects the host without touching the VM settings.
  $guestRunLog=Join-Path $out 'guest-run-output.txt'
  $guestRunErr=Join-Path $out 'guest-start-stderr.txt'
  $guestError=$null
  Write-Output 'VBOX_GUEST_START: starting guest-local no-Java capture (bounded)'
  $startArgs=($run | ForEach-Object {
    # Quote each argument when needed; never print credentials or arg list.
    $item=[string]$_
    if ($item -match '[\s"]') { '"' + $item.Replace('"','\"') + '"' } else { $item }
  }) -join ' '
  # Start-Process -PassThru with redirected streams may lose the process
  # handle / report a null ExitCode on Windows. Use System.Diagnostics.Process
  # directly so a true exit code is never silently mistaken for a failure.
  # Read both streams asynchronously to avoid pipe-buffer deadlock.
  $startProcess=New-Object System.Diagnostics.Process
  $startInfo=New-Object System.Diagnostics.ProcessStartInfo
  $startInfo.FileName=$VBoxManage
  $startInfo.Arguments=$startArgs
  $startInfo.UseShellExecute=$false
  $startInfo.CreateNoWindow=$true
  $startInfo.RedirectStandardOutput=$true
  $startInfo.RedirectStandardError=$true
  $startProcess.StartInfo=$startInfo
  $stdoutTask=$null
  $stderrTask=$null
  $startExitCode=$null
  $processFinished=$false
  try {
    if (-not $startProcess.Start()) { Fail 'VBOX_GUEST_START_FAILED: could not start VBoxManage child' }
    $stdoutTask=$startProcess.StandardOutput.ReadToEndAsync()
    $stderrTask=$startProcess.StandardError.ReadToEndAsync()
    $processFinished=$startProcess.WaitForExit(35000)
    if (-not $processFinished) {
      try { $startProcess.Kill() } catch {}
      $processFinished=$startProcess.WaitForExit(5000)
      Fail 'VBOX_GUEST_START_TIMEOUT: VBoxManage exceeded 35 seconds; guest status UNKNOWN; no Forge execution authorized'
    }
    $startExitCode=[int]$startProcess.ExitCode
  } finally {
    # A child killed by the watchdog may never close both streams: do not
    # wait indefinitely to read their tasks if termination was unsuccessful.
    if ($null -ne $stdoutTask -and $stdoutTask.IsCompleted -and -not $stdoutTask.IsFaulted) {
      [IO.File]::WriteAllText($guestRunLog,$stdoutTask.Result,(New-Object Text.UTF8Encoding($false)))
    } elseif (-not [IO.File]::Exists($guestRunLog)) {
      [IO.File]::WriteAllText($guestRunLog,'',(New-Object Text.UTF8Encoding($false)))
    }
    if ($null -ne $stderrTask -and $stderrTask.IsCompleted -and -not $stderrTask.IsFaulted) {
      [IO.File]::WriteAllText($guestRunErr,$stderrTask.Result,(New-Object Text.UTF8Encoding($false)))
    } elseif (-not [IO.File]::Exists($guestRunErr)) {
      [IO.File]::WriteAllText($guestRunErr,'',(New-Object Text.UTF8Encoding($false)))
    }
    $startProcess.Dispose()
  }
  if ($null -eq $startExitCode) { Fail 'VBOX_GUEST_START_UNKNOWN_EXIT: child exit code unavailable; no Forge execution authorized' }
  if ($startExitCode -ne 0) {
    $detail=(Get-Content -LiteralPath $guestRunErr -Tail 15 -ErrorAction SilentlyContinue) -join ' | '
    Fail "VBOX_GUEST_START_FAILED: VBoxManage exit $startExitCode; stderr: $detail"
  }
  Write-Output 'VBOX_GUEST_START_RETURNED: VBoxManage exit 0; waiting for guest receipt'
  $guestReceiptPath=Join-Path $guest 'guest-launch-receipt.json'
  function GuestFileExists([string]$path) {
    # Absence is expected while the detached guest child is working.
    # Never print passwordfile path, user secrets or argument array.
    $previous=$ErrorActionPreference
    try {
      $ErrorActionPreference='Continue'
      $null=@(& $VBoxManage guestcontrol $Vm stat @common $path 2>&1)
      return ($LASTEXITCODE -eq 0)
    } finally { $ErrorActionPreference=$previous }
  }
  $receiptPresent=$false
  foreach($attempt in 1..45) {
    if (GuestFileExists $guestReceiptPath) { $receiptPresent=$true;break }
    Start-Sleep -Milliseconds 1000
  }
  if (-not $receiptPresent) {
    # Do not authorize processors on ambiguous/no-result guest state.
    # The process list is diagnostic only, not proof of completion.
    Write-Output 'VBOX_GUEST_RECEIPT_NOT_FOUND: guest start returned but receipt absent'
  }
  # Recheck isolation even when guest script failed or produced no JSON.
  $after=HostSnapshot
  [IO.File]::WriteAllText((Join-Path $out 'host-after.json'),($after|ConvertTo-Json -Depth 5),(New-Object Text.UTF8Encoding($false)))
  function Copy-GuestAuditFile([string]$guestBasename,[string]$hostBasename) {
    $guestPath=Join-Path $guest $guestBasename
    $hostPathNew=Join-Path $out $hostBasename
    VBox (@('guestcontrol',$Vm,'stat')+$common+@($guestPath)) | Out-Null
    # Use a NEW full target filename, not the existing output directory.
    VBox (@('guestcontrol',$Vm,'copyfrom')+$common+@('--verbose','--no-replace',("--target-directory=$hostPathNew"),$guestPath)) | Out-Null
    if (-not [IO.File]::Exists($hostPathNew)) { Fail "Copyfrom did not create: $hostBasename" }
  }
  if (-not $receiptPresent) { Fail "Guest launch receipt not created within bounded polling window. See $guestRunLog; VM preflight NOT_VERIFIED" }
  try { Copy-GuestAuditFile 'guest-launch-receipt.json' 'guest-launch-receipt.json' }
  catch {
    Fail "Guest launch receipt exists but retrieval failed: $($_.Exception.Message); start output: $guestRunLog"
  }
  $receipt=Get-Content -LiteralPath (Join-Path $out 'guest-launch-receipt.json') -Raw | ConvertFrom-Json
  if ($receipt.schema -ne 'tl-forge-vbox-guest-launch/v1') { Fail 'Unexpected guest launcher receipt schema' }
  if ($receipt.stdout_exists -eq $true) {
    try { Copy-GuestAuditFile 'guest-preflight-stdout.txt' 'guest-preflight-stdout.txt' }
    catch { Fail "Could not retrieve guest stdout: $($_.Exception.Message)" }
  }
  if ($receipt.stderr_exists -eq $true) {
    try { Copy-GuestAuditFile 'guest-preflight-stderr.txt' 'guest-preflight-stderr.txt' }
    catch { Fail "Could not retrieve guest stderr: $($_.Exception.Message)" }
  }
  $guestReport=Join-Path $out 'virtualbox-preflight.json'
  if ($receipt.evidence_exists -eq $true) {
    try { Copy-GuestAuditFile 'virtualbox-preflight.json' 'virtualbox-preflight.json' }
    catch { Fail "Guest reported evidence exists, but transfer failed: $($_.Exception.Message)" }
  }
  if ($guestError -or $receipt.status -ne 'GUEST_CHILD_PREFLIGHT_COMPLETED' -or
      $receipt.child_exit_code -ne 0) {
    Fail "Guest audit did not finish successfully; child_exit=$($receipt.child_exit_code); reason=$($receipt.error); guest error=$guestError. See $out for guest-preflight-stderr.txt and receipt."
  }
  if (-not [IO.File]::Exists($guestReport)) { Fail "Guest child exited 0 but required JSON evidence missing. See $out" }
  $body=Get-Content -LiteralPath $guestReport -Raw | ConvertFrom-Json
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
