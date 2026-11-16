#requires -Version 5.1
# Windows P0 regression. No Java, Forge executable, network or real VM is run.
# Validates parser and a negative Control-approval gate through a child shell.
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$root=Join-Path ([IO.Path]::GetTempPath()) ('tl03-p0-negative-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
try {
 $scripts=@(
  'run-forge-approved-e2e-processors.ps1',
  'stage-forge-installer-artifacts.ps1'
 )
 foreach($name in $scripts) {
   $path=Join-Path $PSScriptRoot "../scripts/$name"
   $tokens=$null; $errors=$null
   [void][System.Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
   if (@($errors).Count -ne 0) {
     throw "Invalid PowerShell 5.1 syntax in $name : $($errors | Out-String)"
   }
 }
 $runner=Join-Path $PSScriptRoot '../scripts/run-forge-approved-e2e-processors.ps1'
 $output=Join-Path $root 'forged-execution.json'
 & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $runner -PreflightJson (Join-Path $root 'missing-preflight.json') -ControlApprovalJson (Join-Path $root 'missing-approval.json') -NewExecutionReport $output -JobHelperCs (Join-Path $root 'missing-JobProcess.cs') | Out-Null
 if ($LASTEXITCODE -ne 2) { throw "Missing approval must exit 2, got $LASTEXITCODE" }
 if (-not (Test-Path -LiteralPath $output)) { throw 'Execution denial must emit JSON evidence' }
 $failed=Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
 if ($failed.outcome -ne 'Failed' -or $failed.completed -ne 0 -or
     @($failed.verified_outputs).Count -ne 0 -or $null -eq $failed.error) {
   throw 'Missing Control approval gave false EXECUTION_VERIFIED'
 }
 $global:LASTEXITCODE=0
 Write-Output 'FORGE_P0_NEGATIVE_EXECUTION_APPROVAL_AND_POWERSHELL_PARSE_PASS_NO_JAVA'
} finally {
 [IO.Directory]::Delete($root,$true)
}
