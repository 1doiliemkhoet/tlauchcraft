# TLAUCHCRAFT — Control acceptance: Chat 2 Minecraft Core primitives

**Date:** 2026-11-13  
**PR:** [#12](https://github.com/1doiliemkhoet/tlauchcraft/pull/12)  
**Reviewed head:** `0b3c479eb8b373d971f6b545d1d6e5549e993f3a`  
**Merge commit:** [be4abef74029680fc9deb2783cdcbb565b6f69c0](https://github.com/1doiliemkhoet/tlauchcraft/commit/be4abef74029680fc9deb2783cdcbb565b6f69c0)  
**Windows CI:** [run #43459887778](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43459887778)

## Accepted scope (G1 only)
- `crates/tl-game` registered in root Rust workspace; generated `Cargo.lock` committed and pinned.
- Rust toolchain `1.88.0` and root `rust-version = 1.88` to match resolved transitive dependencies.
- Minecraft version metadata inheritance and rule/argument planning, explicit downloads plan, integrity-checked HTTPS downloader primitives, Java runtime/process helpers, typed adapter with `tl-core`.
- Real Windows GitHub Actions job finished `success`: workspace-membership guard, `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace --all-targets`.
- Unit/integration test results: **24 passed / 0 failed** (4 core, 12 game, 8 storage).

## Gate verdict
- **G0:** Foundation v1 previously accepted; Chat 2 adapter follows the shared types. Changes to public DTOs still require Control review.
- **G1:** ACCEPTED for Chat 2 **partial Game Core code and Windows CI**.
- **G2:** NOT_VERIFIED — no real licensed game client installation with files/hashes evidence.
- **G3:** NOT_VERIFIED — no real Minecraft process/menu evidence.
- **G4:** NOT_VERIFIED — no real Singleplayer save/reopen evidence.
- Loader integration, modpack installs, LAN, cosmetics and packaging also remain NOT_VERIFIED.

## Known limits / next work
- Only metadata/download/process primitives. Full version install pipeline, asset/native extraction, classpath, Java provisioning, on-device game startup, world lifecycle and diagnostics need further implementation and E2E.
- Do not interpret process spawning fixture or local unit tests as Minecraft game startup.
- Any unsupported launch rules or server/runtime authentication remain out of scope.
- Chat 3 can now integrate `tl-loader` against the `tl-game` and Foundation contracts on `develop`, by a PR with Windows workspace testing. See [integration gate #15](https://github.com/1doiliemkhoet/tlauchcraft/issues/15).
- Chat 2 continues functional E2E through [issue #2](https://github.com/1doiliemkhoet/tlauchcraft/issues/2) in a **new follow-up branch/PR**. PR #12 is merged; do not make unrelated commits on that merged head.

This evidence was reviewed directly against GitHub Actions job steps/logs. CI PASS does not establish real-game E2E.
