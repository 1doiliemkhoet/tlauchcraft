# TLAUCHCRAFT — CONTROL ROOM / Multi-chat Coordination

**Baseline:** 2026-11-13. **Repository:** https://github.com/1doiliemkhoet/tlauchcraft (private).
**Mode:** this chat = Control; seven other independent conversations = implementation teams.
**Current state:** documentation and contracts exist; no launcher source or passed E2E yet.

## Product intent (non-negotiable)
Build an actual Windows Minecraft Java launcher for **local/offline profiles**; Singleplayer, modpacks, and trusted LAN multiplayer. Import **CurseForge ZIP** and **Modrinth .mrpack** (P0), Prism ZIP (P1). Custom skins, 3D preview and supported offline cosmetic integration. **No Microsoft/Xbox/Premium sign-in module.**
A local profile does not itself grant a Minecraft license. Never distribute third-party EXE/JAR/game assets or bypass entitlement systems. Use legitimate sources and respect pack/mod licenses.

## Repository branches
- `main`: verified milestones/releases only, changed by Control.
- `develop`: integration branch for reviewed task PRs.
- Feature branches from `develop`: see task files below. **One chat = one branch**. Avoid direct pushes to `main` or `develop` by task chats.
- Use separate clones/worktrees if operating all chats on one physical machine. Never checkout a branch in another chat's working directory without consent.
- Each chat must say when waiting for shared contract implementation. Parallel research/tests can proceed but code should not silently invent incompatible types.

## Technology baseline / decisions
- Proposed implementation baseline: **Rust workspace** for core and **Tauri desktop shell with TypeScript/React** for Windows UI. No Tauri scaffold or binary exists yet.
- Chat 1 owns the initial workspace and **shared API/types**. Versions and toolchain are to be pinned in code and verified by CI; do not claim a specific version supported without testing.
- Workspace ownership: Chat 1 `crates/tl-core`, `crates/tl-storage`, root `Cargo.toml`, CI. Chat 2 `crates/tl-game`; Chat 3 `crates/tl-loader`; Chat 4 `crates/tl-pack`; Chat 5 `crates/tl-lan`; Chat 6 `crates/tl-skin`; Chat 7 `apps/desktop`.
- Cross-crate changes to public interfaces, schema, root config, or another team's code must be proposed to Control first. Do not let two chats update root manifests concurrently.

## Common requirements for EVERY task chat
1. Read `README.md`, `ROADMAP.md`, `docs/current/MECHANICS_v5.md`, applicable specs and `docs/control/API_CONTRACT_V1.md`. Treat reverse-engineered TLauncher 2.923 findings as **historical evidence**, not proof of the 2026 EXE.
2. Use only the specified branch; no merges to `main` or `develop`. No changes to secrets, user `.minecraft`, unrelated branches, or user saves.
3. Implement real, testable behavior. Mocks may be used for unit tests but **never substitute for E2E acceptance**. No fabricated test results.
4. Versioned typed errors, cancellation, retry, safe file operations, path validation, detailed logs and reproducible fixtures where applicable.
5. Keep tests hermetic unless explicitly marked integration; test downloads against safe fixtures. Tests must not use the user's real world or modpack directories destructively.
6. No bundled proprietary binaries, copied closed-source TLauncher code, credentials, personal data, redistribution of unlicensed game assets, or unauthorized mirror/scraping bypass.
7. Report exactly what was verified on real hardware and what remains blocked/unverified. Offline `online-mode=false` servers should be limited to a trusted LAN; do not expose them to the public Internet.
8. Give **2–3 Windows PowerShell commands per instruction step**, as the user prefers. Provide Linux commands only when relevant for server/tests.
9. Create commits on own feature branch, open PR targeting `develop`, report commit SHA, PR URL, code paths, tests, risks and required integration dependencies. **Control alone merges after review.**
10. Never assert `IMPLEMENTED`, `INSTALL_VERIFIED`, `LAUNCH_VERIFIED`, `WORLD_VERIFIED`, `MODPACK_VERIFIED`, `LAN_VERIFIED`, `SKIN_VERIFIED` without corresponding evidence.

