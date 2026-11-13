# TLAUCHCRAFT — Windows-only CI Budget Policy

**Effective:** 2026-11-13. **Scope:** Windows desktop launcher for Minecraft Java. Windows is the sole supported product build/test target. Do **not** add Ubuntu/macOS CI matrices for tlauchcraft.

## CI budget rules

1. **PR only**: Run code checks on `pull_request` targeting `develop`; no duplicate feature push and PR events. Use `workflow_dispatch` for explicitly requested checks. Integration runs after merge may be triggered manually when needed.
2. **Only `windows-latest`** for GitHub-hosted CI. No Linux/Ubuntu/macOS GitHub Actions for this project. Do not add Ubuntu jobs as a fallback for cheaper builds.
3. **Path filters**: Rust only on `crates/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` or workflow changes; Desktop only on `apps/desktop/**`, UI lockfiles or its workflow. Documentation and `docs/control/LIVE_REPORTS.md` do not trigger builds.
4. **Cancel superseded runs**: `concurrency` by PR/ref and `cancel-in-progress: true`.
5. **Single Windows job, no matrix**: run fmt/clippy/unit tests for Rust; desktop uses its focused Windows TypeScript/test/build commands in a separate path-scoped workflow only if Control approves it. Never run full installer packaging on each PR commit.
6. **Bound costs**: Cache dependencies safely, enforce reasonable timeouts, do not download massive Minecraft/modpack binaries into CI, and never run continuous loops or automatic E2E on each commit.
7. **Correct evidence**: A skipped workflow is `CI_SKIPPED`, not PASS. Source unit tests are distinct from real Minecraft on-device E2E. Windows clean-machine checks, game/world/modpack/two-PC LAN and skin rendering remain manual/controlled acceptance gates.
8. **Local-first**: Developers run `cargo test`, `cargo fmt`, `cargo clippy`, npm tests and TypeScript build on Windows locally first, then push verified commits. Instructions should use 2–3 PowerShell commands per step.
9. **No silent cross-platform expansion**: Any Linux/macOS implementation or CI workflow requires a new explicit Control decision and scope change.

## Applied workflow

`feat/tl-01-foundation:.github/workflows/rust.yml` uses one `windows-latest` job on `pull_request` to `develop` (path-filtered), plus optional `workflow_dispatch`, without `push` or Ubuntu jobs. Change commit: https://github.com/1doiliemkhoet/tlauchcraft/commit/29cd56d884f452752674038600f70fd95aa92e7a.

This supersedes the earlier temporary Ubuntu-first CI policy. This only controls project CI, **not** whether a locally hosted Minecraft server may technically be run on other operating systems; official tlauchcraft target and E2E remain Windows.
