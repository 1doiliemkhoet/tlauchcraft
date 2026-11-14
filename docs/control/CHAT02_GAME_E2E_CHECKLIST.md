# Chat 2 — Vanilla Windows G2/G3/G4 acceptance (in progress)

This branch adds a **library-level** install and launch orchestrator. It does not bundle, mirror or redistribute Minecraft, Java binaries or account credentials. It is not a standalone executable or UI yet; a desktop caller must wire these APIs into a flow.

## Implementation status

- `resolve_official_version(version_id)`: resolve official Mojang *release* metadata via HTTPS and verify version JSON SHA-1 against its manifest entry. Versions with inheritance are deliberately rejected.
- `install_vanilla(version, root, cancelled)`: fetch and SHA-1-verify official client JAR, allowed libraries, native archives, logging configuration, asset index and asset objects. Existing divergent files are **not overwritten**; manual repair is required.
- `extract_natives(archives, destination)`: Windows PowerShell/.NET `ZipArchive` reads previously verified official archives; inspects every entry, rejects unsafe paths, links and device entries, prevents duplicate DLL names, limits file count/uncompressed sizes, extracts only DLLs into a new directory with `CreateNew`, and cleans up a failed run. Windows CI adds real ZIP fixture tests for valid DLL, traversal and cross-archive duplicate rejection.
- `build_vanilla_launch(version, paths, identity, min_ram_mb, max_ram_mb)`: use explicitly selected compatible Java, derive classpath, environment-filtered arguments and Mojang placeholders, preserve native library path. Game UUID is separate from persisted `tl-core` local profile ID. Access token must come from an authorized caller, never included in evidence files.
- `write_launch_evidence(path, version, java, pid, stdout, stderr)`: record nonsecret runtime facts, **always** set menu/world verification to false. A PID alone does not prove G3.

Not yet complete: trusted Windows Java auto-provisioning, old native layout compatibility, third-party security review of the hardened extractor, executable GUI integration and end-to-end Singleplayer tests. This code does **not** establish that launch of real Minecraft works; no game runtime is available in this assistant's execution environment.


## Run on a real Windows desktop (manual G2/G3/G4)

Use an isolated, fresh root directory that does not contain personal worlds. Install a compatible Java runtime separately; Minecraft 1.21.1 expects Java 21. Use a local offline username. The harness derives the **game/server UUID** via `tl_core::vanilla_offline_server_uuid` (case-sensitive MD5 v3), rather than trusting a manually supplied UUID or the internal persisted profile ID.

From PowerShell in the repository worktree on this branch:

```powershell
git switch feat/tl-02-game-e2e
cargo run --locked --release -p tl-game --example vanilla_e2e -- 1.21.1 'C:\TLaunchCraft\E2E' 'C:\Program Files\Java\jdk-21\bin\java.exe' TestPlayer 00000000000000000000000000000001
```

Replace the Java path with the installed Java executable. Use the **same username** when relaunching; changing name or capitalization changes the offline UUID and can change LAN inventory/playerdata. In the desktop product use `VanillaIdentity::from_offline_profile` with the **persisted** SQLite `OfflineProfile` from PR #18; never substitute `profile.id` for the server UUID. In offline/local mode the harness uses a local placeholder token and `legacy` user type by default; it does **not** authenticate a Microsoft account, bypass an online-mode server or establish a Minecraft entitlement. To use an authorized online session instead, set `TLAUCHCRAFT_ACCESS_TOKEN`, `TLAUCHCRAFT_USER_TYPE`, and optionally `TLAUCHCRAFT_XUID` / `TLAUCHCRAFT_CLIENT_ID` in the environment without saving the token in scripts or logs.

When the game exits, run the same `cargo run` command again to reopen the saved Singleplayer world. Every invocation creates a distinct natives directory, stdout/stderr logs and launch JSON evidence; the same `instance/saves` directory is reused. The harness does **not** automatically detect a displayed main menu, create a world, or verify world re-opening: a tester must collect observable screenshots and precise evidence, then update the PR. Launching the Java process alone is not G3.

**Native archive safety:** The bundled `native_extract.ps1` performs bounded two-phase ZIP validation and copies only DLL streams into a flat, empty, launcher-owned directory. This avoids invoking `tar.exe` on archive paths. It rejects traversal/link entries, duplicate DLL basenames and size/count excesses. Real Windows ZIP-fixture tests are required and have passed in CI, but that is not a security audit or real Mojang native compatibility test. Use only official SHA-1-verified archives in isolated test roots; never treat this as a general-purpose untrusted modpack extractor.

## Windows CI

Use the existing PR Windows Rust pipeline:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

## Windows real-game E2E checklist (G2–G4)

1. On a Windows test machine with an independently authorized Minecraft install/session and an appropriate JDK/JRE, select an official release version. Keep profile and world files in a launcher-owned test instance. Do not commit game or Java files to Git.
2. Use the caller integration to resolve and verify version metadata, then run installer. Record installed file inventory, expected and actual SHA-1, errors and versions; **G2** needs an observed successful install.
3. Verify native extraction and select compatible Java via `java -version`. Launch with `tl-game::launch`, collect PID, stdout, stderr, client logs, version ID and a real screenshot of Minecraft's **main menu**. A spawned PID is insufficient for G3.
4. Create a new Singleplayer world; name it uniquely; take a screenshot in world; save/exit; fully close game; relaunch and reopen the same world; verify player progress and `saves/<world>` persisted. Attach screenshots and logs to the PR without access tokens or private user details.
5. Clean up safely. Do not delete an existing Minecraft installation, user's worlds or launcher profiles.

### Gate state

- G1: accepted by Control for PR #12; new PR #19 Windows Rust fmt/Clippy/tests PASS, 84/84 at run #43583681258.
- G2: NOT_TESTED.
- G3: NOT_TESTED.
- G4: NOT_TESTED.

Do not mark G2/G3/G4 PASS without traceable real Windows execution evidence.
