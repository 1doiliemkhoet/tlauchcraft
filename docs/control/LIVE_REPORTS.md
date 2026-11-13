# TLAUCHCRAFT — LIVE REPORTS (7 implementation chats)

> **Single live report.** Branch: `develop`. This file is the current report for Control Room. All seven feature chats update their own sections here; no need to paste reports into the Control chat.
>
> **Baseline snapshot:** 2026-11-13. Initial PR/branch references below are GitHub facts; build/test/functional statuses remain **UNVERIFIED** until each chat supplies evidence. This is not a CI dashboard.

## Reporting policy (mandatory)

1. Chat N edits **only** its `<!-- CHAT-0N:START -->` ... `<!-- CHAT-0N:END -->` block and its own row in the summary table. All other content is immutable to that chat.
2. **Only this one file** (`docs/control/LIVE_REPORTS.md`) is exempt from the rule against direct pushes to `develop`. All source code, CI and other docs still go by feature branch → PR → Control review → develop.
3. GitHub shared-file race prevention: fetch the **latest** complete file from `develop`, keep all others' blocks exactly unchanged, then update through GitHub Contents API using the latest blob SHA (`fetch_file` followed by `update_file` specifying `branch: develop`, `sha: fetched SHA`). On 409/stale SHA, **re-fetch the new file, reapply only your changes, retry**. Never overwrite another report or force-push.
4. Report after each milestone, PR/commit, actual test, blocker, failed check or important API change. Record exact commands and test counts; include links to GitHub commits, PRs and log artifacts. If not run, mark `NOT_TESTED`; if not checked by Control, mark `UNVERIFIED`.
5. Status vocabulary: `IN_PROGRESS`, `BLOCKED`, `PR_OPEN_UNREVIEWED`, `READY_FOR_CONTROL_REVIEW`, `CHANGES_REQUESTED`, `MERGED`, `E2E_VERIFIED`. **Only Control** can set `MERGED` or grant verification gates.
6. Report changes must not include secrets/tokens, personally identifying data, proprietary binaries, unlicensed Minecraft payloads or local machine paths exposing private information.
7. If no write access to GitHub is available, save the report in the chat's feature branch and alert Control; **never claim shared status was updated without re-reading it from develop**.

## Quick status

