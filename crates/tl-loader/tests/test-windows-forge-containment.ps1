#requires -Version 5.1
# Safe Windows-only OS containment tests. No Forge, Minecraft, or Java executes.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$source=Join-Path $PSScriptRoot '../scripts/ForgeJobProcess.cs'
Add-Type -Path $source -ErrorAction Stop
$root=Join-Path ([IO.Path]::GetTempPath()) ('tl03-job-test-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
try {
  # Actual Windows CreateProcessW(SUSPENDED) + AssignProcessToJobObject +
  # ResumeThread + Wait + KILL_ON_JOB_CLOSE against a benign OS command.
  $cmd=Join-Path $env:WINDIR 'System32\cmd.exe'
  $result=[ForgeJobProcess]::Run($cmd,([string[]]@('/d','/c','exit','0')),$root,10000)
  if ($result -ne 0) { throw "JobObject process test returned $result" }
  $input=Join-Path $root 'inputs'
  [void][IO.Directory]::CreateDirectory($input)
  $output=Join-Path $root 'new-output'
  $wsb=Join-Path $root 'forge-test.wsb'
  & (Join-Path $PSScriptRoot '../scripts/new-forge-windows-sandbox.ps1') -InputDirectory $input -FreshOutputDirectory $output -SandboxFile $wsb | Out-Null
  if (-not (Test-Path -LiteralPath $wsb -PathType Leaf)) { throw 'Sandbox config missing' }
  [xml]$xml=Get-Content -LiteralPath $wsb -Raw
  if ($xml.Configuration.Networking -ne 'Disable' -or
      $xml.Configuration.ClipboardRedirection -ne 'Disable' -or
      $xml.Configuration.VGpu -ne 'Disable' -or
      $xml.Configuration.MappedFolders.MappedFolder.Count -ne 2) {
    throw 'Sandbox has unsafe configuration'
  }
  $mounts=@($xml.Configuration.MappedFolders.MappedFolder)
  if ($mounts[0].ReadOnly -ne 'true' -or $mounts[1].ReadOnly -ne 'false') { throw 'Sandbox filesystem mount restrictions missing' }
  $refused=$false
  try { & (Join-Path $PSScriptRoot '../scripts/new-forge-windows-sandbox.ps1') -InputDirectory $input -FreshOutputDirectory $output -SandboxFile $wsb | Out-Null }
  catch { if ($_.Exception.Message -match 'must be new') { $refused=$true } }
  if (-not $refused) { throw 'Existing output unexpectedly allowed' }
  Write-Output 'WINDOWS_JOB_OBJECT_AND_OFFLINE_SANDBOX_CONFIG_PASS_NO_JAVA'
} finally {
  [IO.Directory]::Delete($root, $true)
}
