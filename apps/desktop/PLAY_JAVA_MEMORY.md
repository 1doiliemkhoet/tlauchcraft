# TLaunchCraft Desktop — Vanilla Play & Java/Memory integration (Issue #24)

## Contract and safety

The Tauri desktop bridge uses **real** merged `tl-game`, `tl-core` and `tl-storage` functions. Neither a successfully spawned Java process nor a `READY` metadata record is proof of Minecraft reaching its main menu.

**Tauri command contract:**

| Command | Parameters | Result |
| --- | --- | --- |
| `instance_create_vanilla` | `req: { name, version, profile_id }` | Persisted `GameInstance` in `empty` state |
| `game_install` | `instanceId` | Operation id; actual worker fetches Mojang metadata/files and verifies checksums |
| `game_cancel` | `instanceId` | Cooperative cancellation request, not immediate success |
| `game_status` | `instanceId` | SQLite install state + actual process status |
| `game_launch` | `request: LaunchRequest` | Spawned Java PID/log path; **not** game menu E2E |
| `game_stop` | `instanceId` | Terminate launcher-owned process only |
| `java_memory_info` | none | Current Windows total/available/reserved MB |
| `java_settings_get` | `instanceId` | Versioned per-instance JVM settings, defaults when no saved record |
| `java_settings_save` | `instanceId, xmsMb, xmxMb, javaPath, expectedRevision` | Persisted settings with CAS conflict protection |

**Other existing commands** (`desktop_capabilities`, profile list/create, instance list, read-only `pack_inspect`) are preserved.

### SQLite: additive schema, no profile resets

`%LOCALAPPDATA%\tlauchcraft\launcher.sqlite3` remains the single source of truth. Foundation migrations run first; then Tauri adds (if absent):

```sql
CREATE TABLE IF NOT EXISTS instance_java_settings (
  instance_id TEXT PRIMARY KEY REFERENCES instances(id) ON DELETE CASCADE,
  xms_mb INTEGER NOT NULL CHECK(xms_mb>=256),
  xmx_mb INTEGER NOT NULL CHECK(xmx_mb>=xms_mb),
  java_path TEXT NOT NULL DEFAULT '',
  revision INTEGER NOT NULL DEFAULT 0
);
```

- Does **not** replace or alter profile rows, profile UUIDs, instance JSON, Foundation migration versions, worlds or loader directories.
- An unsaved instance uses recommended defaults (Xms up to 512 MB; Xmx up to 2048 MB, capped by current Windows free/total memory).
- Changes use `expectedRevision` in an atomic transaction. A stale editor is rejected, never overwrites newer settings. Each instance has independent settings.
- Windows total/available physical memory are queried before saving and again before launching; a 1 GiB physical system reserve and 512 MiB currently-available reserve are applied.
- A Java executable must be a readable absolute `.exe` file, or empty (system `java.exe`). `-version` is probed on selecting a file; exact Mojang-required Java major is checked by `tl-game::vanilla::build_vanilla_launch` when launching.

### Vanilla lifecycle

1. Create an instance attached to an **existing** offline profile. The profile UUID is not regenerated. The in-game offline UUID is derived by `tl_core` separately.
2. Install calls official Mojang metadata resolver and verified download installer, then native DLL extraction; `READY` is persisted only after verified files, extraction and metadata are recorded. Errors produce `FAILED`; cooperative cancellation produces `CANCELLED` after worker acknowledgement.
3. Only `READY` Vanilla instances with a matching profile are eligible for Play. Launch reads saved memory settings, checks available RAM, probes the Java major against the version, builds the **real** `-Xms` and `-Xmx` parameters via `tl-game::build_command` and launches Java with per-launch stdout/stderr logs.
4. Status queries actual running PID/exit state; Stop terminates only that launcher-owned child.
5. Progress messages reflect **actual stages only**; `percent=null` until `tl-game` exposes verified byte counters. Do not fabricate percentages, end-user success, main-menu or world state.

**Out of scope / disabled:** Forge/Fabric/CurseForge installation and launcher Play for those loaders. `pack_inspect` remains read-only. Chat 4 must supply a real verified modpack install and launch contract before enabling those actions. No background fake work.

### Windows checks

Run in repo:

```powershell
cd apps/desktop
npm ci
npm test
npm run build
```

```powershell
cd src-tauri
cargo +stable test --locked
cargo +stable check --locked
```

For real E2E, install the new NSIS on disposable Windows 10/11. Create a Vanilla instance for a persistent profile, change Xms/Xmx and Java path, restart launcher, verify SQLite `instance_java_settings` revision and values, install with actual network files, start Java, inspect command line for the two heap args, screenshot Minecraft main menu, then exit/reopen a world. Keep installer, app and Minecraft logs and screenshots. None of G3/G5/G8 may be marked PASS from Rust unit tests or PID alone.
