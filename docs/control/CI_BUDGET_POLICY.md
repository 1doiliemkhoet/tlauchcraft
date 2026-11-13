# TLAUCHCRAFT — CI Budget Policy (Control)

**Effective:** 2026-11-13. **Goal:** minimize GitHub Actions minutes without weakening actual Windows/game E2E acceptance.

## Trigger policy

- Do not trigger builds on every feature-branch `push` **and** on its `pull_request`. Prefer PR-only checks to `develop`; an optional `push` to `develop` runs integration checks after merge.
- Use `paths` filters targeting the affected module/lockfiles/workflow; changes restricted to documentation and `docs/control/LIVE_REPORTS.md` must not run Rust/Node builds.
- Use a concurrency group scoped to PR number/ref with `cancel-in-progress: true` to stop superseded runs.
- Default PR build: one `ubuntu-latest` job, focused `fmt`, `clippy`, `cargo test` or Node unit/build for UI. No Windows/Linux matrix on every commit.
- Perform expensive Windows verification after merging code into `develop`, on explicit manual request, or before release. **Passing Linux-only tests is never a substitute for real Windows launch/E2E evidence.**
- Cache Rust/Node dependency caches safely. Keep step/job timeouts. Avoid automatically downloading entire Minecraft versions, modpacks or large asset sets into CI; real downloads and two-PC LAN E2E are controlled acceptance tests with evidence.
- Never spend CI minutes on live-report-only updates. Do not add individual overlapping workflows in seven feature branches without Control review.
- Require tracked tests on PR where applicable; skip only when the PR strictly changes docs. CI skips are not interpreted as tests passing.

## Initial applied optimization

Foundation workflow at `feat/tl-01-foundation:.github/workflows/rust.yml` adjusted to:
- PR trigger to `develop` with Rust paths filter (no duplicate feature push).
- Quick Ubuntu Rust checks on PR.
- Windows checks on integration push to develop or optional manual workflow trigger once available on the default branch.
- Concurrency cancellation, `permissions: contents: read`, cache and timeouts.

**Change commit:** https://github.com/1doiliemkhoet/tlauchcraft/commit/6035abc9fbe723da8e2ea527b68197d225f54ca9

## Required Control checks before merge/release

- When source changes: PR has passing relevant checks.
- Windows build/smoke evidence after integration for affected functionality.
- Actual Minecraft install/launch/world/modpack/LAN/skin evidence is separately tracked in `docs/control/LIVE_REPORTS.md`.
- If a workflow was skipped due to `paths`, label it `CI_SKIPPED_NOT_RUN` not PASS.
- For workflows using manual dispatch: GitHub typically requires the workflow file on the default branch before the manual UI appears; until then use integration push or a documented controlled local verification.

This policy optimizes *run triggers*, not test rigor, and does not modify repository-wide GitHub billing or spending limits.
