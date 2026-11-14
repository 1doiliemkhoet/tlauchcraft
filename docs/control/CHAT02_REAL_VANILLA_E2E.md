# Chat 2 — Issue #22: Real Windows Vanilla E2E

**Status:** G2 NOT_VERIFIED · G3 NOT_VERIFIED · G4 NOT_VERIFIED. GitHub Actions Rust tests do not verify Minecraft gameplay. The Control desktop previously verified TLaunchCraft GUI/SQLite only, **not** the game installation or world.

## Scope and prerequisites

- Windows 10/11 with an interactive graphical desktop (not a headless CI runner), working graphics driver, sufficient storage/RAM, network access to official Mojang delivery domains.
- Windows Cargo/Rust 1.88.0 and an installed **Java 21** for Minecraft **1.21.1**. Determine the actual `java.exe` path, don't assume a particular Java distribution.
- Run in an **isolated, disposable directory** unrelated to `%APPDATA%\.minecraft` or existing saved worlds.
- Use a valid offline username (3–16 ASCII letters/digits/underscore), with identical capitalization across test runs. The harness derives the server/game UUID via `tl-core`, never from the internal SQLite profile UUID.
- Minecraft proprietary client/assets/native binaries are fetched only by the operator from approved official endpoints; **never upload them**, or Microsoft access tokens, into the repo/PR.

## Commands for Control (PowerShell on Windows)

From the local repository:

```powershell
git fetch origin
git switch feat/tl-02-real-vanilla-e2e
git pull --ff-only origin feat/tl-02-real-vanilla-e2e
```

Select your real Java path and inspect the host:

```powershell
$java = (Get-Command java.exe).Source; & $java -version
[System.Environment]::OSVersion.VersionString
```

Use a disposable root for the test; the following command downloads and verifies the official client/libraries/assets and runs Minecraft (do not run it from a production game save directory):

```powershell
$root = 'C:\TLaunchCraft\Vanilla-E2E-122'
cargo run --locked --release -p tl-game --example vanilla_e2e -- 1.21.1 $root $java TestPlayer
```

The process remains attached until Minecraft closes. Expected **evidence files** under `$root\evidence\`: `install-checksums-*.json` (actual per-file SHA-1), `launch-*.json` (PID), `runtime-*.json` (Windows version, actual Java version, process exit code, expected game version and log presence), `client-*-stdout.log`, `client-*-stderr.log`, `world-snapshot-*.json`; Minecraft itself may write `$root\instance\logs\latest.log`. These files exist only if the respective stage reaches that point.

## Control acceptance, no synthetic PASS

**G2:** Prove the command actually completed metadata download, full client/libraries/natives/assets install, `write_install_checksums` returned a nonzero full manifest, correct Java major 21, and no unreported verification errors. Record OS version, Java version and Minecraft version. Check the JSON manifest contains verified SHA-1 entries for ClientJar, Library, NativeArchive, AssetIndex and AssetObject (as applicable for this release). Do not count unit/fixture tests as real install.

**G3:** While the Java PID from `launch-*.json` is running, take an **actual screenshot** of the fully rendered Minecraft 1.21.1 main menu. Ensure the PID correlates with a running Java process and `latest.log` / stdout / stderr. If game exits unexpectedly, share sanitized stderr/log traceback and `runtime-*.json`, NOT a success claim.

**G4:** In that game create a uniquely named Singleplayer world, take a real in-world screenshot, save and quit, fully close Minecraft, retain the first `world-snapshot-*.json`. Run **the same command again** with the same `$root` and `TestPlayer`, reopen the same world, take another in-world screenshot, save and exit, compare the two `world-snapshot-*.json` files and confirm `$root\instance\saves\<world>\level.dat` still exists. **Changed SHA-1 by itself is not proof of a successful world reopening; visual evidence is required.**

PowerShell evidence inspection after closing the game:

```powershell
Get-ChildItem "$root\evidence" | Sort-Object Name | Select-Object Name,Length
Get-Content "$root\instance\logs\latest.log" -Tail 60 -ErrorAction SilentlyContinue
Get-ChildItem "$root\evidence\runtime-*.json" | ForEach-Object { Get-Content $_.FullName -Raw }
```

Redact usernames/path PII and session secrets from logs before posting to GitHub. Attach only sanitized manifests, logs and screenshots. Do not add screenshot files or worlds directly to the source repo.

## If G2/G3/G4 fail

Report the **first failing stage**, actual exception/log, `runtime-*.json` if produced, Windows and Java versions. A failed install may not produce a runtime JSON (the game was never spawned). Do not manually force `READY` or mark a gate PASS.

## Handoff to Chat 7

- Connect Install to `tl_game::vanilla::resolve_official_version` → `install_vanilla` → `write_install_checksums` → `extract_natives`, ensuring the UI only marks **READY** after all steps succeed.
- Connect Play to `VanillaIdentity::from_offline_profile(&persisted_profile)` → `build_vanilla_launch` → `tl_game::launch`; profile ID is not game UUID. Persist game instance paths and display launch/exit errors.
- Do not claim menu/world success from a child PID, and never surface/write session tokens to diagnostics. Chat 7 owns the desktop button wiring; this PR is Chat 2's E2E diagnostics.
