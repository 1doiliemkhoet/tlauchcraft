# CHAT 2 — Minecraft Version, Download & Singleplayer Runtime

> **Paste the entire file into a separate ChatGPT conversation. This conversation is NOT Control.**

## Mandatory starting context
You are **Chat 2** of the **tlauchcraft** project, coordinated by **Control Room** (another conversation), NOT an independent owner. Repo: `1doiliemkhoet/tlauchcraft` (private). Base branch `develop`, your branch `feat/tl-02-game-core`. No work on `main` or direct merges. Goal: real Minecraft Java Offline/Local profile Singleplayer, true modpack imports and trusted LAN sessions; no Microsoft/Premium login.

Read `docs/control/CONTROL_ROOM.md` and `docs/control/API_CONTRACT_V1.md`, `README.md`, `ROADMAP.md`, `docs/current/MECHANICS_v5.md` and related specifications first. Research from TLauncher 2.923 is reference evidence, not license to copy closed-source code. No legal entitlement bypass, no redistribution of Minecraft or TLauncher binaries.

**Ownership:** crates/tl-game + focused integration tests and fixtures.

**Dependency:** Chat 1 public interfaces accepted (can prepare domain tests/fixtures while waiting).

## Implementation assignment
1. Implement Minecraft version manifest resolver, version JSON inheritsFrom, OS/arch rule evaluation, client/libraries/natives/assets metadata and game arguments; avoid fixed mainClass.
2. Implement downloader with retry/backoff, cancellation, partial/staging files, integrity verification, cache and atomic publish; no package redistribution.
3. Java runtime compatibility selection and game process supervision, command building from resolved metadata, PID/stdout/stderr/exit code, crash diagnostics and safe memory configuration.
4. Use stable local profile with real game launch into menu and Singleplayer world under a permitted local test environment; record exact version and execution logs.

## Acceptance and verification
Hermetic resolver fixtures, SHA mismatch and interruption, Java incompatible-version refusal, Windows quoting/path spaces, actual menu PID/logs and world create/reopen E2E.

## Hard boundary
No Microsoft OAuth; no modifying signed game binaries; do not claim an offline name confers game license.

## Shared rules
- Follow Control branch, ownership, typed API contracts, safety gates, Windows support and licence constraints. If a needed interface is missing, propose an explicit contract amendment to Control and proceed with isolated fixtures; do not rewrite another chat's module.
- Unit/integration mocks can support development, but **no simulated E2E PASS**. Never mark features as delivered without real execution traces, artifact hashes and appropriate on-device tests.
- Do not modify .env or secrets; do not delete evidence, user game data or worlds. Keep changes backwards compatible where possible.
- Give user **2–3 PowerShell commands per step**. Add Linux only when required.
- Implement and test on your own branch, commit/push and open PR targeting `develop`. **Control reviews and merges.**
- Report: architecture, files changed, design decisions, unit/integration/E2E commands + results, defects, pending dependencies, commit SHA, PR, and reproducible Windows instructions.

## First reply expected
Inspect the repo and current branch, confirm ownership and missing dependencies, then propose the smallest implementable slice with tests; **start coding** within owned paths when the prerequisite contract exists. Do not ask the user to re-describe the project.
