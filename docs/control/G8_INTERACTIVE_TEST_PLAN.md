# Chat 7 — G8 clean Windows interactive acceptance

**Current gate:** G8_PARTIAL. A successful NSIS build, silent install and live process are *not* proof of rendered WebView2, working UI, or stored profiles. **Only Control can approve G8** after reviewing evidence from a real, clean Windows 10/11 machine.

## Test environment

Use a new disposable Windows 10/11 VM with an actual interactive desktop and PowerShell 7 (pwsh). Do not run on a machine holding valuable worlds or existing launcher profiles. Use the Control-accepted NSIS artifact from [Windows build 43498282763](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43583383473/artifacts/14110075234), and verify its SHA-256 equals `E030FA105ED8C23CB6BE10564C21AFE53BFEDE9C03C17F778A20A76FB55E6E5F`. Log OS edition/build, WebView2 runtime version and whether the machine had launcher data before installation. The real executable is `tlauchcraft-desktop.exe`. CI artifacts may expire; if the linked artifact is unavailable, use the latest successfully built trusted NSIS run and verify its SHA-256 against that run's evidence before testing.

The evidence collector at `docs/control/scripts/g8-interactive-capture.ps1` takes a screenshot of the *current real screen* after a 5-second refocus delay; it does not simulate clicks. It saves JSON with Windows details, process/window title, WebView2 registry readings, read-only SQLite lookup when `sqlite3.exe` is available, archive hash, filtered Windows error events, and a listing of app logs *only if they exist*. It **never** declares G8 PASS.

## Operator sequence

| Phase | Manual action | Evidence command | Required visible proof |
| --- | --- | --- | --- |
| baseline | Record Windows version and clean state | `pwsh -File docs/control/scripts/g8-interactive-capture.ps1 -Stage baseline` | OS build, no previous launcher data |
| GUI installer | Double-click actual NSIS, capture wizard and completion separately | `-Stage installer-open`, `-Stage installer-complete` | Windows installer wizard and successful finish |
| WebView2 | Open launcher from Start Menu, see rendered nav, click **Offline profiles**, return Home | `-Stage app-open`, `-Stage app-responsive` | Both views rendered and responded |
| create profile | Create unique offline test username; read UUID displayed | `-Stage profile-created -ExpectedProfileUuid <UUID>` | Username and UUID on screen, optional read-only SQL match |
| restart persistence | Close entire launcher and open it again, go to same profile | `-Stage profile-reopened -ExpectedProfileUuid <SAME_UUID>` | Same UUID on screen and same SQLite row |
| native archive dialog | Click **Inspect ZIP / MRPACK** | `-Stage zip-picker` | Real Windows file-picker window on screen |
| metadata | Select a safe valid .mrpack or .zip with known metadata | `-Stage metadata -ArchivePath C:\g8-test\sample.mrpack` | Actual inspection UI: name, version, loader, SHA-256, warnings |

Run the command above by prepending `pwsh -File docs/control/scripts/g8-interactive-capture.ps1`, from the repo root. Test on an **interactive** PowerShell session. The tool waits five seconds so the operator can refocus the intended window before screenshot capture. A screenshot is not automatically checked for authenticity or screen content.

### Mandatory SQLite corroboration

The actual database path established by `tl-storage::default_data_dir` is `%LOCALAPPDATA%\tlauchcraft\launcher.sqlite3`; table `profiles` has columns `id`, `username`, `payload`. Query the profile in read-only mode using trusted `sqlite3.exe` (may need separate install):

```powershell
sqlite3 -readonly "$env:LOCALAPPDATA\tlauchcraft\launcher.sqlite3" "SELECT id, username FROM profiles ORDER BY username;"
```

Compare the UUID with both screenshots, before and after app restart. If the CLI was unavailable, record `SQLITE_ROW_NOT_VERIFIED`, not PASS. A SQLite header, file size or process count alone is not proof of profile persistence. SQLite can use WAL, so do not infer row loss from changed file hashes. Never delete or replace the real DB.

### Archive and logging

Select a benign known-good ZIP/MRPACK through the **native Windows picker** (not a hard-coded path or browser-drop simulation). Use the archive SHA-256 in the capture JSON and compare it to the UI. Inspection metadata is not proof of installation, Play, LAN or Skin. `Play`, full import, LAN and skin features remain disabled where native backend is incomplete.

Save all stage PNG screenshots, stage JSON, SQLite read-only query summaries, filtered Windows Application error logs and any existing app logs (if no dedicated app log exists, record this explicitly). Redact usernames, paths and unrelated window contents before publishing. Include installer hash and Windows version in the acceptance package.

### Gate decision

If any mandatory interactive action lacks screenshots or corroboration, keep `G8_PARTIAL` / `NOT_VERIFIED`. No installer or process smoke evidence substitutes for WebView2 rendering, native dialog interaction or SQLite verification.

### CI scope

Control removed a duplicate replay workflow. Do not recreate it. Smoke script edits must get at least Windows PowerShell syntax and regression-contract checks via the existing desktop Windows check workflow, while avoiding a fresh NSIS release build for docs/script-only changes.
