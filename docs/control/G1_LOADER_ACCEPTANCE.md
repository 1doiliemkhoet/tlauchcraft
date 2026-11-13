# TLAUCHCRAFT — Chat 3 Mod Loader planning acceptance (Control)

**Reviewed:** 2026-11-14
**PR:** [#8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8)
**Accepted head:** `8587c9dcb78515a8ab615fa235345ed7bf458592`
**Merged:** [1bd1ce8e7efa91df1e2a93aef36110f72ac59d8c](https://github.com/1doiliemkhoet/tlauchcraft/commit/1bd1ce8e7efa91df1e2a93aef36110f72ac59d8c)
**Evidence:** [Windows CI 43468262347](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43468262347)

## Accepted G1 scope
- `tl-loader` registered and compiled in Cargo workspace with Foundation `tl-core` and `tl-game`.
- Official Fabric client profile metadata validation and classpath planning.
- Forge / NeoForge catalog and installer-profile **inspection/planning only**.
- Bounded artifact coordinate, HTTPS/Maven host and supported metadata validation; basic JVM version gate.
- Windows workspace registration, `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace --all-targets` all SUCCESS.
- Run evidence: 35 tests passed, 0 failed across workspace.

## Not accepted
- No real loader installation, no Forge/NeoForge processor execution, no verified launcher/Java/game process, no Fabric mod-loading success.
- G2 actual installer/download, G3 game launch, G5 modpack runtime: **NOT_VERIFIED**.
- `ready_for_metadata_merge` denotes metadata only, never `GameInstance::Ready` or verified installed files.

## Integration follow-up
- Chat 4/5/6 PRs modify common root `Cargo.toml` and `Cargo.lock`; before merge, reconcile against new `develop` head and demonstrate Windows G1 tests with **all currently merged workspace crates**. Do not drop `tl-loader` from the workspace when merging another module.
- Loader execution integration and Windows actual Minecraft testing remain follow-ups with Chat 2 and Chat 4.
