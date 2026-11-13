# TLAUCHCRAFT — Chat 6 Skin G1 Windows Acceptance

**Control review:** 2026-11-14
**PR:** [#11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11)
**Reviewed head:** `7afa96a00f866c62fd3c9a0d8341ff383b8d31dc`
**Merge commit:** [c60a743ce22cce190072b987145c0149cbd2e25e](https://github.com/1doiliemkhoet/tlauchcraft/commit/c60a743ce22cce190072b987145c0149cbd2e25e)
**Windows evidence:** [CI 43480924950](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43480924950)

## Accepted: G1 source/CI only
- `tl-skin` registered in the existing Windows Rust workspace, preserving `tl-core`, `tl-storage`, `tl-game`, `tl-loader`, and `tl-lan`.
- Offline PNG validation and reversible import/export, local skin catalog and selection persistence, Classic/Slim and legacy skin, pixel editor operations (fill, undo/redo, palette, mirror) and local preview state.
- Uses canonical Foundation `SkinSelection` and `SkinModel` instead of inventing duplicate profile/UUID types.
- GitHub Actions Windows job success on head: workspace-membership guard, `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace --all-targets`.
- Logs show **62 passed / 0 failed** across all crates, including **7 skin test cases**.

## Not verified
- **G7 NOT_VERIFIED:** actual custom skin rendering in a Minecraft instance and skin visibility to another person in LAN multiplayer.
- No in-game texture provider is installed/validated; previews are not E2E game proof.
- Actual client launch, pack compatibility, server reachability, two-machine test, cape provider and installer packaging have not been accepted by this PR.

## Next work
- Chat 2/3/5/6/7 integrate a real tested provider with the desktop UI and test Minecraft client + two PCs for G7. Keep separate feature branch/PR for further work.
- Chat 4 modpack must sync `develop` after this merge, retain `tl-skin` in root Cargo workspace and regenerate Cargo.lock using real Cargo before Windows CI G1 acceptance.
