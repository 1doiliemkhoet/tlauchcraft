param(
 [Parameter(Mandatory=$true)]
 [ValidateSet('baseline','installer','app-open','profile-created','profile-reopened','zip-picker','metadata')]
 [string]$Stage,
 [string]$ProfileUuid = '',
 [string]$ModpackFile = '',
 [string]$OutputDir = '.\g8-manual-evidence'
)
$ErrorActionPreference='Stop'
if (-not $IsWindows) { throw 'Requires Windows interactive desktop' }
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$prefix = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ') + '-' + $Stage
$os = Get-CimInstance Win32_OperatingSystem
$db = Join-Path $env:LOCALAPPDATA 'tlauchcraft\launcher.sqlite3'
$record = [ordered]@{
 stage=$Stage; time_utc=(Get-Date).ToUniversalTime().ToString('o')
 os_caption=$os.Caption; os_version=$os.Version; os_build=$os.BuildNumber
 sqlite_path=$db; sqlite_exists=(Test-Path -LiteralPath $db)
 sqlite_header_ok=$false; sqlite_bytes=$null
 operator_entered_uuid=$ProfileUuid; selected_archive_path=$ModpackFile
 archive_sha256=$null; screen='NOT_CAPTURED'; screen_error=$null
 webview2_versions=@(); app_processes=@()
 evidence_status='CAPTURE_ONLY_NOT_AUTOMATIC_PASS'
}
foreach ($hive in @([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryHive]::CurrentUser)) {
 foreach ($view in @([Microsoft.Win32.RegistryView]::Registry32,[Microsoft.Win32.RegistryView]::Registry64)) {
  $root=[Microsoft.Win32.RegistryKey]::OpenBaseKey($hive,$view)
  $key=$root.OpenSubKey('SOFTWARE\Microsoft\EdgeUpdate\Clients\{F1E7E0A8-89EE-4CB8-9F6E-E14B5F6B2E5E}')
  if ($key -and $key.GetValue('pv')) { $record.webview2_versions += "$hive/$view=$($key.GetValue('pv'))" }
 }
}
$record.app_processes=@(Get-Process -Name tlauchcraft-desktop -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,MainWindowTitle)
if ($record.sqlite_exists) {
 $record.sqlite_bytes=(Get-Item -LiteralPath $db).Length
 $stream=[IO.File]::Open($db,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
 try {
  $bytes=New-Object byte[] 16
  $read=$stream.Read($bytes,0,16)
  if ($read -eq 16) { $record.sqlite_header_ok=([Text.Encoding]::ASCII.GetString($bytes) -eq ("SQLite format 3"+[char]0)) }
 } finally { $stream.Dispose() }
}
if ($ModpackFile -and (Test-Path -LiteralPath $ModpackFile)) {
 $record.archive_sha256=(Get-FileHash -LiteralPath $ModpackFile -Algorithm SHA256).Hash
}
try {
 Add-Type -AssemblyName System.Windows.Forms
 Add-Type -AssemblyName System.Drawing
 $rect=[Windows.Forms.SystemInformation]::VirtualScreen
 $bmp=[Drawing.Bitmap]::new($rect.Width,$rect.Height)
 try {
  $graphics=[Drawing.Graphics]::FromImage($bmp)
  try { $graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$bmp.Size) } finally { $graphics.Dispose() }
  $img=Join-Path $OutputDir "$prefix.png"
  $bmp.Save($img,[Drawing.Imaging.ImageFormat]::Png)
  $record.screen=$img
 } finally { $bmp.Dispose() }
} catch { $record.screen_error=$_.Exception.Message }
try {
 Get-WinEvent -FilterHashtable @{LogName='Application';StartTime=(Get-Date).AddHours(-2)} -MaxEvents 60 -ErrorAction Stop |
 Select-Object TimeCreated,Id,ProviderName,LevelDisplayName,Message |
 ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $OutputDir "$prefix-events.json")
} catch { $record.events_error=$_.Exception.Message }
$record | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $OutputDir "$prefix.json")
Write-Host "Captured stage=$Stage screen=$($record.screen) sqlite_exists=$($record.sqlite_exists)"
Write-Host 'Human Control must inspect screenshots, query SQLite and verify matching UUID. No automatic G8 PASS.'
