# Collect evidence only. This script neither clicks the GUI nor marks acceptance PASS.
param(
  [Parameter(Mandatory=$true)]
  [ValidateSet('baseline','installer-open','installer-complete','app-open','app-responsive','profile-created','profile-reopened','zip-picker','metadata')]
  [string]$Stage,
  [string]$ExpectedProfileUuid = '',
  [string]$ArchivePath = '',
  [string]$OutputDir = '.\g8-manual-evidence'
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This collector requires an interactive Windows PowerShell 7 session.' }
$directory = [IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $directory | Out-Null
$stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ')
$key = "$stamp-$Stage"
$db = Join-Path $env:LOCALAPPDATA 'tlauchcraft\launcher.sqlite3'
$os = Get-CimInstance Win32_OperatingSystem
$report = [ordered]@{
  stage = $Stage
  captured_utc = (Get-Date).ToUniversalTime().ToString('o')
  capture_context = 'Interactive user-driven capture, no automated E2E assertion'
  operating_system = [ordered]@{
    caption=$os.Caption; version=$os.Version; build=$os.BuildNumber
    architecture=$os.OSArchitecture
  }
  webview2_registry_versions = @()
  application_processes = @()
  sqlite = [ordered]@{
    path=$db; exists=(Test-Path -LiteralPath $db); magic_header_ok=$false
    bytes=$null; journal_files=@(); readonly_query_status='NOT_RUN'
    matching_profile_uuid=$false; query_result_path=$null
  }
  expected_profile_uuid = $ExpectedProfileUuid
  selected_archive_path = $ArchivePath
  selected_archive_sha256 = $null
  screenshot_path = $null
  screenshot_capture_error = $null
  app_log_files = @()
  windows_event_log_path = $null
  windows_event_error = $null
  operator_review = 'NOT_REVIEWED'
  warnings = @()
}
# A version registry entry is supplemental: actual rendered UI still requires a screenshot.
foreach ($hive in @([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryHive]::CurrentUser)) {
  foreach ($view in @([Microsoft.Win32.RegistryView]::Registry32,[Microsoft.Win32.RegistryView]::Registry64)) {
    try {
      $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey($hive,$view)
      $entry=$base.OpenSubKey('SOFTWARE\Microsoft\EdgeUpdate\Clients\{F1E7E0A8-89EE-4CB8-9F6E-E14B5F6B2E5E}')
      if ($entry) {
        $ver=[string]$entry.GetValue('pv')
        if ($ver -and $ver -ne '0.0.0.0') { $report.webview2_registry_versions += "$hive/$view=$ver" }
      }
    } catch { $report.warnings += "WebView2 registry read unavailable: $hive/$view" }
  }
}
$report.application_processes = @(
  Get-Process -Name 'tlauchcraft-desktop' -ErrorAction SilentlyContinue |
  Select-Object Id,ProcessName,MainWindowTitle,MainWindowHandle,StartTime
)
if ($report.sqlite.exists) {
  try {
    $file=Get-Item -LiteralPath $db
    $report.sqlite.bytes=$file.Length
    $stream=[IO.File]::Open($db,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
    try {
      $header=New-Object byte[] 16
      if ($stream.Read($header,0,16) -eq 16) {
        $report.sqlite.magic_header_ok=([Text.Encoding]::ASCII.GetString($header) -eq ("SQLite format 3"+[char]0))
      }
    } finally { $stream.Dispose() }
    $report.sqlite.journal_files=@("$db-wal","$db-shm" | Where-Object { Test-Path -LiteralPath $_ })
  } catch { $report.warnings += "SQLite header read failed: $($_.Exception.Message)" }
}
# sqlite3 is optional, but a real SELECT is mandatory evidence for SQLite acceptance.
if ($ExpectedProfileUuid) {
  if (-not [Guid]::TryParse($ExpectedProfileUuid,[ref]([Guid]::Empty))) {
    $report.warnings += 'ExpectedProfileUuid is not recognized as a UUID'
  }
  $sqlite3=Get-Command 'sqlite3.exe' -ErrorAction SilentlyContinue
  if ($sqlite3 -and $report.sqlite.exists) {
    $sql = "SELECT id FROM profiles WHERE id = '$($ExpectedProfileUuid.Replace("'","''"))' LIMIT 1;"
    $out=& $sqlite3.Source -readonly $db $sql 2>&1
    if ($LASTEXITCODE -eq 0) {
      $report.sqlite.readonly_query_status='EXIT_0'
      $result=@($out | ForEach-Object { [string]$_ })
      $report.sqlite.matching_profile_uuid=($result -contains $ExpectedProfileUuid)
      $queryFile=Join-Path $directory "$key-profile-lookup.txt"
      @("SQL: SELECT id FROM profiles WHERE id = <captured UUID> LIMIT 1;",
        "MATCHING_PROFILE_UUID=$($report.sqlite.matching_profile_uuid)",
        "RESULT_ROWS=$($result.Count)") | Set-Content -Encoding UTF8 $queryFile
      $report.sqlite.query_result_path=$queryFile
    } else {
      $report.sqlite.readonly_query_status='FAILED'
      $report.warnings += "sqlite3 read-only query failed: $($out -join ' ')"
    }
  } else {
    $report.sqlite.readonly_query_status='NOT_RUN_SQLITE3_CLI_MISSING_OR_DB_ABSENT'
  }
}
if ($ArchivePath) {
  if (Test-Path -LiteralPath $ArchivePath -PathType Leaf) {
    $report.selected_archive_sha256=(Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash
  } else { $report.warnings += 'Specified archive does not exist. No metadata verification possible.' }
}
# Collect existing app logs only; never invent an app.log or collect entire user profile.
$logDirectory=Join-Path $env:LOCALAPPDATA 'tlauchcraft\logs'
if (Test-Path -LiteralPath $logDirectory) {
  $logfiles=Get-ChildItem -LiteralPath $logDirectory -File -ErrorAction SilentlyContinue |
     Where-Object { $_.Extension -in @('.log','.txt','.jsonl') } |
     Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 5
  foreach ($logFile in $logfiles) {
    $report.app_log_files += [ordered]@{ name=$logFile.Name; bytes=$logFile.Length; source=$logFile.FullName }
  }
} else {
  $report.warnings += 'No dedicated application logs folder present; Windows event log will be collected separately.'
}
Write-Host "Stage=$Stage. Focus the actual application/installer/native dialog. Screenshot in 5 seconds."
Start-Sleep -Seconds 5
try {
  Add-Type -AssemblyName System.Windows.Forms
  Add-Type -AssemblyName System.Drawing
  $rect=[System.Windows.Forms.SystemInformation]::VirtualScreen
  $bitmap=[System.Drawing.Bitmap]::new($rect.Width,$rect.Height)
  try {
    $graphics=[System.Drawing.Graphics]::FromImage($bitmap)
    try {
      $graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$bitmap.Size)
    } finally { $graphics.Dispose() }
    $png=Join-Path $directory "$key.png"
    $bitmap.Save($png,[System.Drawing.Imaging.ImageFormat]::Png)
    $report.screenshot_path=$png
  } finally { $bitmap.Dispose() }
} catch { $report.screenshot_capture_error=$_.Exception.Message }
try {
  $evfile=Join-Path $directory "$key-windows-events.json"
  $events=@(Get-WinEvent -FilterHashtable @{LogName='Application';StartTime=(Get-Date).AddHours(-2)} -MaxEvents 70 -ErrorAction Stop |
    Where-Object { $_.ProviderName -match 'Application Error|Windows Error Reporting|tlauchcraft|WebView' } |
    Select-Object TimeCreated,Id,ProviderName,LevelDisplayName,Message)
  ConvertTo-Json -InputObject $events -Depth 4 | Set-Content -LiteralPath $evfile -Encoding UTF8
  $report.windows_event_log_path=$evfile
} catch { $report.windows_event_error=$_.Exception.Message }
$outJson=Join-Path $directory "$key.json"
$report | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath $outJson -Encoding UTF8
Write-Host "CAPTURED=$outJson SCREENSHOT=$($report.screenshot_path)"
Write-Host 'This output is observations only. An operator must review screenshots, actual UI responses, SQLite query, and ZIP metadata before Control accepts G8.'
