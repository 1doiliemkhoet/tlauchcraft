# Control Acceptance — Chat 1 Foundation v1

**Date:** 2026-11-13
**Source:** [PR #13](https://github.com/1doiliemkhoet/tlauchcraft/pull/13)
**Merge:** [ba79eba1c1bbd10477c146aafa2f74b45c243255](https://github.com/1doiliemkhoet/tlauchcraft/commit/ba79eba1c1bbd10477c146aafa2f74b45c243255) on `develop`
**Reviewed source HEAD:** `29cd56d884f452752674038600f70fd95aa92e7a`

## Accepted scope
- `tl-core`: initial public typed API, local OfflineProfile model, GameInstance/LoaderSpec/InstanceState/errors/events, Windows-safe relative paths.
- `tl-storage`: SQLite version 1 metadata, profile/instance CRUD, reference restrictions, metadata-only removal (does not erase world files), SQLite backup and schema version guard.
- Root Rust 1.85 workspace, lockfile and Windows-only PR CI.

## Verification
- [Windows-only Actions run 43448481543](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43448481543) on latest source HEAD completed `success`:
  - `cargo fmt --all -- --check` — PASS
  - `cargo clippy --locked --workspace --all-targets -- -D warnings` — PASS
  - `cargo test --locked --workspace --all-targets` — PASS
- Historical run [43446780516](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43446780516) showed 12 passing Rust tests on Windows and Ubuntu. Ubuntu is not an official product acceptance target.
- Control reviewed exported Rust source and SQLite tests, including metadata round-trip, directory safety and backup behavior.

## Control verdict
- **G0 = ACCEPTED for Foundation v1**. This is a provisional stable data contract for integration; public changes require Control review and migration handling.
- **G1 = ACCEPTED for Foundation Windows CI only**, not for other crates.
- **G2–G8 = NOT_VERIFIED**. No real Minecraft installation, process, Singleplayer world, modpack, LAN or skin acceptance has been demonstrated by this PR.

## Known risks / follow-ups
- [#15](https://github.com/1doiliemkhoet/tlauchcraft/issues/15): register every next Rust crate in root workspace; enforce Windows CI per PR.
- [#16](https://github.com/1doiliemkhoet/tlauchcraft/issues/16): internal stable profile UUID is not equivalent to offline-mode server game UUID; renames can affect playerdata. Required before G6.
- [#17](https://github.com/1doiliemkhoet/tlauchcraft/issues/17): optimistic concurrency, backup path/symlink/partial failure hardening. Required before release.
- The current `InstanceState::Ready` type represents persisted state only; every importer must validate real dependencies before setting READY.

## Next integration wave
Chat 2 `tl-game` first; then Chat 3 loaders, Chat 4 modpack, Chat 5 LAN. Chat 6 skins can proceed in parallel with tested API. Chat 7 Tauri UI remains gated until real backend methods and Windows checks exist. Reports go to [LIVE_REPORTS.md](LIVE_REPORTS.md).