| Chat | Module | Status | PR | Chat tests | Control gate |
|---|---|---|---|---|---|
| [Chat 1](#chat-1--foundation--persistence) | Foundation & Persistence | **PR_OPEN_UNREVIEWED · #18 CI PASS** | [#18](https://github.com/1doiliemkhoet/tlauchcraft/pull/18) | Windows 30/30 tests; fmt/clippy PASS | Baseline G0/G1 ACCEPTED; follow-up NOT_VERIFIED |
| [Chat 2](#chat-2--minecraft-core--singleplayer) | Minecraft Core & Singleplayer | **G1 ACCEPTED · E2E P0 IN_PROGRESS** | [#19 (draft)](https://github.com/1doiliemkhoet/tlauchcraft/pull/19) | [Windows fmt, Clippy, 27/27 tests PASS](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43472405584) | **G2–G4 NOT_VERIFIED** |
| [Chat 3](#chat-3--mod-loader-adapters) | Mod Loader Adapters | **MERGED · G1 CONTROL_ACCEPTED** | [#8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8) | Windows 35/35 tests; fmt/clippy PASS | **G1 ACCEPTED · G2/G3/G5 NOT_VERIFIED** |
| [Chat 4](#chat-4--modpack-zip-import) | Modpack ZIP Import | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10) | NOT_TESTED | NOT_VERIFIED |
| [Chat 5](#chat-5--trusted-lan-multiplayer) | Trusted LAN Multiplayer | **IN_PROGRESS · PR_OPEN_UNREVIEWED** | [#9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9) | FMT PASS · CLIPPY FIX PUSHED · TEST PENDING | NOT_VERIFIED |
| [Chat 6](#chat-6--offline-skin-manager) | Offline Skin Manager | **PR_OPEN_UNREVIEWED · G1 TESTED PASS** | [#11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11) (Draft) | **Windows 31 PASS (7 skin)** | G1 awaiting Control; G7 NOT_VERIFIED |
| [Chat 7](#chat-7--desktop-ui--packaging) | Desktop UI & Packaging | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14) | NOT_TESTED (npm lock/toolchain blocker) | NOT_VERIFIED |

---

## Individual reports

<!-- CHAT-01:START -->
## Chat 1 — Foundation & Persistence

- **Baseline:** Foundation PR [#13](https://github.com/1doiliemkhoet/tlauchcraft/pull/13) MERGED into develop; Control approved G0/G1 for baseline only (2026-11-13). No further work on PR #13.
- **New branch:** `feat/tl-01-concurrency-identity` (created from `develop`).
- **Current HEAD:** [`1b7a0c8494c3e5e6d29ab204a610511ee96acf90`](https://github.com/1doiliemkhoet/tlauchcraft/commit/1b7a0c8494c3e5e6d29ab204a610511ee96acf90).
- **New PR:** [#18](https://github.com/1doiliemkhoet/tlauchcraft/pull/18) into `develop`, DRAFT/OPEN/UNMERGED, covering [Issue #17](https://github.com/1doiliemkhoet/tlauchcraft/issues/17) and Chat 1 contract scope of [Issue #16](https://github.com/1doiliemkhoet/tlauchcraft/issues/16).
- **Status 2026-11-14:** `IN_PROGRESS` / `PR_OPEN_UNREVIEWED`, source `IMPLEMENTED`, Windows CI `TESTED: PASS` on current head; G0/G1 new changes `NOT_VERIFIED` by Control.
- **Implemented:** SQLite schema v2 adds `revision INTEGER NOT NULL DEFAULT 0` to profiles and instances with transactional v1→v2 migration; versioned reads and conditional CAS updates reject stale revisions. Existing `update_profile`/`update_instance` methods use observed revisions to reject stale data, preserving serialized profile and instance formats. Safe SQLite backup uses staging file + `persist_noclobber` and checks backup parent symlink, refusing existing targets; no world data mutation.
- **Identity contract:** `tl_core::identity` declares separate `profile_id`, `effective_game_uuid`, `offline_server_uuid`; vanilla offline-mode MD5/v3 algorithm over case-sensitive `OfflinePlayer:<username>`. `preview_rename_impact` flags world backup and potential inventory change; original `OfflineProfile` JSON remains unchanged. Algorithm requires Chat 5 real-game validation; this is not G6 evidence.
- **Changed paths:** `Cargo.toml`, `Cargo.lock`, `crates/tl-core/Cargo.toml`, `crates/tl-core/src/lib.rs`, `crates/tl-core/src/identity.rs`, `crates/tl-storage/Cargo.toml`, `crates/tl-storage/src/lib.rs`, `crates/tl-storage/tests/storage.rs`, `docs/foundation/OFFLINE_IDENTITY_V1.md`. Temporary formatter/lock generator workflow was deleted from feature branch.
- **Tests added:** two SQLite connections stale profile write, old API stale profile/instance writes, v1 schema/records retained after upgrade, backup refuses existing symlink, identity sample fixture, rename impacts and username case behavior. Final Windows CI: **30/30 passed, 0 failed** (7 core, 12 game, 11 storage).
- **CI final verified:** [Windows workflow run 43471222988](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43471222988) on commit `1b7a0c8494c3e5e6d29ab204a610511ee96acf90`, overall **SUCCESS**. `cargo fmt --all -- --check` PASS; `cargo clippy --locked --workspace --all-targets -- -D warnings` PASS; `cargo test --locked --workspace --all-targets` **30 passed / 0 failed** (tl-core 7, tl-game 12, tl-storage 11). Earlier failed Windows run [43470179160](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43470179160) identified missing `instances.revision` on new schema; fixed in [`1459ba6`](https://github.com/1doiliemkhoet/tlauchcraft/commit/1459ba6484bb74a2ecd62c69afc55a636089eddb). Follow-up fixed stale CAS check not refreshing expected revision; latest fmt/clippy/test PASS. No full launcher E2E evidence.
- **E2E / gates:** No real client/server/world/playerdata tests by Chat 1. G0/G1 from accepted PR #13 remain Control-accepted baseline; current PR #18 `NOT_VERIFIED`. G2–G8 `NOT_TESTED` here.
- **Risks / dependencies:** Chat 5 must test authoritative server UUID, two Windows users, reconnect and playerdata/inventory with backups; Chat 2 must check launch UUID args. Contract must be approved by Control. Offline-mode must remain trusted LAN only. Parent reparse-point path race on Windows still requires review; backup symlink checks do not eliminate malicious concurrent parent replacement.
- **Coordination:** [Issue #16 comment](https://github.com/1doiliemkhoet/tlauchcraft/issues/16) specifies proposed shared mapping and requests Chat 5/Chat 2 validation.
- **Next:** Control reviews PR #18, identity mapping and security residual risks; Chat 5 must verify real LAN/playerdata, Chat 2 launch UUID contract. No self-merge.

<!-- CHAT-01:END -->

---

<!-- CHAT-02:START -->
## Chat 2 — Minecraft Core & Singleplayer

- **Control-accepted prior stage:** [PR #12](https://github.com/1doiliemkhoet/tlauchcraft/pull/12) MERGED to `develop` (merge `be4abef74029680fc9deb2783cdcbb565b6f69c0`). G1 Windows accepted, 24/24 tests + fmt + Clippy [PASS](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43459887778). Historical phase is CLOSED.
- **Current P0 branch:** `feat/tl-02-game-e2e` created directly from `develop` at `fbbe4e6aeee5b3745621cef915d27305433e3243`.
- **Current HEAD:** [`c261455a3be1baded4b1e7b2be80616e003107ae`](https://github.com/1doiliemkhoet/tlauchcraft/commit/c261455a3be1baded4b1e7b2be80616e003107ae)
- **PR:** [#19 — Draft/Open/NOT MERGED](https://github.com/1doiliemkhoet/tlauchcraft/pull/19). No source changes on `develop`.
- **Current state:** `P0_IMPLEMENTATION_IN_PROGRESS`; **Windows new-branch CI PASS (27/27 tests)**; G2/G3/G4 `NOT_TESTED` / `NOT_VERIFIED`. Running Minecraft on real Windows hardware remains required.
- **Updated:** 2026-11-14 (Chat 02, Windows CI evidence).\n- **Changed paths:** `crates/tl-game/src/vanilla.rs` (official-release metadata, metadata-SHA1, verified downloads, asset index/objects, native archive listing and extraction, Java version probe, classpath and launch substitutions, nonsecret runtime evidence); `crates/tl-game/src/lib.rs` (module registration); `crates/tl-game/examples/vanilla_e2e.rs` (Windows manual installer/launch harness producing PID/stdout/stderr); `docs/control/CHAT02_GAME_E2E_CHECKLIST.md` (real-game acceptance).
- **Security/data:** No Minecraft or Java binaries committed/distributed; official HTTPS allowlist and SHA1 enforced for downloads. Offline test harness supports an optional local token placeholder without requiring Microsoft login. Refuses overwrite of conflicting existing files. Persisted `tl-core` profile ID is not assumed to equal game UUID. Access token supplied via environment and excluded from evidence file. Native extraction path scanning is experimental; link/collision security review pending.
- **Windows CI status:** [Latest run 43472405584](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43472405584), PR HEAD `c261455a`, job `132047495916`: workspace membership **PASS**, `cargo fmt --all -- --check` **PASS**, `cargo clippy --locked --workspace --all-targets -- -D warnings` **PASS**, `cargo test --locked --workspace --all-targets` **PASS**. Actual suite counts 4 + 15 + 0 + 0 + 8 = **27 tests passed, 0 failed**. Overall Windows job **SUCCESS**. Earlier format-only failures repaired without changing `Cargo.lock` or disabling CI.
- **Windows commands:** `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace --all-targets`. No CI bypasses or Cargo.lock changes made.
- **Evidence:** No real game run available in this ChatGPT environment; no Windows hardware game PID/menu screenshot, successful G2 install, or Singleplayer save/reopen proof. G3/G4 remain NOT_TESTED.
- **Gates:** prior G1 ACCEPTED; new-branch Windows fmt/Clippy/unit tests `PASS` (27/27). **G2 real file installation NOT_TESTED**, **G3 actual Minecraft menu NOT_TESTED**, **G4 Singleplayer save/reopen NOT_TESTED**. No PID/screenshots provided.
- **Dependencies/risks:** Windows Java compatible runtime and authorized Minecraft account/session supplied by tester; manual real Windows E2E via example, log/screenshots/world re-open; extractor audit and native compatibility.
- **Next:** Run `cargo run --locked --release -p tl-game --example vanilla_e2e` with parameters as documented in `docs/control/CHAT02_GAME_E2E_CHECKLIST.md` on an authorized real Windows test machine, collect install SHA/log/PID/menu screenshots and world reopen proof, then record G2/G3/G4 outcome. Native archive extraction remains experimental and needs hardening/review. **No self-merge.**

<!-- CHAT-02:END -->

---

<!-- CHAT-03:START -->
## Chat 3 — Mod Loader Adapters

- **Branch:** `feat/tl-03-loaders`
- **Head:** [`8587c9dcb78515a8ab615fa235345ed7bf458592`](https://github.com/1doiliemkhoet/tlauchcraft/commit/8587c9dcb78515a8ab615fa235345ed7bf458592)
- **Issue:** [#3](https://github.com/1doiliemkhoet/tlauchcraft/issues/3)
- **PR:** [#8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8), OPEN DRAFT, **NOT MERGED**
- **Status:** `READY_FOR_CONTROL_REVIEW` for **G1 code and unit tests only**. **G2 INSTALL / G3 LAUNCH / G5 MODPACK NOT_VERIFIED.**
- **Updated:** 2026-11-13 (Chat 3, after Windows CI).
- **IMPLEMENTED:** registered `crates/tl-loader` in root Cargo workspace; generated and committed authentic `Cargo.lock` using Cargo 1.88 on Windows Actions. Reuses shared `tl_core::{LoaderKind, LoaderSpec, GameInstance}` (no duplicate shared types) and `tl_game::inherit` through `merge_fabric_version`. Fabric official profile metadata/classpath resolution, Java major checking, official Maven origin validation; Forge installer metadata/acquisition planning, processor-profile inspection; NeoForge metadata catalog and fail-closed compatibility handling. **No verified installer or actual modded game launch.**
- **Changed paths:** `Cargo.toml`, `Cargo.lock`, `crates/tl-loader/Cargo.toml`, `crates/tl-loader/src/lib.rs`, `crates/tl-loader/src/fabric.rs`, `crates/tl-loader/src/maven.rs`, `crates/tl-loader/src/installer.rs`, `crates/tl-loader/tests/metadata.rs`. Standard CI workflow restored unchanged after diagnostic.
- **Proof of Cargo-generated lock:** [Diagnostic Windows run #43467527310](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43467527310), real `cargo generate-lockfile` success, source output captured and committed; no hand-authored lock content.
- **Windows CI PASS:** [**Run #43468262347**](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43468262347), HEAD `8587c9d`: `cargo metadata --locked` workspace registration **PASS**; `cargo fmt --all -- --check` **PASS**; `cargo clippy --locked --workspace --all-targets -- -D warnings` **PASS**; `cargo test --locked --workspace --all-targets` **PASS (35 passed, 0 failed across workspace)**. All test counts obtained from job logs (4 + 12 + 4 + 7 + 0 + 8).
- **Real game E2E:** **NOT_TESTED/NOT_VERIFIED**. No actual loader-installed artifact hashes, Minecraft process PID or Fabric mod startup evidence. 11 Chat 3 fixture tests do not substitute for E2E.
- **Acceptance gates:** G0/G1 awaiting Control grant (CI code evidence available); G2/G3/G5 NOT_VERIFIED.
- **Remaining gaps:** Chat 2 FileInstaller currently restricts sources to Mojang domains and requires SHA1 metadata. Actual official Fabric/Forge/NeoForge Maven artifact acquisition needs safe reviewed host/integrity extension. Forge/NeoForge Java installer processors, staging/rollback and real game smoke tests not implemented.
- **Control decisions requested:** approve G1 only, decide official loader artifact origin + integrity API extension, select real Fabric/Minecraft/Java/mod E2E matrix. **No auto-merge.**
- **Next:** integrate approved verified downloader and official installers, verify Minecraft mod startup on Windows before G2/G3/G5 claim.

- **Control decision 2026-11-14:** PR #8 merged into `develop` (`1bd1ce8e`), G1 Windows metadata-planning ACCEPTED ([CI 35/35](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43468262347)). No actual loader installation/launch or Forge processor execution verified (G2/G3/G5 NOT_VERIFIED). Previous Chat 3 status above is historical.

<!-- CHAT-03:END -->

---

<!-- CHAT-04:START -->
- **2026-11-14 Control follow-up (PR #10, no new source commit):** Rechecked current `develop` tree: Foundation/Minecraft Core plus `crates/tl-loader` from PR #8; root workspace includes `tl-core`, `tl-storage`, `tl-game`, `tl-loader`. Chat 4 feature branch at `08532c9772d97cf20af19e6a0cfe8352efdeac6e` still includes `tl-pack` but has not incorporated `tl-loader`. Current `develop/Cargo.lock` contains `tl-loader` and not `tl-pack`. Required target workspace is **all five crates**. Current environment: Linux, `cargo` and `rustc` not installed, no Windows runner execution interface available; therefore sync/lock regeneration, Windows fmt/clippy/tests and CI PASS **NOT_EXECUTED / BLOCKED**. Control requested no repeated trial commits; intentionally did **not** push a known stale-lock workspace merge to PR #10. Request Control to perform Windows checkout of feature branch, merge latest `develop`, retain all five members, run actual Cargo lock regeneration and `--locked` checks, push one validated commit, and supply CI proof. No modpack READY or gate PASS asserted. PR remains Draft and not merged.

- **2026-11-14 recheck:** Control requested Windows Cargo regeneration and locked workspace checks. Verified PR #10 remains Draft/Open at `08532c9772d97cf20af19e6a0cfe8352efdeac6e`; latest CI run `43470699059` still FAILED after Windows fmt PASS, at Clippy `--locked` because `Cargo.lock` is stale; tests skipped. Execution environment has no Cargo and is not Windows, so Windows `cargo generate-lockfile`, Clippy, tests, commit/push of lockfile are **BLOCKED / NOT_EXECUTED**. Reproduction on a real Windows checkout: `cargo generate-lockfile`, `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace --all-targets`; commit Cargo.lock only after the commands succeed. No new PASS, commit, or CI evidence claimed.

## Chat 4 — Modpack ZIP Import

- **Branch:** `feat/tl-04-modpack`
- **HEAD:** [`08532c9772d97cf20af19e6a0cfe8352efdeac6e`](https://github.com/1doiliemkhoet/tlauchcraft/commit/08532c9772d97cf20af19e6a0cfe8352efdeac6e)
- **PR:** [#10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10), OPEN / DRAFT, **not merged**.
- **Status:** `IN_PROGRESS` / `BLOCKED` (locked Cargo + Windows CI). Source `IMPLEMENTED`, runnable modpack E2E `NOT_VERIFIED`.
- **Updated at:** 2026-11-14.
- **Implemented:** Synced `develop` Foundation and Minecraft Core into own branch with merge commit [`2e4abff`](https://github.com/1doiliemkhoet/tlauchcraft/commit/2e4abffb4930ccbbc3793d518c6ca080aea85d95), retained own pack code. Registered `crates/tl-pack` in root `Cargo.toml`. CurseForge/Modrinth/Prism archive inspector + typed file plan; Windows path traversal/case aliases, excessive counts/sizes/compression ratio, corrupt ZIP, changed ZIP SHA check; safe isolated override staging, Modrinth client-overrides precedence, cleanup of failed stage. Staging is **not** a READY runnable instance. Appended 5 security/rollback regression tests (13 fixtures total), used Windows CI rustfmt diffs to format changed sources.
- **Changed paths:** `Cargo.toml` (workspace registration); `crates/tl-pack/Cargo.toml`, `src/lib.rs`, `tests/archive.rs`, `README.md`. Retained Foundation/Minecraft Core from latest merged `develop`; no cross-team source modifications.
- **Windows CI observed:** [run 43470122867](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43470122867) `FAIL`: workspace membership step `PASS`, `cargo fmt --all -- --check` `FAIL`; Clippy/tests `SKIPPED`. [run 43470206054](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43470206054) `FAIL` at format stage. Rustfmt changes pushed thereafter. **Final HEAD Windows run [43470699059](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43470699059): workspace registration PASS; `cargo fmt --all -- --check` PASS; `cargo clippy --locked --workspace --all-targets -- -D warnings` FAILED before compilation because `Cargo.lock` needs updating; tests SKIPPED.** No local Windows Cargo executed in this session.
- **Cargo.lock:** existing develop lock **does not include** `tl-pack`, `zip`, `sha2`, `hex`; must run genuine Windows Cargo generation and commit lockfile. Do not manually fabricate lock metadata. `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo test --locked --workspace --all-targets` **NOT_TESTED** at final HEAD.
- **E2E:** No real CurseForge/Modrinth installation or Minecraft launch verified. G1/G2/G3/G5: **NOT_VERIFIED** (fmt is not G1 acceptance).
- **Dependencies / blockers:** Windows Cargo lock regeneration; Chat 2 `FileInstaller` downloader, SHA verification, HTTPS/SSRF safeguards; Chat 3 `LoaderInstaller`; Chat 1 persistence contract integration. CurseForge requires authorized API resolution; never fabricate mod downloads or mark READY. Durable lockfile, dependency graph, robust publication/cancel/recovery and real launch pending.
- **Control decisions:** Confirm ownership of root workspace+lock change and loader/download adapters; enable Windows Cargo environment to regenerate lock before PR review. No merge or G1/G5 approval until CI evidence.
- **Windows next commands:** `cargo generate-lockfile`; `cargo fmt --all`; `cargo clippy --locked --workspace --all-targets -- -D warnings; cargo test --locked --workspace --all-targets` (execute individually to catch exact failures).

<!-- CHAT-04:END -->

---

<!-- CHAT-05:START -->
## Chat 5 — Trusted LAN Multiplayer

- **Branch:** `feat/tl-05-lan`; source changes remain in feature branch, not pushed to `develop`.
- **Branch HEAD:** [`245e96f4a23ff737cfb39cb17fd471873bd62f1a`](https://github.com/1doiliemkhoet/tlauchcraft/commit/245e96f4a23ff737cfb39cb17fd471873bd62f1a).
- **Issue:** [#5](https://github.com/1doiliemkhoet/tlauchcraft/issues/5); offline identity coordination [#16](https://github.com/1doiliemkhoet/tlauchcraft/issues/16) with [Chat 5 comment](https://github.com/1doiliemkhoet/tlauchcraft/issues/16#issuecomment-6860186854) (comment link ID may be connector-specific).
- **PR:** [#9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9) OPEN/DRAFT, NOT MERGED.
- **Status:** `IN_PROGRESS`, `PR_OPEN_UNREVIEWED`; G1/G6 not approved.
- **Updated at:** 2026-11-14 (Chat 5, CI follow-up).
- **Code / changes:** Synced branch with `develop` by merge commit `b288c1d`, included `tl-lan` in root `Cargo.toml` workspace (`a688488`), aligned workspace crate entry in `Cargo.lock` (`5e8e63e`; **edited without running cargo; --locked verification still required**). `crates/tl-lan/src/lib.rs`: trusted LAN server config, safe private bind, server startup requires offline-safe properties/EULA, PID/logs, graceful stop, path-safe cold world backup, gameplay SHA256 fingerprint comparison (hashes supplied by caller), name stability guard, new bounded private TCP reachability probe; 20 Windows-oriented hermetic unit tests authored (including server lifecycle rejection, mod mismatch, backup bytes/inventory fixture, connection refusal and port conflict). `crates/tl-lan/G6_WINDOWS_E2E.md`: real two-PC Windows test plan and evidence gate. `crates/tl-lan/README.md`: contract and integration notes. Rust formatting fixes from Windows CI log committed `8ed09db`.
- **Changed paths:** `Cargo.toml`, `Cargo.lock`, `crates/tl-lan/Cargo.toml`, `crates/tl-lan/src/lib.rs`, `crates/tl-lan/README.md`, `crates/tl-lan/G6_WINDOWS_E2E.md`.
- **CI (observed evidence):** Run [43473121205](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43473121205) failed formatting. Later Windows run [43473706394](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43473706394) **PASS workspace inclusion and `cargo fmt --all -- --check`**, then **FAIL Clippy** on `manual_range_contains` and `question_mark`; tests skipped. Both Clippy findings fixed in [`245e96f`](https://github.com/1doiliemkhoet/tlauchcraft/commit/245e96f4a23ff737cfb39cb17fd471873bd62f1a). New [Windows run 43476043990](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43476043990) is IN_PROGRESS at report time. **No claim of Clippy/test PASS.**
- **Windows commands to reproduce:** `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace --all-targets`.
- **E2E reality:** `NOT_TESTED` — no two actual PCs, game handshake, mod interaction, reconnect, playerdata/inventory or world persistence evidence; socket fixtures are not Minecraft server E2E.
- **Identity design (#16):** Chat 1 owns immutable internal profile UUID and distinct game/offline server identity contract. Chat 5 blocks username renames; does NOT migrate inventory, force server UUID, or equate app ID and playerdata UUID. Need observed server playerdata UUID and two-PC persistence evidence after Chat 1 contract.
- **Dependencies/risks:** Chat 1 typed mapping; Chat 2 runnable game and process contract; Chat 3 Fabric/Forge/NeoForge dedicated-server recipes; Chat 4 verified lockfile/hash/side semantics; Chat 7 UI. Offline nicknames are spoofable; no public bind, firewall changes or port forwarding. `java -jar` is insufficient for some modded launch recipes.
- **Control decisions:** Approve server-scoped identity mapping/rename migration and loader-specific server launch interface; review CI once green. G0/G1 `UNVERIFIED`, G6 `NOT_TESTED`; no merge.
- **Next:** Inspect CI run 43476043990, fix additional Clippy or unit-test failures if present, update exact Windows test counts. G6 two-PC remains NOT_TESTED.

<!-- CHAT-05:END -->

---

<!-- CHAT-06:START -->
## Chat 6 — Offline Skin Manager

- **Branch:** `feat/tl-06-skins`; **HEAD:** `7ffec58cf7c13d809b3a8da0855374899977a45c` (2026-11-14 Vietnam). **Issue:** [#6](https://github.com/1doiliemkhoet/tlauchcraft/issues/6). **PR:** [#11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11) into `develop`, OPEN/DRAFT, NOT MERGED.
- **Status:** `PR_OPEN_UNREVIEWED`, `IN_PROGRESS`; **G1 Windows code checks: TESTED/PASS by GitHub Actions; Control acceptance still NOT_VERIFIED**. Minecraft Singleplayer and LAN skin E2E NOT_TESTED.
- **Foundation synchronization:** merge commit `01eb78e7b6ccd76d23d9ead9820cf07dccb15aea` merges Foundation `develop` into feature branch. Recent report-only commits on `develop` may advance it further; no source conflicts identified.
- **Implemented:** `tl-skin` registered in root `Cargo.toml`; real Cargo 1.88-generated `Cargo.lock`; `tl-core::SkinSelection`/`SkinModel` rather than duplicate shared types; binding with stable `OfflineProfile.id`/`skin_id` (caller must persist profile via `tl-storage`); validated PNG 64×64 and 64×32, SHA-256, library import/export/rename/duplicate/delete, Classic/Slim, persistent profile/instance appearance choices; pixel paint/erase/fill/mirror/sample/palette/undo/redo and UV metadata; optional consent-gated bounded LAN PNG payload, no transport.
- **PR changed paths:** `Cargo.toml`, `Cargo.lock`, `crates/tl-skin/Cargo.toml`, `crates/tl-skin/src/lib.rs`, `crates/tl-skin/tests/skin_tests.rs`, `crates/tl-skin/README.md`, `docs/control/tasks/CHAT-06-IMPLEMENTATION-NOTES.md`. Temporary workflow `chat6-g1.yml` was deleted after validation to avoid duplicate CI.
- **Windows test evidence (real runner, Rust 1.88):** [Rust Foundation run #43475028477](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43475028477): **SUCCESS**. Workspace registration PASS; `cargo fmt --all -- --check` PASS; `cargo clippy --locked --workspace --all-targets -- -D warnings` PASS; `cargo test --locked --workspace --all-targets` PASS. [Chat 6 run #43474516602](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43474516602): **SUCCESS**, test groups 4 + 12 + 0 + 7 + 0 + 8 = **31 passed, 0 failed**, of which **7 tl-skin tests**; logs independently inspected. Generated lockfile via `cargo generate-lockfile` on Windows (commit `802a5d17`). Early CI failures: missing formatted files and 3 Clippy lints; fixed, with successful reruns.
- **Skin test categories:** PNG 64×64/64×32 roundtrip, invalid dimensions/corrupt/oversized data, library and persistence on reopen, stable shared UUID profile selection + Slim, instance override, paint/sample, fill/mirror, undo/redo, consent-off blocks transfer and preview/local/peer distinct states. These are unit tests, not Minecraft runtime evidence.
- **Gates:** G1 Windows build/fmt/clippy/unit **TESTED PASS, awaiting Control sign-off**. G7 launcher 3D preview NOT_TESTED; skin rendered in actual Minecraft NOT_TESTED; two-PC LAN visibility NOT_TESTED. Other acceptance gates NOT_VERIFIED.
- **Coordination:** GitHub issue comments sent to Chat 2 (prelaunch provider hook), Chat 3 (Fabric compatibility matrix), Chat 5 (opt-in LAN transport/peer hash/cache) and Chat 7 (3D UI and distinct visibility statuses), via Control.
- **Blockers / pending:** functional 3D preview renderer, complete 64×32 anatomical UV conversion, custom cape storage/provider, version+loader compatible Minecraft skin mod integration, trusted-LAN transfer, two real Windows PCs and screenshot evidence. Local library metadata and test-provided evidence strings never equal actual game/peer verification. Offline UUID/username can be spoofed.
- **Control decisions:** approve profile persistence transaction boundary, version-aware provider integration with Chat 2/3 and LAN payload protocol with Chat 5; Control alone reviews and merges PR #11. Keep PR Draft until integration review; do not label G7 SKIN_VERIFIED.
- **Next actions:** real supported Fabric/Vanilla Singleplayer skin adapter with Chat 2/3; UI 3D preview with Chat 7; peer visibility E2E with Chat 5.

<!-- CHAT-06:END -->

---

<!-- CHAT-07:START -->
## Chat 7 — Desktop UI & Packaging

- **Branch:** `feat/tl-07-desktop`
- **Latest branch HEAD:** [`04cfc6dc913008b4659e751730741aebdca6b21f`](https://github.com/1doiliemkhoet/tlauchcraft/commit/04cfc6dc913008b4659e751730741aebdca6b21f)
- **PR:** [#14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14) — OPEN DRAFT, NOT MERGED. **Issue:** [#7](https://github.com/1doiliemkhoet/tlauchcraft/issues/7)
- **Status:** `IN_PROGRESS`, `PR_OPEN_UNREVIEWED`. Source `IMPLEMENTED` (unverified build); Windows tests `NOT_TESTED`.
- **Updated:** 2026-11-14 (Chat 7)
- **Foundation/Game Core sync:** Merged current `develop` `fb89c40cf33403323bfbae57a68e15716d74079d` into Chat 7 at [`7f347e63740f918bb620e4e374bb9fe8011efdda`](https://github.com/1doiliemkhoet/tlauchcraft/commit/7f347e63740f918bb620e4e374bb9fe8011efdda) (2 parents, no source modifications on develop). Includes `tl-core`, `tl-storage`, `tl-game`.
- **Implemented:** Tauri 2 + React/TypeScript scaffold; real Rust `profile_list`, `profile_create`, `instance_list` using `tl-core` Rust types, `tl-storage` SQLite, platform local data directory; `desktop_capabilities` only advertises profiles and instances. Fixed snake_case serde enum mismatch (`ready`, `vanilla`, `neo_forge`) and progress event contract_version validation. Native modpack picker, typed progress handlers, logs, disabled/informational LAN/Skin/Play/Import navigation; no fake success. Added separate desktop Cargo workspace and Windows ICO resource. Windows NSIS config present but NOT BUILT.
- **Changed paths:** `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src-tauri/icons/icon.ico`, `apps/desktop/src/backend.ts`, `apps/desktop/src/backend.test.ts`, `apps/desktop/src/App.tsx`, `apps/desktop/README.md`. No files owned by other feature chats modified.
- **Test commands / actual results:** `npm ci`, `npm test`, `npm run build`, `npm run tauri dev`, `npm run tauri build`: **NOT_TESTED on Windows**; cannot claim PASS. Authoring Linux container Node 22/npm 10 present, Rust toolchain absent; npm registry reachability probe `npm view vite version --fetch-timeout=5000 --fetch-retries=0` **FAILED** (`EAI_AGAIN`); `package-lock.json` not generated, so `npm ci` is blocked until generating real dependency lockfile on networked Windows and committing it. No Windows E2E log, installer output or artifact hashes.
- **Tests authored:** Capability fail-closed, malformed negotiation, command arguments, backend errors, supported ZIP/MRPACK suffix, percent clamping and snake_case wire type checks. **NOT_RUN**, not evidence of G1.
- **Evidence:** GitHub merge + feature commits [`d954e1360b0b6826ca9a9e40118d5b5bb62b2332`](https://github.com/1doiliemkhoet/tlauchcraft/commit/d954e1360b0b6826ca9a9e40118d5b5bb62b2332), [`04cfc6dc913008b4659e751730741aebdca6b21f`](https://github.com/1doiliemkhoet/tlauchcraft/commit/04cfc6dc913008b4659e751730741aebdca6b21f); no G1–G8 device evidence.
- **Gates:** G0 API v1 types integrated in desktop, final command surface unapproved; G1–G8 **NOT_VERIFIED**.
- **Blockers:** No npm package-lock; cannot run npm ci or compile Windows in current environment. Chat 2 currently provides launch primitives, not full safe client install/launch facade; Chats 4/5/6 needed for real modpack/LAN/skin. Core storage uses local SQLite; no destructive world changes.
- **Control decisions:** Approve command name/error/event facade and sequencing; provide Windows CI runner or Windows acceptance evidence; decide signing/update/rollback policy. Do not enable Play/Import/LAN/Skin until registered native backends with evidence.
- **Next:** On networked Windows: generate reproducible package lock, run `npm ci`, `npm test`, `npm run build`, desktop Tauri compile and smoke; fix failures and report logs. PR remains Draft; **do not merge**.

<!-- CHAT-07:END -->

---

## Control log

- 2026-11-13 — Live ledger created; all seven PRs currently open; evidence not independently reviewed. Control owns merges and gate verification.
- 2026-11-13 — CONTROL CI budget optimization: foundation PR #13 workflow edited at `6035abc9` to PR-only Ubuntu Rust checks, path filter and concurrency; Windows tests deferred to integration/manual. New policy: [CI_BUDGET_POLICY.md](https://github.com/1doiliemkhoet/tlauchcraft/blob/main/docs/control/CI_BUDGET_POLICY.md). First optimized run [43447713759](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43447713759): Ubuntu success, Windows intentionally skipped. Skip does NOT count as Windows PASS.
- 2026-11-13 — CONTROL D-007: tlauchcraft officially targets Windows only. Ubuntu/Linux CI removed; PR #13 Rust workflow now uses a single path-filtered `windows-latest` job (commit [`29cd56d8`](https://github.com/1doiliemkhoet/tlauchcraft/commit/29cd56d884f452752674038600f70fd95aa92e7a)), PR-only + concurrency cancel. [Updated CI policy](https://github.com/1doiliemkhoet/tlauchcraft/blob/develop/docs/control/CI_BUDGET_POLICY.md). Previous Ubuntu success is historical evidence, not Windows acceptance for the new commit.
- 2026-11-13 — CONTROL verified [Windows-only Rust CI run 43448481543](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43448481543) for commit `29cd56d8`: **one Windows job completed with success**. This validates the new Windows CI configuration and Rust checks at this revision; it is **not** Minecraft Singleplayer/LAN E2E evidence.
- 2026-11-13 — CONTROL accepted [PR #13](https://github.com/1doiliemkhoet/tlauchcraft/pull/13) and merged it into `develop` at [`ba79eba1`](https://github.com/1doiliemkhoet/tlauchcraft/commit/ba79eba1c1bbd10477c146aafa2f74b45c243255). **G0 Foundation v1 ACCEPTED, G1 Windows Foundation CI ACCEPTED** ([run 43448481543](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43448481543)); G2–G8 NOT_VERIFIED. [Control acceptance](https://github.com/1doiliemkhoet/tlauchcraft/blob/develop/docs/control/G0_FOUNDATION_ACCEPTANCE.md). [Integration issue #15](https://github.com/1doiliemkhoet/tlauchcraft/issues/15), offline identity #16, storage hardening #17. All six remaining feature PRs remain open; Control requires Windows workspace tests and contract alignment before integration.
- 2026-11-13 — CONTROL rechecked [PR #12](https://github.com/1doiliemkhoet/tlauchcraft/pull/12) HEAD `edc0dc77`: [Windows CI run 43452641107](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43452641107) **FAIL** because root `Cargo.lock` needs regeneration after adding `tl-game`; workspace guard and `cargo fmt` PASS, `cargo clippy --locked` FAIL, `cargo test` SKIPPED. Remains BLOCKED/NOT_VERIFIED, no merge. Exact remediation posted on [PR #12](https://github.com/1doiliemkhoet/tlauchcraft/pull/12#issuecomment-6857530756). Other five feature PRs remain draft/unverified.
- 2026-11-13 — CONTROL repaired Chat 2 stale `Cargo.lock` with [Windows one-shot generator run 43455593254](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43455593254), committed generated lockfile at `5d811d8`, removed temporary workflow at `69779a8`. [Windows run 43455813557](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43455813557) discovered Rust MSRV mismatch (ICU 2.4 requires >=1.88); aligned Chat 2 branch rust-toolchain/workflow/workspace MSRV to 1.88.0 at `983d089`. [Latest Windows run 43456243159](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43456243159): workspace and rustfmt PASS; Clippy **FAIL** on six `unnecessary_map_or` findings in `crates/tl-game/src/lib.rs`; tests SKIPPED. Fix instructions posted on [PR #12](https://github.com/1doiliemkhoet/tlauchcraft/pull/12#issuecomment-6857923060). G1 still NOT_VERIFIED; no merge or Minecraft E2E claimed.
- 2026-11-13 — CONTROL accepted [PR #12](https://github.com/1doiliemkhoet/tlauchcraft/pull/12) for **Windows G1 only**: [run 43459887778](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43459887778) completed SUCCESS; workspace guard, rustfmt, clippy -D warnings and tests PASS (**24 tests passed; 0 failed**). Merged Chat 2 to `develop` at [`be4abef7`](https://github.com/1doiliemkhoet/tlauchcraft/commit/be4abef74029680fc9deb2783cdcbb565b6f69c0). [Acceptance details](https://github.com/1doiliemkhoet/tlauchcraft/blob/develop/docs/control/G1_GAME_CORE_ACCEPTANCE.md). **G2 actual game install / G3 menu launch / G4 world save-reopen NOT_VERIFIED**. Chat 3 PR #8 is now the next workspace integration gate ([Control direction](https://github.com/1doiliemkhoet/tlauchcraft/pull/8#issuecomment-6858939617)); Chat 2 continues functional E2E via new follow-up branch/PR. Issue #2 stays open.
- 2026-11-13 — CONTROL parallel tasking issued to Chat 4 [PR #10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10#issuecomment-6859270448), Chat 5 [PR #9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9#issuecomment-6859271255), Chat 6 [PR #11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11#issuecomment-6859272341), Chat 7 [PR #14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14#issuecomment-6859273343). Scope: sync merged Foundation/Game Core, isolate module tests, Windows G1, keep G2–G8 NOT_VERIFIED until real E2E. Chat 1 continues #16/#17 hardening on a new branch; Chat 2 continues G2–G4 real Minecraft E2E on a new branch. Existing PR #8 Chat 3 is in progress. No additional PR merges granted.
- 2026-11-14 — CONTROL verified [PR #8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8) at `8587c9d` with [Windows CI 43468262347](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43468262347): workspace registered, fmt/clippy PASS and **35/35 tests PASS**. **MERGED** to `develop` at [`1bd1ce8e`](https://github.com/1doiliemkhoet/tlauchcraft/commit/1bd1ce8e7efa91df1e2a93aef36110f72ac59d8c) for G1 metadata/loader planning only; G2/G3/G5 real install/launch NOT_VERIFIED. [Acceptance](https://github.com/1doiliemkhoet/tlauchcraft/blob/develop/docs/control/G1_LOADER_ACCEPTANCE.md). Windows CI [43476043990](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43476043990) Chat 5 LAN **PASS** and [43476040735](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43476040735) Chat 6 Skin **PASS** on their feature heads, but must sync `develop` after loader merge and re-run G1 before acceptance. Chat 4 PR #10 head unchanged, last Windows run [43470699059](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43470699059) still **FAIL** on stale `Cargo.lock`.
