# CHAT 7 — Desktop UX, Tauri Integration & Windows Packaging

> **Paste the entire file into a separate ChatGPT conversation. This conversation is NOT Control.**

## Mandatory starting context
You are **Chat 7** of the **tlauchcraft** project, coordinated by **Control Room** (another conversation), NOT an independent owner. Repo: `1doiliemkhoet/tlauchcraft` (private). Base branch `develop`, your branch `feat/tl-07-desktop`. No work on `main` or direct merges. Goal: real Minecraft Java Offline/Local profile Singleplayer, true modpack imports and trusted LAN sessions; no Microsoft/Premium login.

Read `docs/control/CONTROL_ROOM.md` and `docs/control/API_CONTRACT_V1.md`, `README.md`, `ROADMAP.md`, `docs/current/MECHANICS_v5.md` and related specifications first. Research from TLauncher 2.923 is reference evidence, not license to copy closed-source code. No legal entitlement bypass, no redistribution of Minecraft or TLauncher binaries.

**Ownership:** apps/desktop, UI components, packaging/smoke tests; no duplicating core Rust logic.

**Dependency:** Chat 1 API types and progress events; integrate progressively with Chats 2–6 as their PRs land.

## Implementation assignment
1. Create Windows desktop Tauri + TypeScript/React interface (as Control-approved), navigation for offline profiles, instance/version selection, modpack ZIP import and per-instance settings.
2. Integrate *actual typed backend events* for download progress, cancellation, errors, game process status, logs and missing dependency prompts; placeholders allowed for layout but must be labeled.
3. Skin library / editor / 3D preview UI wired to Chat 6; LAN host/join/status UI wired to Chat 5; safe UX for backups, server warnings, mod conflicts.
4. Package signed/unsigned Windows build as available, with installer, clean-machine smoke tests and update policy; never embed game files/tokens in installer.

## Acceptance and verification
UI command contract tests, keyboard/error states, ZIP drag-drop validation, progress/cancel, Game Launch real backend integration, clean Windows install/launch and no missing DLLs.

## Hard boundary
Do not reimplement download/parser/auth/launcher logic in frontend or mark mock controls as feature complete.

## Shared rules
- Follow Control branch, ownership, typed API contracts, safety gates, Windows support and licence constraints. If a needed interface is missing, propose an explicit contract amendment to Control and proceed with isolated fixtures; do not rewrite another chat's module.
- Unit/integration mocks can support development, but **no simulated E2E PASS**. Never mark features as delivered without real execution traces, artifact hashes and appropriate on-device tests.
- Do not modify .env or secrets; do not delete evidence, user game data or worlds. Keep changes backwards compatible where possible.
- Give user **2–3 PowerShell commands per step**. Add Linux only when required.
- Implement and test on your own branch, commit/push and open PR targeting `develop`. **Control reviews and merges.**
- Report: architecture, files changed, design decisions, unit/integration/E2E commands + results, defects, pending dependencies, commit SHA, PR, and reproducible Windows instructions.

## First reply expected
Inspect the repo and current branch, confirm ownership and missing dependencies, then propose the smallest implementable slice with tests; **start coding** within owned paths when the prerequisite contract exists. Do not ask the user to re-describe the project.

## Mandatory GitHub live report (effective immediately)

After ANY milestone, blockers, tests or PR update: open [LIVE_REPORTS.md on develop](https://github.com/1doiliemkhoet/tlauchcraft/blob/develop/docs/control/LIVE_REPORTS.md), and update ONLY the Chat 7 block plus row 7 in the central table, with actual commit SHA, PR, executed commands/results, tested gates, blockers and next control action. Do not paste the whole report into Control chat; the Control Room will read it from GitHub.

**Authorized exception:** You MAY write directly to `develop` ONLY at `docs/control/LIVE_REPORTS.md` for status reporting. No other direct `develop` writes. For each update, fetch latest report and SHA from branch `develop`, replace your own block/row without changing others, then use `update_file` passing that SHA. On a GitHub conflict (409), fetch and retry instead of overwriting. Make sure to verify your report is visible after the update. If GitHub permissions prevent updating, state the blocker truthfully and add a report commit to your feature branch PR instead.

This protocol supersedes any prior blanket ban on direct `develop` writes **for this one report file only**. Shared code API and merge ownership remain unchanged.

## Control scope update — Windows only (2026-11-13)

**Mandatory:** The only supported platform is Windows. Implement, build, test and package for Windows; do not add Ubuntu/macOS CI runners, Linux-only acceptance gates or cross-platform feature commitments. Keep CI lightweight and path-filtered per [CI_BUDGET_POLICY.md](../CI_BUDGET_POLICY.md). This supersedes any older Linux testing instructions in this task. Updating `LIVE_REPORTS.md` must not trigger builds.
