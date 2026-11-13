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
| [Chat 2](#chat-2--minecraft-core--singleplayer) | Minecraft Core & Singleplayer | **G1 ACCEPTED · P0 E2E IN_PROGRESS** | [#19 (draft)](https://github.com/1doiliemkhoet/tlauchcraft/pull/19) | [Windows CI pending](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43471898813) | **G2–G4 NOT_VERIFIED** |
| [Chat 3](#chat-3--mod-loader-adapters) | Mod Loader Adapters | **READY_FOR_CONTROL_REVIEW · G1 CI PASS** | [#8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8) | Windows CI 35 PASS / 0 failed | G1 review pending; G2/G3/G5 NOT_VERIFIED |
| [Chat 4](#chat-4--modpack-zip-import) | Modpack ZIP Import | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10) | NOT_TESTED | NOT_VERIFIED |
| [Chat 5](#chat-5--trusted-lan-multiplayer) | Trusted LAN Multiplayer | **IN_PROGRESS · PR_OPEN_UNREVIEWED** | [#9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9) | 15 AUTHORED · NOT_TESTED | NOT_VERIFIED |
| [Chat 6](#chat-6--offline-skin-manager) | Offline Skin Manager | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11) (Draft) | NOT_TESTED | NOT_VERIFIED |
| [Chat 7](#chat-7--desktop-ui--packaging) | Desktop UI & Packaging | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14) | NOT_TESTED | NOT_VERIFIED |

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
- **Current HEAD:** [`409d66e51f5b464bcee0abff395832089e88e46f`](https://github.com/1doiliemkhoet/tlauchcraft/commit/409d66e51f5b464bcee0abff395832089e88e46f)
- **PR:** [#19 — Draft/Open/NOT MERGED](https://github.com/1doiliemkhoet/tlauchcraft/pull/19). No source changes on `develop`.
- **Current state:** `P0_IMPLEMENTATION_IN_PROGRESS`. Native extractor and run harness authored; G2/G3/G4 `NOT_TESTED` / `NOT_VERIFIED`. Windows new-branch CI not yet fully verified.
- **Changed paths:** `crates/tl-game/src/vanilla.rs` (official-release metadata, metadata-SHA1, verified downloads, asset index/objects, native archive listing and extraction, Java version probe, classpath and launch substitutions, nonsecret runtime evidence); `crates/tl-game/src/lib.rs` (module registration); `crates/tl-game/examples/vanilla_e2e.rs` (Windows manual installer/launch harness producing PID/stdout/stderr); `docs/control/CHAT02_GAME_E2E_CHECKLIST.md` (real-game acceptance).
- **Security/data:** No Minecraft or Java binaries committed/distributed; official HTTPS allowlist and SHA1 enforced for downloads. Refuses overwrite of conflicting existing files. Persisted `tl-core` profile ID is not assumed to equal game UUID. Access token supplied via environment and excluded from evidence file. Native extraction path scanning is experimental; link/collision security review pending.
- **Windows CI status:** [Run 43470464820](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43470464820) fmt FAIL (fixed in `eb51138a`); [run 43470978253](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43470978253) fmt FAIL on new example (fixed in `3693e9fa`); [run 43471667933](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43471667933) fmt FAIL on logging changes (fixed in `409d66e5`). [Current run 43471898813](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43471898813) triggered; final results pending at time of this update. No success claim until observed.
- **Windows commands:** `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace --all-targets`. No CI bypasses or Cargo.lock changes made.
- **Evidence:** No real game run available in this ChatGPT environment; no Windows hardware game PID/menu screenshot, successful G2 install, or Singleplayer save/reopen proof. G3/G4 remain NOT_TESTED.
- **Gates:** prior G1 ACCEPTED, new-branch unit CI PENDING; G2 NOT_VERIFIED; G3 NOT_VERIFIED; G4 NOT_VERIFIED.
- **Dependencies/risks:** Windows Java compatible runtime and authorized Minecraft account/session supplied by tester; manual real Windows E2E via example, log/screenshots/world re-open; extractor audit and native compatibility.
- **Next:** Resolve actual Windows CI errors, then a Windows tester performs G2–G4 and attaches concrete evidence to PR #19 for Control review. **No self-merge.**

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

<!-- CHAT-03:END -->

---

<!-- CHAT-04:START -->
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

- **Branch:** `feat/tl-05-lan`
- **Branch HEAD:** [`b601bfd81ceac7af0a1c46313c792114da784ed8`](https://github.com/1doiliemkhoet/tlauchcraft/commit/b601bfd81ceac7af0a1c46313c792114da784ed8)
- **Issue:** [#5](https://github.com/1doiliemkhoet/tlauchcraft/issues/5)
- **PR to develop:** [#9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9) — OPEN, DRAFT, NOT MERGED.
- **Status:** `PR_OPEN_UNREVIEWED` / `IN_PROGRESS`; functions below are CODE_WRITTEN, **NOT_TESTED**, not accepted.
- **Updated at:** 2026-11-13 (Chat 5 confirmed PR and branch)
- **Implementation (code written):** Standalone Rust `tl-lan` compatibility comparison for Minecraft/loader version and provided SHA256 gameplay fingerprints, client/server side filtering; trusted private-IPv4 bind validation; explicit offline-mode risk acknowledgement; non-overwriting `server.properties` with online-mode=false and RCON off; Direct Connect IP:port; username persistence guard; locally supplied server-JAR process spawn/PID/log/graceful-stop API; stopped-world cold backup. Added guard against backup directory nested inside world; six extra tests cover nested/live backups, bad/duplicate hashes, unsafe Direct Connect, non-overwrite properties and port collision. Code does NOT install a loader or hash actual pack files; process launch is generic `java -jar` and not compatible with every modded server.
- **Changed files:** `crates/tl-lan/Cargo.toml`, `crates/tl-lan/src/lib.rs`, `crates/tl-lan/README.md`. Five commits on branch; latest adds backup path containment guard and six hermetic regression test cases; PR stats should be refreshed by Control.
- **API impact:** Temporary crate-local `LanServerSpec`, `PackFingerprint`, `CompatibilityReport`, `LanError`. Shared API Contract v1 still provisional, requires Chat 1 + Control G0 review. No change outside owned paths.
- **Test commands (documented, NOT_EXECUTED):** `cargo test --manifest-path crates/tl-lan/Cargo.toml`; `cargo clippy --manifest-path crates/tl-lan/Cargo.toml --all-targets -- -D warnings`. Fifteen unit tests authored; **0 executed, 0 passed claimed**, no CI artifact reviewed. GitHub code commit/push and PR creation verified by GitHub API.
- **Real device / Minecraft / modpack evidence:** `NOT_TESTED` — no real server Java log, no client handshake, no two-PC session, no world/inventory rejoin artifacts.
- **Gates:** G0 `BLOCKED` on concrete shared typed interface approval; G1 `NOT_TESTED`; G6 `NOT_TESTED`; all other gates `NOT_VERIFIED`. Control has granted no acceptance.
- **Risks / dependencies:** Chat 1 stable identity/shared types; Chat 2 Java process contract; Chat 3 Fabric/Forge/NeoForge server recipes; Chat 4 verified modpack lockfile/side/hash; Chat 7 UI confirmation; LAN-only offline identity spoofing, firewall/subnet policies and username change affecting offline UUID. No router forwarding.
- **Control decisions requested:** Freeze `LanService` and pack fingerprint interface; determine loader-specific launch contract, server-side mod selection and authenticated trusted-LAN approach; review PR #9 only after Windows Rust tests and G6 evidence. No merge requested now.
- **Next actions:** Safety/regression test expansion committed as [`b601bfd`](https://github.com/1doiliemkhoet/tlauchcraft/commit/b601bfd81ceac7af0a1c46313c792114da784ed8); next run cargo/fmt/clippy on a supported Rust runner, integrate typed contracts, perform two-PC Fabric G6 and preserve actual logs/hashes.

<!-- CHAT-05:END -->

---

<!-- CHAT-06:START -->
## Chat 6 — Offline Skin Manager

- **Branch:** `feat/tl-06-skins`
- **HEAD:** `9f4fc91b2532668bb3543d4c07327c7eb2670929` (2026-11-13)
- **Issue:** [#6](https://github.com/1doiliemkhoet/tlauchcraft/issues/6)
- **PR:** [#11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11) → `develop`, OPEN / DRAFT / not merged.
- **Status:** `PR_OPEN_UNREVIEWED` + `IN_PROGRESS`. Source files committed and pushed; compilation and acceptance NOT_TESTED.
- **Updated:** 2026-11-13 (Chat 6 report).
- **Implemented in source (not yet runtime-validated):** PNG 64×64 and legacy 64×32 decoding/encoding with bounds validation; app-managed skin catalog, SHA-256 integrity, import/export/rename/duplicate/confirmed delete; stable profile UUID selections, Classic/Slim model, per-instance override; pixel paint/sample/erase/flood fill/mirror/palette/undo/redo; UV atlas descriptors; explicit preview/local/peer visibility states and consent-gated bounded PNG payload (does not transfer data itself).
- **Changed paths:** `crates/tl-skin/Cargo.toml`, `crates/tl-skin/src/lib.rs`, `crates/tl-skin/tests/skin_tests.rs`, `crates/tl-skin/README.md`, `docs/control/tasks/CHAT-06-IMPLEMENTATION-NOTES.md`. Public crate interfaces are provisional; root workspace/shared contract untouched.
- **Test command (proposed, NOT RUN):** `cargo test --manifest-path crates/tl-skin/Cargo.toml`; `cargo fmt --manifest-path crates/tl-skin/Cargo.toml -- --check`. **Observed result: NOT_TESTED** (no executable Rust toolchain/test logs in this session; GitHub returned 0 workflow runs and 0 statuses for feature HEAD on 2026-11-13). Hermetic Rust test source covers PNG, invalid inputs, persistence, per-instance override, pixel editor, consent and visibility state transitions; existence is not PASS.
- **E2E evidence:** NONE. Singleplayer skin visibility NOT_TESTED; 2-PC LAN peer skin visibility NOT_TESTED. G1 code, G7 skin and all other gates NOT_VERIFIED.
- **Incomplete/risks:** 3D renderer, complete legacy UV conversion, custom cape storage/application, cosmetics packs, actual Minecraft skin provider installation and supported-version matrix, LAN transport and peer cache handling, real-game evidence. Offline LAN identity is spoofable; consent required. Provider metadata cannot prove actual skin display.
- **Dependencies:** Chat 1 profile/instance shared Rust types and workspace; Chat 2/3 version/loader-aware provider; Chat 5 trusted LAN appearance transport; Chat 7 UI, 3D preview and editor integration.
- **Control decisions requested:** Approve SkinService adapter and ownership of profile/instance cosmetic selections; establish provider/version compatibility matrix and LAN consent handshake contract before cross-crate integration. Keep PR Draft until G1 tests pass; Control owns merge/gate signoff.
- **Additional commit:** [`9f4fc91`](https://github.com/1doiliemkhoet/tlauchcraft/commit/9f4fc91b2532668bb3543d4c07327c7eb2670929) fixes the library duplicate borrow-order issue; **NOT_TESTED** until Rust compilation is available.
- **Next:** audit/fix crate; run real Windows cargo tests once toolchain accessible; implement additional P1 skin editing/storage without modifying other chats' files; collect actual Singleplayer and 2-PC evidence after integration.

<!-- CHAT-06:END -->

---

<!-- CHAT-07:START -->
## Chat 7 — Desktop UI & Packaging

- **Branch:** `feat/tl-07-desktop`
- **Branch HEAD:** [`625dd0f4f70aa17799d83f477a136b78e50e150c`](https://github.com/1doiliemkhoet/tlauchcraft/commit/625dd0f4f70aa17799d83f477a136b78e50e150c)
- **Issue:** [#7](https://github.com/1doiliemkhoet/tlauchcraft/issues/7)
- **PR to develop:** [#14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14) — OPEN DRAFT; NOT MERGED
- **Status:** `PR_OPEN_UNREVIEWED` / `IN_PROGRESS`; source scaffold `IMPLEMENTED`, functionality `NOT_VERIFIED`
- **Updated at:** 2026-11-13 (Chat 7 report)
- **Summary / implemented functionality:** Tauri 2 + React/TypeScript desktop scaffold, navigation for offline profiles, instance/version selection, pack archive picker (.zip/.mrpack), skin preview/visibility status, trusted LAN status, settings, download progress, session logs and error feedback. Typed backend adapter negotiates capabilities; Tauri host currently advertises all core features as unavailable. Controls requiring real backend are disabled; no simulated game launch, download or successful archive import.
- **Changed paths / API impact:** `apps/desktop/src/App.tsx`, `src/backend.ts`, `src/backend.test.ts`, `src/main.tsx`, `src/style.css`, `src-tauri/src/main.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/icons/icon.png`, package/toolchain setup and `apps/desktop/README.md`. Only `apps/desktop/**` changed in feature PR. Proposed command names (`desktop_capabilities`, `profile_list`, `profile_create`, `instance_list`, `version_list`, `pack_import`, `game_launch`, `operation_cancel`, `skin_list`, `lan_status`) are NOT frozen/implemented interfaces; pending Control approval.
- **Additional commit:** [\`625dd0f4f70aa17799d83f477a136b78e50e150c\`](https://github.com/1doiliemkhoet/tlauchcraft/commit/625dd0f4f70aa17799d83f477a136b78e50e150c) — expanded Vitest coverage of malformed capability response, Tauri command rejection, backend error propagation and command argument payload. Tests authored, **NOT_TESTED**.
- **Test commands and observed results:** `cd apps/desktop; npm install; npm run test` — `NOT_TESTED` (not executed). `npm run build`, `npm run tauri dev`, `npm run tauri build` — `NOT_TESTED`. Source unit tests cover capability gating, extension filter and percent bounds but have NOT been executed; do not infer PASS. GitHub branch/PR existence confirmed through GitHub API.
- **Real device / Minecraft / modpack evidence:** `NOT_TESTED`; no Windows installer hash, clean-machine smoke, Java/Minecraft PID, world, two-PC LAN or real skin screenshot.
- **Acceptance gates G0–G8:** G0 `BLOCKED` (shared Rust/Tauri facade pending); G1 `NOT_VERIFIED` (build/test not run); G2–G8 `NOT_VERIFIED`.
- **Blockers / dependencies / risks:** Requires Chat 1 frozen Rust shared types and registered Tauri facade; Chat 2/3 game/versions; Chat 4 pack import; Chat 5 LAN; Chat 6 skin integration. Browser drag/drop intentionally does not trust a native path; native file dialog used. Windows toolchain, NSIS and WebView2 packaging untested.
- **Pending decisions for Control:** Approve stable Tauri command names/argument/return shapes, error schema, capability negotiation, event subscription/cancellation semantics, frontend packaging/signing/update policy. Decide integration sequencing after Chat 1.
- **Next actions:** Run and fix npm/Vitest/TypeScript checks, add UI and command integration tests; wire actual feature commands only after Rust API approval; build unsigned NSIS and record Windows clean-machine smoke evidence. Control to review PR; DO NOT MERGE.

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
