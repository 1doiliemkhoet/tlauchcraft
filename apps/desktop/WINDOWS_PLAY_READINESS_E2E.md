# Control — Desktop Play readiness verification after Chat 2 G2

**Gate:** G3 / G5 / G8 stay NOT_VERIFIED without real Windows GUI/Java evidence. Existing Chat 2 PR #26 is DRAFT and must **not** be imported until merged to `develop`. The branch is based on official `develop` 3846759 and preserves all seven Rust workspace crates.

## Before testing

Use a **disposable Windows 10/11 VM**. Copy the real NSIS installer from the PR #27 Windows artifact. Record Windows edition/build, install mode, WebView2 version, Java major, installer SHA-256 and screenshots. Do not touch the existing Control profile or its UUID; create a new dedicated test profile and instance.

Windows PowerShell:

```powershell
Get-CimInstance Win32_OperatingSystem | Select Caption,Version,BuildNumber,TotalVisibleMemorySize,FreePhysicalMemory
Get-Command java.exe -ErrorAction SilentlyContinue | Select-Object Source
java -version
```

## Verification matrix

| Test | Real operator action | Required evidence / expected behavior |
| --- | --- | --- |
| Metadata timeout | Disconnect network **only in disposable VM** while installing an official version | Actual metadata timeout/network error displayed, state FAILED; no fake READY, Play disabled |
| SHA/checksum error | Use a dedicated corrupted *test copy* of an official game file, never modify user worlds | Real downloader rejects mismatch, error visible, failed state remains queryable after restart |
| Native extraction | Use Chat 2 G2 native DLL extractor once merged; induce a reproducible safe failure in test instance | Extractor's actual message visible, state FAILED, no Play until full verified install |
| Cancellation | Click Install, then Cancel while worker is downloading | Actual cooperative cancel -> CANCELLED, no fabricated 100%, retry permitted |
| App restart | Close launcher during active install, relaunch | Unowned install is reconciled as FAILED with actual interruption explanation, not stuck DOWNLOADING |
| Vanilla install | Install official Minecraft 1.21.1 | State READY **only after** verified files, native DLLs and metadata. Save logs and progress stage screenshots |
| Preflight | Select matching offline profile + READY instance; change Java to wrong major | Play stays disabled with actual compatibility error from `tl-game` |
| Play | Fix Java and click Play | Record actual java.exe command line containing chosen `-Xms` and `-Xmx`; PID is not G3 proof |
| Main Menu | Wait for Minecraft window on real desktop | Screenshot actual menu with exact MC version, capture logs and process status |
| World save | Create test world; close and reopen | Same world and contents present; record evidence before G3 PASS |
| Stop | Click Stop only for launcher-owned child | Process exits; no unrelated Java processes killed |
| Java settings | Save separate RAM settings on two instances, close and restart launcher | Both settings reload independently; SQLite `instance_java_settings` has expected `revision`, Xms and Xmx |
| CAS | Open stale editor/session and save with outdated revision | Real conflict displayed; newer settings not overwritten |
| Modpack | Inspect real ZIP/MRPACK | Only read-only metadata, no install/Play until Chat 4 contract is merged and verified |

### SQLite read-only evidence

Open the launcher and capture settings UI. After saving, close/reopen and compare the displayed Xms/Xmx/Java values with:

```powershell
sqlite3 -readonly "$env:LOCALAPPDATA\tlauchcraft\launcher.sqlite3" "SELECT instance_id,xms_mb,xmx_mb,java_path,revision FROM instance_java_settings ORDER BY instance_id;"
```

Do not delete, reinitialize or copy over the existing SQLite DB. Preserve source `profiles` UUIDs and existing world directories.

### Java process evidence

On Windows, while *the real Minecraft Java process* is running, read its command line:

```powershell
Get-CimInstance Win32_Process -Filter "Name = 'java.exe' OR Name = 'javaw.exe'" | Select ProcessId,CommandLine
```

Screenshot Java & Memory settings and compare actual `-Xms`/`-Xmx` flags. Redact sensitive values from command-line diagnostics if present. Tauri Play returning a PID does **not** mean main menu/world reached.

### Evidence package

Save Windows edition/build, Tauri/NSIS installer hash, screenshots for each required stage, Rust install errors, Java stdout/stderr per-launch files, SQLite read-only query, real Minecraft version and world screenshots. Use `docs/control/scripts/g8-interactive-capture.ps1` for interactive screenshots. Report observed failures explicitly and leave G3/G5/G8 unverified when screenshots are absent.

**Contract coupling:** Desktop relies on stable `tl_game::vanilla::resolve_official_version`, `install_vanilla`, `extract_natives`, `build_vanilla_launch`, and `tl_game::launch`. When Chat 2 merges its extraction fix into `develop`, synchronize and rerun Windows Desktop tests and NSIS **without importing draft PR #26**.