## Evidence gates
- **G0 CONTRACT:** shared types/events/API documented, compatibility and owner reviewed.
- **G1 CODE:** builds, fmt/lint/unit/integration fixtures pass on supported Windows target.
- **G2 INSTALL:** actual downloaded/installed instance, SHA/metadata and logs from a permitted test source; recover from corruption.
- **G3 LAUNCH:** real Minecraft client launched, process PID, logs and main menu; no simulated Play.
- **G4 WORLD:** create/save/reopen Singleplayer world; preserve data, restore from safe backup.
- **G5 MODPACK:** real supported CurseForge ZIP/.mrpack imported into isolated instance, required mods present, game runs.
- **G6 LAN:** two actual PCs, matching modpack and loader, connect and interact, disconnect/reconnect, world and inventories persist.
- **G7 SKIN:** valid PNG preview, actual skin visible in Singleplayer, and separately verified peer visibility across LAN when provider supports it.
- **G8 RELEASE:** Windows installer/startup, update/rollback/error logs, clean PC smoke test.
Gates are **not all required for every chat**; their relevant owner must supply evidence. Control tracks cross-gate E2E.

## Dependency / merge order
Chat 1 shared contract+CI → Chat 2 executable Vanilla foundation → Chat 3 mod loaders → Chat 4 pack import to runnable instance → Chat 5 LAN E2E. Chat 6 can develop in parallel after Chat 1 and integrate with Chat 5 for LAN cosmetics. Chat 7 can build UI scaffolding after Chat 1 and only claim functional UI when real core events are wired.

## Control decisions
- **D-001:** User-facing scope is local/offline Singleplayer + modpacks + LAN, not premium sign-in.
- **D-002:** Imported pack = separate isolated instance; no silent writes to shared game/saves.
- **D-003:** Distinguish local skin preview, in-game display and peer visibility.
- **D-004:** Main is protected by process; all task chats PR into develop.
- **D-005:** Start P0 with Vanilla + one Fabric test combination, extend loader matrix with evidence.
- **D-006:** Core stack is a working baseline choice, not a claim a functioning Rust/Tauri app already exists.

## New chat workflow
Open a new ChatGPT conversation; paste the ENTIRE corresponding `docs/control/tasks/CHAT-0X.md`. This chat stays the Control Room. Track issues/PRs by ID and post completion reports here.

## Live reporting for all seven chats (mandatory)

**Single source of truth:** [LIVE_REPORTS.md on develop](https://github.com/1doiliemkhoet/tlauchcraft/blob/develop/docs/control/LIVE_REPORTS.md). Each chat MUST directly update **only its assigned Chat 0N block and corresponding status row** after each meaningful milestone, failed test, blocker or PR change. Write the exact executed test commands/results and evidence; use NOT_TESTED/NOT_VERIFIED for anything not run.

**Exception to no-direct-develop rule:** Reporting-only updates to `docs/control/LIVE_REPORTS.md` on `develop` are allowed. Do **not** push source code or any other file to develop. Before each update, read the latest version on `develop`, retain all other chats' content, and use GitHub Contents API optimistic-lock SHA (`fetch_file` then `update_file` with current `sha` and `branch="develop"`). On 409/conflict, re-fetch/reapply/retry; never force-push, overwrite, or reset the document. Chat edits to code remain in their respective feature branches with PR to develop. This file is a live status ledger and NOT proof of E2E success unless evidence is linked.

Control Room can retrieve the latest report with the connected GitHub integration, without asking the user to copy seven reports. This chat does not monitor GitHub in the background; Control reads the file when actively reviewing.

## Windows-only scope and CI policy (Control D-007)

**Decision D-007:** `tlauchcraft` officially targets **Windows desktop only**. CI, packaging and product E2E must use Windows; do not use Ubuntu/macOS runner matrices or claim Linux deliverables. Tests of local game/server components may be portable internally, but platform support remains Windows. All task chats obey [Windows-only CI Budget Policy](CI_BUDGET_POLICY.md). PR checks: path-filtered Windows single job, no duplicate push, concurrency cancellation; no CI for `LIVE_REPORTS.md`-only updates. Linux instructions and tests are optional developer references, never an acceptance requirement. This supersedes prior Ubuntu-first CI guidance.

