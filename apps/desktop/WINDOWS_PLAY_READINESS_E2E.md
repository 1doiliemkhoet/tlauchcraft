# Control — Desktop Play readiness verification after Chat 2 G2

**Gate:** Chat 2's standalone Vanilla G2/G3 passed and G4 is user-confirmed, but **Desktop-specific GUI E2E is NOT_VERIFIED** until Control drives the new Desktop installer. Chat 2 PR #26 merged at `634213cd8f50355fe1dc3fcd9c0a25b42d110e46` and its API is now used directly by this branch. All seven Rust workspace crates, SQLite CAS and offline UUID are preserved.

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
| Native extraction | Use merged Chat 2 G2 native DLL extractor; induce a reproducible safe failure in test instance | Extractor's actual message visible, state FAILED, no Play until full verified install |
| Cancellation | Click Install, then Cancel while worker is downloading | Actual cooperative cancel -> CANCELLED, no fabricated 100%, retry permitted |
| App restart | Close launcher during active install, relaunch | Unowned install is reconciled as FAILED with actual interruption explanation, not stuck DOWNLOADING |
| Vanilla install | Install official Minecraft 1.21.1 | State READY **only after** verified files, native DLLs and metadata. Check `instances/<id>/evidence/install-checksums-*.json` independently rehashes every official file and native DLL count >0. Save logs and progress stage screenshots |
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

**Contract coupling:** Desktop now consumes the *merged* `tl_game::vanilla::{resolve_official_version,install_vanilla,extract_natives,build_vanilla_launch}` and `tl_game::vanilla_evidence::write_install_checksums`. New installation sets READY only after independent SHA-1 rehash and nonzero extracted Windows DLLs. Desktop root: `%LOCALAPPDATA%\\tlauchcraft\\instances\\<instance UUID>`; Java `game_dir` is its persistent `game` subfolder, with saves under `game\\saves`. A successful standalone `vanilla_e2e.exe` run at another root does **not** automatically register or adopt that installation into Desktop. Do not delete or move any existing saved worlds. Desktop G3 still requires Control's real GUI test.


## P0 console popup / Downloads latency retest (PR #27)

Previous NSIS run `43633240009` was observed by Control to flash CMD when Java started/stopped, sometimes on Downloads tab, and to stutter in Downloads. The code now uses `CREATE_NO_WINDOW` in `tl-game::probe_java` and `tl-game::launch`, as well as `java_settings::validate` and native extraction PowerShell. RAM detection now uses native `kernel32::GlobalMemoryStatusEx` instead of spawning `powershell.exe Get-CimInstance`. No `java.exe` path substitution, JVM 8/21 version policy change, file/world move or database reset.

**Quantitative code changes (not measured GUI speedups):**

| Signal | Previous | New code |
| --- | --- | --- |
| Idle status refresh | Every 1,600 ms | Every 9,000 ms |
| Active installation refresh | Every 1,600 ms | Every 2,400 ms |
| Downloads items in initial render | Up to 100 | Latest 24, explicit Show all |
| RAM Windows query | PowerShell CIM child | In-process GlobalMemoryStatusEx |
| Java launch/probe window flags | default Windows console | CREATE_NO_WINDOW |
| Java preflight/launch/settings | synchronous Rust Tauri handlers | spawn_blocking async commands |

**Real release benchmark:** Open Activity logs, switch Home → Downloads → Home 10 times and record the emitted `UI navigation downloads: N.Nms` entries, plus observed CMD popups. Repeat while a real instance is downloading and while Java is running; report median and p95 before/after where comparable measurements exist. Browser two-frame mark is diagnostic, not a full frame-time profiler. There are **no measured before/after GUI timings from this agent**.

**Java process ownership:** Two `java.exe` PIDs are not proof of duplicate launcher-owned Minecraft. Confirm full command line, process parent ID, individual start times and game directory before classifying. Within one launcher process, duplicate Play is refused for the same instance, and Stop only kills children present in the in-memory launcher-owned process registry. After a launcher restart the previous children are **not automatically adopted or terminated**. A still-running Java process from a previous launcher can therefore coexist with a later launch, and this must be checked manually before declaring duplicate-process protection fully verified. **Never kill Java processes solely by executable name or PID reported in a screenshot.**

```powershell
Get-CimInstance Win32_Process -Filter "Name='java.exe' OR Name='javaw.exe'" |
 Select-Object ProcessId,ParentProcessId,CreationDate,CommandLine
```

For acceptance, preserve the same world, capture popup/no-popup screenshots or video, compare the log's UI navigation measurements, check RAM and Java 8/21 compatibility, and report all PIDs and their launch roots. Keep **full G8 PARTIAL** until Control passes these checks.
