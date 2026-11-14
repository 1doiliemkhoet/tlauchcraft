# No GUI or installer needed: verify PowerShell parsing and capture contract on Windows.
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$paths = @(
  'apps/desktop/scripts/g8-installer-smoke.ps1',
  'docs/control/scripts/g8-manual-capture.ps1',
  'docs/control/scripts/g8-interactive-capture.ps1'
)
foreach ($relative in $paths) {
  $path = Join-Path $repo $relative
  if (!(Test-Path -LiteralPath $path)) { throw "Missing script: $relative" }
  $tokens=$null; $errors=$null
  [void][System.Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
  if ($errors.Count -gt 0) {
    throw "PowerShell parser failed for $relative : $($errors | ForEach-Object Message | Out-String)"
  }
  Write-Host "PARSE_PASS $relative"
}
$collector = Get-Content -LiteralPath (Join-Path $repo 'docs/control/scripts/g8-interactive-capture.ps1') -Raw
$required=@(
 'installer-open','installer-complete','app-open','app-responsive',
 'profile-created','profile-reopened','zip-picker','metadata',
 'launcher.sqlite3','Get-WinEvent','CopyFromScreen','sqlite3.exe',
 'Get-FileHash','NOT_REVIEWED'
)
foreach ($name in $required) {
  if (-not $collector.Contains($name)) { throw "Missing evidence-collector contract: $name" }
}
$smoke=Get-Content -LiteralPath (Join-Path $repo 'apps/desktop/scripts/g8-installer-smoke.ps1') -Raw
foreach ($contract in @('Get-FileHash','/S /D=','tlauchcraft-desktop.exe','results.json')) {
  if (-not $smoke.Contains($contract)) { throw "Smoke contract regression: $contract" }
}
Write-Host 'G8_CAPTURE_CONTRACT_TESTS_PASS (static source checks only, NOT GUI E2E)'
