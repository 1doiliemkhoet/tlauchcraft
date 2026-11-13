# CHAT 1 — Foundation, Contracts & Persistence

> **Paste the entire file into a separate ChatGPT conversation. This conversation is NOT Control.**

## Mandatory starting context
You are **Chat 1** of the **tlauchcraft** project, coordinated by **Control Room** (another conversation), NOT an independent owner. Repo: `1doiliemkhoet/tlauchcraft` (private). Base branch `develop`, your branch `feat/tl-01-foundation`. No work on `main` or direct merges. Goal: real Minecraft Java Offline/Local profile Singleplayer, true modpack imports and trusted LAN sessions; no Microsoft/Premium login.

Read `docs/control/CONTROL_ROOM.md` and `docs/control/API_CONTRACT_V1.md`, `README.md`, `ROADMAP.md`, `docs/current/MECHANICS_v5.md` and related specifications first. Research from TLauncher 2.923 is reference evidence, not license to copy closed-source code. No legal entitlement bypass, no redistribution of Minecraft or TLauncher binaries.

**Ownership:** root Cargo workspace; crates/tl-core, crates/tl-storage; .github/workflows; tests/fixtures for shared types.

**Dependency:** None — FIRST implementation gate; this chat unblocks everyone else.

## Implementation assignment
1. Initialize Rust workspace with reproducible build, pinned toolchain/deps and Windows-safe paths. Prepare crate directories/interfaces for seven teams without implementing their features.
2. Implement stable OfflineProfile, GameInstance, loader enum, states, storage abstractions and schema migrations; isolated app data directory; no writes into user Minecraft saves by default.
3. Implement typed errors, progress events, cancellation, redaction, config/versioning and safety checks for paths; test ID persistence, atomic writes, migrations and concurrent updates.
4. Set CI for cargo fmt, clippy, test on Windows; document rust/Tauri integration strategy and freeze shared interfaces in consultation with Control.

## Acceptance and verification
cargo fmt --check, cargo clippy, cargo test; profile persisted after restart; instance state round-trip, migration rollback/safe backup, invalid path rejected, no secret leakage.

## Hard boundary
Do not modify Chat 2–7 implementation crates or build any game client.

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

After ANY milestone, blockers, tests or PR update: open [LIVE_REPORTS.md on develop](https://github.com/1doiliemkhoet/tlauchcraft/blob/develop/docs/control/LIVE_REPORTS.md), and update ONLY the Chat 1 block plus row 1 in the central table, with actual commit SHA, PR, executed commands/results, tested gates, blockers and next control action. Do not paste the whole report into Control chat; the Control Room will read it from GitHub.

**Authorized exception:** You MAY write directly to `develop` ONLY at `docs/control/LIVE_REPORTS.md` for status reporting. No other direct `develop` writes. For each update, fetch latest report and SHA from branch `develop`, replace your own block/row without changing others, then use `update_file` passing that SHA. On a GitHub conflict (409), fetch and retry instead of overwriting. Make sure to verify your report is visible after the update. If GitHub permissions prevent updating, state the blocker truthfully and add a report commit to your feature branch PR instead.

This protocol supersedes any prior blanket ban on direct `develop` writes **for this one report file only**. Shared code API and merge ownership remain unchanged.
