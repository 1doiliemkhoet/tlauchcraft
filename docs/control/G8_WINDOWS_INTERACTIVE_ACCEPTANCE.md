# G8 Windows 10/11 interactive acceptance — Chat 7

STATUS: NOT_VERIFIED pending actual clean Windows desktop evidence. No service-runner process spawn counts as WebView2 UI proof.

Use a disposable Windows 10/11 VM with real desktop and PowerShell 7. Download the NSIS setup from G8 run 43498282763. Verify SHA-256 D7FF3976C58EB768838DE9B27F066977AAED19506E410ECBD1E64EA8BE9D663B. Do not use an existing machine with worlds or profiles.

From the feature branch, use docs/control/scripts/g8-manual-capture.ps1 as the collector. For each stage below, run the command from an interactive PowerShell 7 window while the actual target UI is visible.

1. BEFORE INSTALL: Get-FileHash on the setup EXE, capture baseline using pwsh -File docs/control/scripts/g8-manual-capture.ps1 -Stage baseline. Record Windows build and WebView2 version.
2. GUI INSTALL: double click the NSIS installer and complete the actual wizard. Capture installer wizard and final screen with -Stage installer. Script does not click installer.
3. WEBVIEW2: open tlauchcraft from Start Menu. Verify real rendered navigation, not just process. Capture with -Stage app-open. If window blank, record failure.
4. PROFILE: GUI > Offline profiles > create a new test user. Copy UUID shown onscreen. Capture with -Stage profile-created -ProfileUuid UUID.
5. PERSISTENCE: close via window close control; relaunch the app and view same profile/UUID. Capture -Stage profile-reopened -ProfileUuid UUID. The operator-entered UUID is NOT independent database evidence.
6. SQLITE: actual path from tl-storage is %LOCALAPPDATA%\tlauchcraft\launcher.sqlite3. With a trusted sqlite3 CLI, perform read-only query: sqlite3 -readonly "$env:LOCALAPPDATA\tlauchcraft\launcher.sqlite3" "SELECT id,username FROM profiles ORDER BY username;". Confirm UUID equals screenshots. Save query output. If sqlite3 CLI is missing: DB_ROW_NOT_VERIFIED, not PASS.
7. ZIP: choose a benign known-good .mrpack or ZIP from Inspect ZIP / MRPACK using the native Windows dialog. Capture while dialog is open using -Stage zip-picker; after selection capture -Stage metadata -ModpackFile C:\path\to\pack.mrpack. Verify name, version, loader, SHA-256, warnings against actual archive. Archive inspect is read-only, NOT installed.

Evidence bundle: image PNG for each stage, JSON with OS/WebView2 registry/SQLite magic/process observations, read-only SQLite query output, Windows event logs, installer UI screenshots. Review for PII before sharing. A registry WebView2 marker or process PID alone does not prove a working GUI.

G8 may be VERIFIED only by Control after reviewing screenshots of wizard, rendered UI, UUID before/after, SQLite SELECT, native picker and real inspected metadata. All Play, full import, LAN and Skin actions remain disabled unless backend genuinely exists.

Prior replay workflow was deleted by Control; do not restore it or trigger unnecessary CI for these documentation changes.