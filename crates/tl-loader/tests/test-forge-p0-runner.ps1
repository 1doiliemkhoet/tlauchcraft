#requires -Version 5.1
# Production P0 scripts must parse on Windows and fail before any Java without
# real Control approval. This test never starts Forge or alters Minecraft.
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$root=Join-Path ([IO.Path]::GetTempPath()) ("forge-p0-negative-"+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
try {
 foreach($file in @('invoke-forge-p0-approved-chain.ps1','extract-forge-embedded-runtime.ps1')) {
  $path=Join-Path $PSScriptRoot "../scripts/$file"
  $tokens=$null;$errors=$null
  [void][Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
  if (@($errors).Count -ne 0) { throw "$file syntax errors: $($errors|Out-String)" }
 }
 $runner=Join-Path $PSScriptRoot '../scripts/invoke-forge-p0-approved-chain.ps1'
 $report=Join-Path $root 'failed-execution.json'
 & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $runner -PreflightJson (Join-Path $root 'missing-preflight.json') -IsolationEvidenceJson (Join-Path $root 'missing-approval.json') -ControlApprovalJson (Join-Path $root 'missing-control.json') -ReportNewJson $report -JobSource (Join-Path $root 'missing-helper.cs') | Out-Null
 if ($LASTEXITCODE -ne 2) { throw "Missing approval must exit 2, got $LASTEXITCODE" }
 if (-not (Test-Path -LiteralPath $report)) { throw 'Missing approval did not produce FAILED report' }
 $result=Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
 if ($result.status -ne 'FORGE_EXECUTION_FAILED_NO_INSTANCE' -or
     $result.outcome -ne 'Failed' -or $result.java_executed -ne $false -or
     $result.instance_ready -ne $false -or $result.completed -ne 0) {
   throw 'Forge execution false PASS on absent approval'
 }
 $global:LASTEXITCODE=0
 Write-Output 'FORGE_P0_RUNNER_AND_EMBEDDED_EXTRACTOR_PARSE_AND_FAIL_CLOSED_PASS_NO_JAVA'
} finally { [IO.Directory]::Delete($root,$true) }
