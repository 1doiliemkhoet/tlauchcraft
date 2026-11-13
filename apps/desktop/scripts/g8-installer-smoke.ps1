# Windows runner proves an installer artifact exists and can be invoked.
# It does NOT assert interactive WebView2, offline profile creation or ZIP UI E2E.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$evidence = Join-Path $PSScriptRoot '..\g8-evidence'
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
Start-Transcript -Path (Join-Path $evidence 'installer-transcript.txt') -Force | Out-Null
$report = [ordered]@{
  timestamp_utc = (Get-Date).ToUniversalTime().ToString('o')
  os = [Environment]::OSVersion.VersionString
  powershell = $PSVersionTable.PSVersion.ToString()
  installer_artifact = 'NOT_VERIFIED'
  installer_sha256 = $null
  installer_bytes = $null
  webview2_runtime = 'NOT_VERIFIED'
  silent_install = 'NOT_TESTED'
  app_binary = 'NOT_VERIFIED'
  app_process = 'NOT_TESTED'
  profile_via_ui = 'NOT_TESTED'
  profile_sqlite_persistence = 'NOT_TESTED'
  zip_picker_via_ui = 'NOT_TESTED'
  modpack_metadata_via_ui = 'NOT_TESTED'
  manual_clean_machine = 'NOT_TESTED'
  notes = @()
}
try {
  $nsis = Join-Path $PSScriptRoot '..\src-tauri\target\release\bundle\nsis'
  $installers = @(Get-ChildItem -LiteralPath $nsis -Filter '*.exe' -File -ErrorAction Stop)
  if ($installers.Count -ne 1) { throw "Expected exactly one NSIS exe, found $($installers.Count)" }
  $installer = $installers[0]
  if ($installer.Length -lt 100000) { throw "Installer unexpectedly small ($($installer.Length) bytes)" }
  $report.installer_artifact = 'VERIFIED'
  $report.installer_sha256 = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash
  $report.installer_bytes = $installer.Length
  Write-Host "NSIS_FILE=$($installer.FullName) SIZE=$($installer.Length) SHA256=$($report.installer_sha256)"

  # Microsoft WebView2 Evergreen runtime registry marker (both 32/64 views).
  $wvKey = 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F1E7E0A8-89EE-4CB8-9F6E-E14B5F6B2E5E}'
  $wvVersions = @()
  foreach ($view in @([Microsoft.Win32.RegistryView]::Registry32,[Microsoft.Win32.RegistryView]::Registry64)) {
    $key = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $view).OpenSubKey($wvKey)
    if ($key) { $pv = $key.GetValue('pv'); if ($pv) { $wvVersions += [string]$pv } }
    $userKey = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser, $view).OpenSubKey($wvKey)
    if ($userKey) { $pv = $userKey.GetValue('pv'); if ($pv) { $wvVersions += [string]$pv } }
  }
  if ($wvVersions.Count -gt 0) {
    $report.webview2_runtime = 'DETECTED_VERSION_' + (($wvVersions | Select-Object -Unique) -join ',')
  } else {
    $report.notes += 'WebView2 runtime registry key not found; installer may download it or the machine may lack runtime.'
  }

  # Explicit isolated destination on a newly provisioned GitHub-hosted Windows runner.
  $target = Join-Path $env:RUNNER_TEMP 'tlauchcraft-g8-install'
  if (Test-Path $target) { throw "Test destination already exists: $target" }
  # NSIS /D must be the final unquoted argument. Run synchronously and enforce exit code.
  $args = "/S /D=$target"
  $process = Start-Process -FilePath $installer.FullName -ArgumentList $args -Wait -PassThru
  $report.silent_install = if ($process.ExitCode -eq 0) { 'EXIT_0' } else { 'FAILED_EXIT_' + $process.ExitCode }
  Write-Host "INSTALL_EXIT=$($process.ExitCode) TARGET=$target"
  if ($process.ExitCode -ne 0) { throw "Installer returned exit code $($process.ExitCode)" }
  $binary = Join-Path $target 'tlauchcraft.exe'
  $programFiles32 = [Environment]::GetFolderPath('ProgramFilesX86')
  $candidateRoots = @(
    $target,
    (Join-Path $env:LOCALAPPDATA 'tlauchcraft'),
    (Join-Path $env:LOCALAPPDATA 'Programs\tlauchcraft'),
    (Join-Path $env:ProgramFiles 'tlauchcraft'),
    (Join-Path $programFiles32 'tlauchcraft')
  ) | Select-Object -Unique
  $candidates = @()
  foreach ($root in $candidateRoots) {
    if (Test-Path -LiteralPath $root) {
      $report.notes += "Installed-path candidate exists: $root"
      $candidates += @(Get-ChildItem -LiteralPath $root -Filter 'tlauchcraft.exe' -File -Recurse -ErrorAction SilentlyContinue)
      $report.notes += "Path entries: " + ((Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue | Select-Object -First 10 -ExpandProperty Name) -join ', ')
    }
  }
  $uninstallRoots = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
  )
  foreach ($key in $uninstallRoots) {
    if (Test-Path $key) {
      Get-ChildItem $key -ErrorAction SilentlyContinue | ForEach-Object {
        $entry = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
        if ($entry.DisplayName -match 'tlauchcraft') {
          $report.notes += "Uninstall entry: $($entry.DisplayName), InstallLocation=$($entry.InstallLocation), UninstallString=$($entry.UninstallString)"
          if ($entry.InstallLocation -and (Test-Path $entry.InstallLocation)) {
            $candidates += @(Get-ChildItem -LiteralPath $entry.InstallLocation -Filter 'tlauchcraft.exe' -File -Recurse -ErrorAction SilentlyContinue)
          }
        }
      }
    }
  }
  $candidates = @($candidates | Sort-Object FullName -Unique)
  if (Test-Path -LiteralPath $binary) { $report.app_binary = 'VERIFIED' }
  elseif ($candidates.Count -eq 1) {
    $binary = $candidates[0].FullName
    $report.app_binary = 'VERIFIED'
  } else {
    throw "Installer exit 0 but expected app executable not found. Candidates count=$($candidates.Count); inspect structured results notes."
  }
  $report.app_binary = 'VERIFIED'
  $report.notes += "Installed executable: $binary"

  # GitHub-hosted runners lack a guaranteed interactive desktop. This is a process smoke only.
  try {
    $app = Start-Process -FilePath $binary -WorkingDirectory (Split-Path $binary) -PassThru
    Start-Sleep -Seconds 6
    $app.Refresh()
    if ($app.HasExited) {
      $report.app_process = "EXITED_CODE_$($app.ExitCode)"
      $report.notes += 'Process exited before smoke window ended; UI WebView2 not verified.'
    } else {
      $report.app_process = "ALIVE_PID_$($app.Id)"
      $report.notes += 'Process alive is not proof that WebView2 rendered or that the UI is interactive.'
      Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
    }
  } catch {
    $report.app_process = 'FAILED'
    $report.notes += "Process smoke exception: $($_.Exception.Message)"
  }
} catch {
  $report.notes += "Installer smoke exception: $($_.Exception.Message)"
  Write-Error $_.Exception.Message
  throw
} finally {
  $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $evidence 'results.json') -Encoding UTF8
  Stop-Transcript | Out-Null
}
