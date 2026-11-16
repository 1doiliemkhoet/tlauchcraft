#requires -Version 5.1
<#
 P0 FORGE 1.16.5 — DO NOT CALL FROM PREFLIGHT.
 Run only during Control-approved combined modpack/LAN E2E in an offline
 Windows VirtualBox GUEST. No VirtualBox GuestControl transport is used.
 Uses existing ForgeJobProcess.cs (CREATE_SUSPENDED -> Job -> ResumeThread,
 KILL_ON_JOB_CLOSE) for four Java tool processes.
 Strictly no Minecraft world/profile write or instance publication.
 Output is a Rust ProcessorRunReport-compatible JSON, not a Play verdict.
#>
[CmdletBinding()]
param(
 [Parameter(Mandatory=$true)][string]$PreflightJson,
 [Parameter(Mandatory=$true)][string]$ControlApprovalJson,
 [Parameter(Mandatory=$true)][string]$NewExecutionReport,
 [Parameter(Mandatory=$true)][string]$JobHelperCs
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Deny([string]$message) { throw "FORGE_APPROVED_PROCESSOR_DENIED: $message" }
function Assert-NoReparse([string]$path) {
 $cursor=[IO.Path]::GetFullPath($path)
 while (-not [string]::IsNullOrWhiteSpace($cursor)) {
   if (([IO.File]::Exists($cursor) -or [IO.Directory]::Exists($cursor)) -and
       ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
     Deny "Symlink/reparse point forbidden: $cursor"
   }
   $parent=[IO.Path]::GetDirectoryName($cursor)
   if ($parent -eq $cursor) { break }
   $cursor=$parent
 }
}
function Sha1([string]$path) {
 Assert-NoReparse $path
 if (-not [IO.File]::Exists($path)) { Deny "File missing: $path" }
 (Get-FileHash -LiteralPath $path -Algorithm SHA1).Hash.ToLowerInvariant()
}
function Restricted([string]$path) {
 if (-not [IO.Path]::IsPathRooted($path)) { Deny 'Relative path forbidden' }
 $full=[IO.Path]::GetFullPath($path)
 if (@($full -split '[\\/]' | Where-Object { $_ -match '^(?i:worlds|saves|profiles|instances|\.minecraft)$' }).Count -gt 0) {
   Deny "Existing Minecraft data path forbidden: $full"
 }
 foreach($part in @($full,([IO.Path]::GetDirectoryName($full)))) {
  if ([IO.File]::Exists($part) -and ((Get-Item -LiteralPath $part -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { Deny "Reparse point forbidden: $part" }
 }
 $full
}
function StageChild([string]$stage,[string]$relative) {
 if ([string]::IsNullOrWhiteSpace($relative) -or [IO.Path]::IsPathRooted($relative) -or
     $relative -match '(?:^|[\\/])\.\.?(?:[\\/]|$)' -or $relative.Contains(':')) {
   Deny "Unsafe output relative path: $relative"
 }
 $path=[IO.Path]::GetFullPath((Join-Path $stage $relative))
 if (-not $path.StartsWith($stage.TrimEnd([char[]]@('\','/'))+'\',[StringComparison]::OrdinalIgnoreCase)) { Deny 'Output escaped stage' }
 $path
}
$javaStarted=$false
$stageCreated=$false
$verified=@()
$intermediates=@()
$exitCodes=@()
$logs=@()
$completed=0
$stage=$null
$failure=$null
try {
 $reportFile=Restricted $NewExecutionReport
 if (Test-Path -LiteralPath $reportFile) { Deny 'Execution report must be new, no overwrites' }
 $preflight=Get-Content -LiteralPath $PreflightJson -Raw | ConvertFrom-Json
 if ($preflight.import.processor_input_readiness -ne 'ProcessorInputsVerified' -or
     $preflight.import.state -ne 'Verified' -or
     $preflight.prepare.state -ne 'Verified' -or
     $preflight.prepare.processor_plan_count -ne 4 -or
     $preflight.prepare.execution_authorized -ne $false -or
     @($preflight.import.missing).Count -ne 0 -or
     $preflight.prepare.java_executed -ne $false) { Deny 'Four verified processor inputs required; plan is not an execution permit' }
 $plan=$preflight.prepare.processor_plan
 if ($null -eq $plan -or @($plan.processors).Count -ne 4 -or
     $plan.minecraft_version -ne '1.16.5') { Deny 'Unapproved Minecraft/Forge graph' }
 $approval=Get-Content -LiteralPath $ControlApprovalJson -Raw | ConvertFrom-Json
 $payload=[Text.Encoding]::UTF8.GetBytes(($plan|ConvertTo-Json -Depth 64 -Compress))
 $sha256=[Security.Cryptography.SHA256]::Create()
 try { $planHash=([BitConverter]::ToString($sha256.ComputeHash($payload))).Replace('-','').ToLowerInvariant() }
 finally { $sha256.Dispose() }
 if ($approval.schema -ne 'tl-control-forge-execution-approval/v1' -or
     $approval.status -ne 'CONTROL_APPROVED_COMBINED_E2E' -or
     ([string]$approval.plan_sha256).ToLowerInvariant() -cne $planHash -or
     $approval.java_major -ne 8 -or
     $approval.java_sha1 -notmatch '^[0-9a-fA-F]{40}$') { Deny 'No matching explicit Control approval for exact processor plan' }
 # Enforced environment: must be in VirtualBox guest, not on Control's host.
 $vbox=Join-Path $env:ProgramFiles 'Oracle\VirtualBox Guest Additions\VBoxControl.exe'
 if (-not [IO.File]::Exists($vbox)) { Deny 'Approved execution requires a verified isolated VirtualBox guest' }
 $routes=@(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction Stop)
 $nic=@(Get-NetAdapter -ErrorAction Stop | Where-Object { $_.Status -eq 'Up' -and $_.InterfaceDescription -notmatch 'Loopback' })
 $shares=@(Get-CimInstance Win32_LogicalDisk -ErrorAction Stop | Where-Object { $_.DriveType -eq 4 })
 $shareText=(& $vbox sharedfolder list 2>&1 | Out-String)
 $code=$LASTEXITCODE
 if ($routes.Count -gt 0 -or $nic.Count -gt 0 -or $shares.Count -gt 0 -or $code -ne 0 -or
     $shareText -notmatch '(?i)no shared folders|mappings \(0\)|0 mappings') { Deny 'Guest network or host shared-folder isolation not proven' }
 $stage=Restricted ([string]$plan.stage_root)
 if (-not $stage.StartsWith('C:\ForgeRuns\',[StringComparison]::OrdinalIgnoreCase) -or
     (Test-Path -LiteralPath $stage)) { Deny 'Require a fresh C:\ForgeRuns\... private stage' }
 $java=Restricted ([string]$plan.java)
 if ((Sha1 $java) -cne ([string]$approval.java_sha1).ToLowerInvariant()) { Deny 'Java executable SHA-1 differs from approved Java 8' }
 $helper=Restricted $JobHelperCs
 if (-not [IO.File]::Exists($helper)) { Deny 'Native JobObject helper missing' }
 $expected=@('net.minecraftforge:installertools:1.2.6','net.minecraftforge:jarsplitter:1.1.4','net.md-5:SpecialSource:1.8.5','net.minecraftforge:binarypatcher:1.0.12')
 $mains=@('net.minecraftforge.installertools.ConsoleTool','net.minecraftforge.jarsplitter.ConsoleTool','net.md_5.specialsource.SpecialSource','net.minecraftforge.binarypatcher.ConsoleTool')
 $counts=@(17,2,7,11)
 for($i=0;$i -lt 4;$i++) {
   $p=$plan.processors[$i]
   if ($p.executable.coordinate -cne $expected[$i] -or $p.main_class -cne $mains[$i] -or
       @($p.classpath).Count -ne $counts[$i]) { Deny "Forge step $i differs from reviewed four-processor graph" }
   foreach($a in @($p.executable)+@($p.classpath)) {
     $file=Restricted ([string]$a.path)
     if ($a.sha1 -notmatch '^[a-fA-F0-9]{40}$' -or (Sha1 $file) -cne ([string]$a.sha1).ToLowerInvariant()) { Deny "Invalid processor dependency hash: $($a.coordinate)" }
     if ($a.source -notmatch '^https://(maven\.minecraftforge\.net|repo\.maven\.apache\.org/maven2|libraries\.minecraft\.net)/') { Deny "Untrusted processor Maven origin: $($a.coordinate)" }
   }
   foreach($arg in @($p.arguments)) {
     if ([string]::IsNullOrEmpty($arg) -or $arg -match '[{}\[\]]') { Deny "Unexpanded processor argument at step $i" }
   }
   foreach($o in @($p.outputs)) { [void](StageChild $stage ([string]$o.relative_path)) }
   foreach($o in @($p.intermediate_outputs)) { [void](StageChild $stage ([string]$o)) }
 }
 [void][IO.Directory]::CreateDirectory($stage)
 $stageCreated=$true
 # The stage is freshly created. Prepare only declared output parent paths;
 # Forge tools may require directories to exist before writing mappings/JARs.
 foreach($step in @($plan.processors)) {
   foreach($entry in @($step.outputs)) {
     $target=StageChild $stage ([string]$entry.relative_path)
     [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
   }
   foreach($relative in @($step.intermediate_outputs)) {
     $target=StageChild $stage ([string]$relative)
     [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
   }
 }
 $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User
 $acl=New-Object Security.AccessControl.DirectorySecurity
 $acl.SetAccessRuleProtection($true,$false)
 $rule=New-Object Security.AccessControl.FileSystemAccessRule($owner,[Security.AccessControl.FileSystemRights]::FullControl,([Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)
 $acl.AddAccessRule($rule)
 Set-Acl -LiteralPath $stage -AclObject $acl
 if (-not (Get-Acl -LiteralPath $stage).AreAccessRulesProtected) { Deny 'Stage ACL protection failed' }
 [IO.File]::WriteAllText((Join-Path $stage '.tl-loader-stage'),'tl-loader-forge-v1',(New-Object Text.UTF8Encoding($false)))
 Add-Type -Path $helper -ErrorAction Stop
 for($i=0;$i -lt 4;$i++) {
   $p=$plan.processors[$i]
   $classpath=(@([string]$p.executable.path)+@($p.classpath|ForEach-Object { [string]$_.path })) -join ';'
   $arguments=[string[]](@('-cp',$classpath,[string]$p.main_class)+@($p.arguments|ForEach-Object { [string]$_ }))
   $commandLog=Join-Path $stage "processor-$i.execution.json"
   $digestLog=Join-Path $stage "processor-$i.outputs.json"
   [IO.File]::WriteAllText($commandLog,(@{processor=$p.executable.coordinate;index=$i;started_utc=[DateTime]::UtcNow.ToString('o');status='STARTED';java_sha1=$approval.java_sha1}|ConvertTo-Json),(New-Object Text.UTF8Encoding($false)))
   $logs+=@($commandLog,$digestLog)
   $javaStarted=$true
   $exit=[ForgeJobProcess]::Run($java,$arguments,$stage,([int]$plan.timeout_seconds*1000))
   $exitCodes+=([int]$exit)
   if ($exit -ne 0) { Deny "Forge processor $i failed with exit $exit" }
   $outEvidence=@()
   foreach($o in @($p.outputs)) {
     $path=StageChild $stage ([string]$o.relative_path)
     $actual=Sha1 $path
     if ($actual -cne ([string]$o.sha1).ToLowerInvariant()) { Deny "Forge output hash mismatch at step $i" }
     $verified+=,$path
     $outEvidence+=@{relative_path=$o.relative_path;sha1=$actual;size_bytes=([IO.FileInfo]$path).Length}
   }
   foreach($relative in @($p.intermediate_outputs)) {
     $path=StageChild $stage ([string]$relative)
     $size=([IO.FileInfo]$path).Length
     if ($size -le 0) { Deny "Forge intermediate output empty at step $i" }
     $digest=Sha1 $path
     $intermediates+=@{producer_index=$i;relative_path=$relative;observed_sha1=$digest;size_bytes=$size}
     $outEvidence+=@{relative_path=$relative;observed_sha1=$digest;size_bytes=$size;source='observed-not-official'}
   }
   [IO.File]::WriteAllText($digestLog,(@{processor=$p.executable.coordinate;outputs=$outEvidence;exit_code=$exit;finished_utc=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json -Depth 8),(New-Object Text.UTF8Encoding($false)))
   $completed++
 }
} catch { $failure=$_.Exception.Message }
# Rust ProcessorRunReport: a failure never becomes a success because a plan existed.
$body=[ordered]@{
 outcome=$(if($null -eq $failure -and $completed -eq 4){'Verified'}else{'Failed'})
 completed=$completed
 exit_codes=@($exitCodes)
 log_paths=@($logs)
 verified_outputs=@($verified)
 intermediate_evidence=@($intermediates)
 error=$failure
}
$finalReport=Restricted $NewExecutionReport
if (Test-Path -LiteralPath $finalReport) { Deny 'Execution report already exists' }
$parent=[IO.Path]::GetDirectoryName($finalReport)
if (-not [IO.Directory]::Exists($parent)) { Deny 'Execution report parent directory must already exist' }
[IO.File]::WriteAllText($finalReport,($body|ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding($false)))
$body|ConvertTo-Json -Depth 12
if ($body.outcome -ne 'Verified') { exit 2 }
