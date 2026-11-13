# TLAUCHCRAFT — Chat 4 Modpack archive G1 acceptance

**Control review:** 2026-11-14
**PR:** [#10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10)
**Accepted HEAD:** `093aff365bc6faf946ee2c2dc03c9ce061c7d02a`
**Merge commit:** [305ba384003390f95103df92c409c7e50a5b99b0](https://github.com/1doiliemkhoet/tlauchcraft/commit/305ba384003390f95103df92c409c7e50a5b99b0)
**Windows CI:** [run 43484334378](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43484334378)

## Accepted (Windows G1)
- `tl-pack` registered in the seven-crate Windows Cargo workspace preserving `tl-core`, `tl-storage`, `tl-game`, `tl-loader`, `tl-lan`, `tl-skin`.
- Windows-generated authentic `Cargo.lock` committed; no CI gate weakened.
- CurseForge ZIP, Modrinth `.mrpack`, and Prism ZIP metadata parsing; explicit file plans, constrained archive entry names, size/bomb and duplicate checks, isolated overrides staging and non-ready import state.
- Fixed CurseForge canonical `projectID` / `fileID` field parsing and nested `modLoaders` handling. Added regression assertions for Forge loader identification.
- Windows workflow passed: registered workspace crates, `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace --all-targets`.
- **75 tests PASS / 0 failed** across workspace, including 13 archive tests for `tl-pack`.

## NOT accepted (functional gates)
- **G5 NOT_VERIFIED**: real modpack dependency download, Forge/Fabric/NeoForge install, file integrity checks for every required mod, Minecraft runtime launch and gameplay.
- CurseForge provider file IDs are not yet resolved through an authorized API. Parser/planner does not imply a runnable modpack.
- `tl-pack` archive inspection is not the complete runtime install pipeline. No unrestricted user-supplied URL downloading or arbitrary executable installer scripts are authorized.
- No claim of end-to-end launcher functionality, two-PC LAN, or installer G8.

## Next work
- Chat 7: integrate Desktop frontend with capability-gated real Rust commands; Windows npm/TypeScript/Tauri CI and clean-machine installer smoke later.
- Chat 2 and Chat 3: integrate real game+loader installation and E2E. Chat 4 to follow up on actual modpack install on a separate PR, coordinating downloads and installed file verification with Chat 2/3.
- Chat 1 hardening PR #18 and Chat 2 game E2E PR #19 remain independent pending reviews. Future feature work uses new branches from latest `develop`.
