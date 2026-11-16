#requires -Version 5.1
<#
  P0 SINGLE E2E ONLY. Never called by installer, UI, CI or preflight.
  Runs in an independently Control-approved OFFLINE ISOLATED WINDOWS VM, not
  on the user's Windows host. Does not require VirtualBox GuestControl.
  Requires approved plan SHA256 and verified local immutable Java/Maven bytes.
  No publish, world/profile writes or network usage are supported.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory=$true)][string]$PreflightJson,
  [Parameter(Mandatory=$true)][string]$IsolationEvidenceJson,
  [Parameter(Mandatory=$true)][string]$ControlApprovalJson,
  [Parameter(Mandatory=$true)][string]$ReportNewJson,
  [string]$JobSource='C:\ForgeP0\ForgeJobProcess.cs'
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$journal=New-Object System.Collections.ArrayList
function Stop-Denied([string]$why) { throw "FORGE_P0_EXECUTION_DENIED: $why" }
function File-Sha1([string]$path) {
  if (-not [IO.File]::Exists($path)) { Stop-Denied "Missing approved artifact: $path" }
  $cursor=[IO.Path]::GetFullPath($path)
  while (-not [string]::IsNullOrWhiteSpace($cursor)) {
    if (([IO.File]::Exists($cursor) -or [IO.Directory]::Exists($cursor)) -and
        ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
      Stop-Denied "Symlink/reparse point in approved artifact: $cursor"
    }
    $parent=[IO.Path]::GetDirectoryName($cursor)
    if ($parent -eq $cursor) { break }
    $cursor=$parent
  }
  (Get-FileHash -LiteralPath $path -Algorithm SHA1).Hash.ToLowerInvariant()
}
function Check-GuestPath([string]$path,[string]$label) {
  if (-not [IO.Path]::IsPathRooted($path)) { Stop-Denied "Nonabsolute $label" }
  $full=[IO.Path]::GetFullPath($path)
  if (-not $full.StartsWith('C:\ForgeP0\',[StringComparison]::OrdinalIgnoreCase) -or
      @($full -split '[\\/]' | Where-Object { $_ -match '^(?i:saves|worlds|instances|profiles|\.minecraft)$' }).Count -ne 0) {
    Stop-Denied "Nonisolated or Minecraft-data $label path"
  }
  $full
}
function Save-Report([string]$status,[string]$failure) {
  $data=[ordered]@{
    schema='tl-forge-p0-processors/v1'
    status=$status
    java_executed=$javaStarted
    processors_verified=($status -eq 'FORGE_EXECUTION_VERIFIED_NOT_INSTALLED')
    instance_ready=$false
    error=$failure
    steps=@($journal.ToArray())
    outcome=$(if($status -eq 'FORGE_EXECUTION_VERIFIED_NOT_INSTALLED'){'Verified'}else{'Failed'})
    completed=@($journal.ToArray() | Where-Object { $_.status -eq 'OUTPUTS_VERIFIED' }).Count
    exit_codes=@($journal.ToArray() | ForEach-Object { $_.exit_code })
    log_paths=@()
    verified_outputs=@($journal.ToArray() | ForEach-Object { $_.output_hashes } | ForEach-Object { $_.path })
    intermediate_evidence=@($journal.ToArray() | ForEach-Object { $_.intermediates } | ForEach-Object { [ordered]@{producer_index=$_.producer_index;relative_path=$_.relative_path;observed_sha1=$_.observed_sha1;size_bytes=$_.size} })
  }
  $target=[IO.Path]::GetFullPath($ReportNewJson)
  if ([IO.File]::Exists($target)) { throw "Evidence report already exists: $target" }
  [IO.File]::WriteAllText($target,($data|ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding($false)))
  $data | ConvertTo-Json -Depth 12
}
$javaStarted=$false
try {
  $vm=Get-Content -LiteralPath $IsolationEvidenceJson -Raw | ConvertFrom-Json
  $model=(Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).Model
  if ($model -notmatch '(?i)VirtualBox|VMware|Virtual Machine|KVM|QEMU' -or
      $vm.status -ne 'CONTROL_APPROVED_RESTRICTED_WINDOWS_VM_E2E' -or
      $vm.java_executed -ne $false -or
      @($vm.checks | Where-Object { $_.state -ne 'PASS' }).Count -ne 0) {
    Stop-Denied 'Only a Control-approved isolated Windows guest may run Forge; host and synthetic boolean permits rejected'
  }
  $report=Get-Content -LiteralPath $PreflightJson -Raw | ConvertFrom-Json
  if ($report.status -ne 'READ_ONLY_NO_JAVA_NOT_INSTALLED' -or
      $report.import.processor_input_readiness -ne 'ProcessorInputsVerified' -or
      $report.prepare.state -ne 'Verified' -or $report.prepare.processor_plan_count -ne 4 -or
      $report.prepare.execution_authorized -ne $false -or
      $report.prepare.instance_ready -ne $false -or
      @($report.import.missing).Count -ne 0) {
    Stop-Denied 'Incomplete input evidence or inconsistent 4-processor preflight'
  }
  $plan=$report.prepare.processor_plan
  if ($null -eq $plan -or @($plan.processors).Count -ne 4 -or $plan.minecraft_version -ne '1.16.5') {
    Stop-Denied 'Four real Forge processors absent'
  }
  $approval=Get-Content -LiteralPath $ControlApprovalJson -Raw | ConvertFrom-Json
  $planBytes=[Text.Encoding]::UTF8.GetBytes(($plan|ConvertTo-Json -Depth 64 -Compress))
  $digest=[Security.Cryptography.SHA256]::Create()
  try { $planHash=([BitConverter]::ToString($digest.ComputeHash($planBytes))).Replace('-','').ToLowerInvariant() }
  finally { $digest.Dispose() }
  if ($approval.status -ne 'CONTROL_EXPLICITLY_APPROVED_FORGE_P0_E2E' -or
      $approval.plan_sha256 -cne $planHash -or
      $approval.java_sha1 -notmatch '^[0-9a-fA-F]{40}$') {
    Stop-Denied 'No matching exact Control approval for these planned bytes'
  }
  $stage=Check-GuestPath ([string]$plan.stage_root) 'new stage'
  if (Test-Path -LiteralPath $stage) { Stop-Denied 'Stage must be fresh and not overwrite existing data' }
  $java=Check-GuestPath ([string]$plan.java) 'Java8'
  if ((File-Sha1 $java) -ne $approval.java_sha1.ToLowerInvariant()) { Stop-Denied 'Java8 executable digest differs from approved bytes' }
  if (-not (Test-Path -LiteralPath $JobSource -PathType Leaf)) { Stop-Denied 'Missing approved native JobObject helper' }
  $expect=@('net.minecraftforge:installertools:1.2.6','net.minecraftforge:jarsplitter:1.1.4','net.md-5:SpecialSource:1.8.5','net.minecraftforge:binarypatcher:1.0.12')
  foreach($i in 0..3) {
    $step=$plan.processors[$i]
    if ($step.executable.coordinate -cne $expect[$i]) { Stop-Denied "Wrong Forge processor order $i" }
    foreach($artifact in @($step.executable)+@($step.classpath)) {
      $path=Check-GuestPath ([string]$artifact.path) 'processor Maven artifact'
      if ($artifact.sha1 -notmatch '^[0-9a-fA-F]{40}$' -or
          (File-Sha1 $path) -ne $artifact.sha1.ToLowerInvariant()) { Stop-Denied "Processor/classpath SHA-1 mismatch: $($artifact.coordinate)" }
      if ($artifact.source -notmatch '^https://(maven\.minecraftforge\.net|maven\.forgecdn\.net|maven\.forge\.net|repo\.maven\.apache\.org|libraries\.minecraft\.net)/') { Stop-Denied 'Unapproved processor Maven source' }
    }
    foreach($arg in @($step.arguments)) {
      if ($arg -match '(?i)(?:[A-Z]:\\|/)(?:[^ ]*\.minecraft|[^ ]*\\saves\\|[^ ]*\\profiles\\)' ) { Stop-Denied 'Minecraft data path in processor arguments' }
    }
  }
  $route=@(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction Stop)
  if ($route.Count -ne 0 -or @(Get-NetAdapter -ErrorAction Stop | Where-Object { $_.Status -eq 'Up' }).Count -ne 0) { Stop-Denied 'VM networking was re-enabled after preflight' }
  # Validate/create only official declared output directories inside the fresh stage.
  # Forge tools do not necessarily create parent directories themselves.
  foreach($step in @($plan.processors)) {
    foreach($relative in @($step.outputs | ForEach-Object { [string]$_.relative_path }) + @($step.intermediate_outputs | ForEach-Object { [string]$_ })) {
      if ([string]::IsNullOrWhiteSpace($relative) -or [IO.Path]::IsPathRooted($relative) -or
          $relative.Contains(':') -or $relative -match '(^|[\\/])\.\.?([\\/]|$)') {
        Stop-Denied "Unsafe staged Forge output path: $relative"
      }
      $target=[IO.Path]::GetFullPath((Join-Path $stage $relative))
      if (-not $target.StartsWith($stage.TrimEnd([char[]]@('\','/'))+'\',[StringComparison]::OrdinalIgnoreCase)) {
        Stop-Denied 'Forge output escaped the approved private stage'
      }
    }
  }
  [void][IO.Directory]::CreateDirectory($stage)
  foreach($step in @($plan.processors)) {
    foreach($relative in @($step.outputs | ForEach-Object { [string]$_.relative_path }) + @($step.intermediate_outputs | ForEach-Object { [string]$_ })) {
      $target=[IO.Path]::GetFullPath((Join-Path $stage $relative))
      [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
    }
  }
  $who=[Security.Principal.WindowsIdentity]::GetCurrent().User
  $acl=New-Object Security.AccessControl.DirectorySecurity
  $acl.SetAccessRuleProtection($true,$false)
  $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($who,[Security.AccessControl.FileSystemRights]::FullControl,([Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)))
  Set-Acl -LiteralPath $stage -AclObject $acl -ErrorAction Stop
  if (-not (Get-Acl -LiteralPath $stage).AreAccessRulesProtected) { Stop-Denied 'Stage ACL protection failed' }
  Add-Type -Path $JobSource -ErrorAction Stop
  foreach($index in 0..3) {
    $step=$plan.processors[$index]
    $classpath=(@([string]$step.executable.path)+@($step.classpath | ForEach-Object { [string]$_.path })) -join ';'
    $arguments=@('-cp',$classpath,[string]$step.main_class)+@($step.arguments | ForEach-Object { [string]$_ })
    $record=[ordered]@{index=$index;tool=$step.executable.coordinate;status='STARTED';exit_code=$null;output_hashes=@();intermediates=@()}
    [void]$journal.Add($record)
    $javaStarted=$true
    $exit=[ForgeJobProcess]::Run($java,([string[]]$arguments),$stage,([int]$plan.timeout_seconds*1000))
    if ($exit -ne 0) { Stop-Denied "Processor $index Java exit=$exit" }
    foreach($expected in @($step.outputs)) {
      $path=Check-GuestPath (Join-Path $stage ([string]$expected.relative_path)) 'declared output'
      if (-not $path.StartsWith($stage.TrimEnd([char[]]@('\','/'))+'\',[StringComparison]::OrdinalIgnoreCase)) { Stop-Denied 'Output escaped private stage' }
      $observed=File-Sha1 $path
      if ($observed -ne ([string]$expected.sha1).ToLowerInvariant()) { Stop-Denied "Output checksum mismatch: $path" }
      $record.output_hashes+=@{path=$path;sha1=$observed}
    }
    foreach($relative in @($step.intermediate_outputs)) {
      $path=Check-GuestPath (Join-Path $stage ([string]$relative)) 'intermediate output'
      if (-not $path.StartsWith($stage.TrimEnd([char[]]@('\','/'))+'\',[StringComparison]::OrdinalIgnoreCase)) { Stop-Denied 'Intermediate escaped stage' }
      $len=([IO.FileInfo]$path).Length
      if ($len -le 0) { Stop-Denied "Empty intermediate output: $path" }
      $record.intermediates+=@{producer_index=$index;relative_path=[string]$relative;path=$path;size=$len;observed_sha1=(File-Sha1 $path)}
    }
    $record.exit_code=[int]$exit
    $record.status='OUTPUTS_VERIFIED'
  }
  Save-Report 'FORGE_EXECUTION_VERIFIED_NOT_INSTALLED' $null
} catch {
  $errorText=$_.Exception.Message
  try { Save-Report 'FORGE_EXECUTION_FAILED_NO_INSTANCE' $errorText } catch { Write-Error -ErrorAction Continue "Unable to save failure evidence: $($_.Exception.Message)" }
  exit 2
}
