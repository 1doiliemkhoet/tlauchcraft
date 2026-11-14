# G8 — Windows Installer E2E acceptance (Chat 7)

## Evidence categories

- **CI build proof**: Actual NSIS `.exe`, byte size, SHA-256, Windows job log and uploaded artifact. NOT equivalent to successful installation.
- **Automated install smoke**: A fresh GitHub-hosted Windows runner, silent NSIS /S exit code, installed `.exe` discovery, WebView2 registry check and process-alive/exit record. A GitHub runner is **not** a certified clean user workstation; no UI screenshot or keyboard interaction is implied.
- **Manual clean-machine acceptance (mandatory)**: Use a new Windows 10/11 VM with no launcher data. Save VM build + installed WebView2 version; record install wizard steps, generated app icon, start menu entry and app screen with timestamp.
- **Profile persistence**: Create a valid Offline Profile through Tauri UI, capture UI screenshot with generated UUID (redact unnecessary data). Close app, reopen, verify same UUID; check `%LOCALAPPDATA%\tlauchcraft\launcher.sqlite3` exists with row in profiles table (read-only SQLite query); record timestamps. Do not delete an existing profile or world.
- **Modpack picker**: Choose a harmless test .mrpack/.zip with valid metadata via native Windows picker. Screenshot inspect result including SHA-256 and warnings. This only proves inspection, NOT import/install, Play, Minecraft main menu or modpack functionality.
- **Logs**: Archive installer transcript and results.json from CI. On a real machine capture Windows Application event logs and `%LOCALAPPDATA%\tlauchcraft` metadata safely; protect PII and do not collect unnecessary secrets.
- **Final criteria**: G8 requires a real installer artifact **and** confirmed installation + usable UI/SQLite/ZIP on a clean Windows machine. If manual steps are absent, mark `G8_PARTIAL` or `NOT_VERIFIED`, never PASS.

## Commands (Windows PowerShell, after downloading the CI artifact)

```powershell
Get-FileHash .\tlauchcraft_*_x64-setup.exe -Algorithm SHA256
Start-Process .\tlauchcraft_*_x64-setup.exe -Wait
```

```powershell
Get-Item "$env:LOCALAPPDATA\tlauchcraft\launcher.sqlite3"
Get-WinEvent -FilterHashtable @{LogName='Application'; StartTime=(Get-Date).AddHours(-2)} -MaxEvents 100 | Out-File .\windows-application-events.txt
```

Use a dedicated disposable Windows VM to avoid modifying an existing installation. The normal installer may show/download WebView2. Keep game launch, full import, LAN and skin functionality disabled until each native backend supports them.
