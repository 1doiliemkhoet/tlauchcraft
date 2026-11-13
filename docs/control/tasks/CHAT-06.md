# CHAT 6 — Offline Skin Manager, Editor & LAN Visibility

> **Paste the entire file into a separate ChatGPT conversation. This conversation is NOT Control.**

## Mandatory starting context
You are **Chat 6** of the **tlauchcraft** project, coordinated by **Control Room** (another conversation), NOT an independent owner. Repo: `1doiliemkhoet/tlauchcraft` (private). Base branch `develop`, your branch `feat/tl-06-skins`. No work on `main` or direct merges. Goal: real Minecraft Java Offline/Local profile Singleplayer, true modpack imports and trusted LAN sessions; no Microsoft/Premium login.

Read `docs/control/CONTROL_ROOM.md` and `docs/control/API_CONTRACT_V1.md`, `README.md`, `ROADMAP.md`, `docs/current/MECHANICS_v5.md` and related specifications first. Research from TLauncher 2.923 is reference evidence, not license to copy closed-source code. No legal entitlement bypass, no redistribution of Minecraft or TLauncher binaries.

**Ownership:** crates/tl-skin, skin test fixtures; coordinate UI widgets with Chat 7 and LAN hooks with Chat 5.

**Dependency:** Chat 1 offline profile and instance types. Game/loader provider experiments need Chat 2/3; LAN skin sharing integration needs Chat 5.

## Implementation assignment
1. Implement validated PNG 64x64/64x32 import/export, library, stable skin assignment to profile, Classic/Slim distinction and per-instance overrides.
2. Skin UV metadata/pixel editing model with layers, undo/redo and 3D preview data API; coordinate actual editor rendering with Chat 7.
3. Research/implement approved version-compatible offline skin provider; separate preview/local-in-game/LAN-visible states; no altering official game JARs.
4. Add opt-in LAN skin sharing or compatible provider adapter, optional custom cape only if supported; ensure friend sees skin with evidence from both PCs.

## Acceptance and verification
Skin PNG round-trip, invalid images, profile persistence, Classic/Slim transforms, actual Singleplayer skin screenshot, two-PC visibility distinct gate, consent off no sharing.

## Hard boundary
No Microsoft skin APIs, automatic third-party uploads or false claim that client preview implies peer skin.

## Shared rules
- Follow Control branch, ownership, typed API contracts, safety gates, Windows support and licence constraints. If a needed interface is missing, propose an explicit contract amendment to Control and proceed with isolated fixtures; do not rewrite another chat's module.
- Unit/integration mocks can support development, but **no simulated E2E PASS**. Never mark features as delivered without real execution traces, artifact hashes and appropriate on-device tests.
- Do not modify .env or secrets; do not delete evidence, user game data or worlds. Keep changes backwards compatible where possible.
- Give user **2–3 PowerShell commands per step**. Add Linux only when required.
- Implement and test on your own branch, commit/push and open PR targeting `develop`. **Control reviews and merges.**
- Report: architecture, files changed, design decisions, unit/integration/E2E commands + results, defects, pending dependencies, commit SHA, PR, and reproducible Windows instructions.

## First reply expected
Inspect the repo and current branch, confirm ownership and missing dependencies, then propose the smallest implementable slice with tests; **start coding** within owned paths when the prerequisite contract exists. Do not ask the user to re-describe the project.
