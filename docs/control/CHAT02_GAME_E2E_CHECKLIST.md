# Chat 2 — Vanilla Windows G2/G3/G4 acceptance (in progress)

This branch adds a **library-level** install and launch orchestrator. It does not bundle, mirror or redistribute Minecraft, Java binaries or account credentials. It is not a standalone executable or UI yet; a desktop caller must wire these APIs into a flow.

## Implementation status

- `resolve_official_version(version_id)`: resolve official Mojang *release* metadata via HTTPS and verify version JSON SHA-1 against its manifest entry. Versions with inheritance are deliberately rejected.
- `install_vanilla(version, root, cancelled)`: fetch and SHA-1-verify official client JAR, allowed libraries, native archives, logging configuration, asset index and asset objects. Existing divergent files are **not overwritten**; manual repair is required.
- `extract_natives(archives, destination)`: Windows `tar.exe` extraction of previously verified official native archives, rejecting traversal paths and refusing an occupied extraction directory. Extraction is experimental, not yet security-reviewed for symlink entries and file collision.
- `build_vanilla_launch(version, paths, identity, min_ram_mb, max_ram_mb)`: use explicitly selected compatible Java, derive classpath, environment-filtered arguments and Mojang placeholders, preserve native library path. Game UUID is separate from persisted `tl-core` local profile ID. Access token must come from an authorized caller, never included in evidence files.
- `write_launch_evidence(path, version, java, pid, stdout, stderr)`: record nonsecret runtime facts, **always** set menu/world verification to false. A PID alone does not prove G3.

Not yet complete: trusted Windows Java auto-provisioning, old native handling and collisions, secure extraction review, executable GUI integration and end-to-end Singleplayer tests. This code does **not** establish that launch of real Minecraft works; no game runtime is available in this assistant's execution environment.

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

- G1: already accepted by Control for PR #12; **new branch CI still required**.
- G2: NOT_TESTED.
- G3: NOT_TESTED.
- G4: NOT_TESTED.

Do not mark G2/G3/G4 PASS without traceable real Windows execution evidence.
