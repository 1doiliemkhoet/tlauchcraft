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
| [Chat 1](#chat-1--foundation--persistence) | Foundation & Persistence | **MERGED · CONTROL_ACCEPTED G0/G1** | [#13](https://github.com/1doiliemkhoet/tlauchcraft/pull/13) | Windows fmt/clippy/test PASS | **G0/G1 ACCEPTED** |
| [Chat 2](#chat-2--minecraft-core--singleplayer) | Minecraft Core & Singleplayer | **PR_OPEN_UNREVIEWED · BLOCKED (Cargo.lock)** | [#12 (draft)](https://github.com/1doiliemkhoet/tlauchcraft/pull/12) | [Windows fmt PASS, clippy FAIL (lock), tests SKIPPED](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43454574300) | NOT_VERIFIED |
| [Chat 3](#chat-3--mod-loader-adapters) | Mod Loader Adapters | **PR_OPEN_UNREVIEWED · Draft** | [#8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8) | 11 authored; NOT_TESTED | NOT_VERIFIED |
| [Chat 4](#chat-4--modpack-zip-import) | Modpack ZIP Import | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10) | NOT_TESTED | NOT_VERIFIED |
| [Chat 5](#chat-5--trusted-lan-multiplayer) | Trusted LAN Multiplayer | **IN_PROGRESS · PR_OPEN_UNREVIEWED** | [#9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9) | 15 AUTHORED · NOT_TESTED | NOT_VERIFIED |
| [Chat 6](#chat-6--offline-skin-manager) | Offline Skin Manager | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11) (Draft) | NOT_TESTED | NOT_VERIFIED |
| [Chat 7](#chat-7--desktop-ui--packaging) | Desktop UI & Packaging | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14) | NOT_TESTED | NOT_VERIFIED |

---

## Individual reports

<!-- CHAT-01:START -->
## Chat 1 — Foundation & Persistence

- **Branch:** `feat/tl-01-foundation`
- **Current branch HEAD / last tested commit:** `7fd46ed764c08142a57f983eed1d2a7ac389a6b7` ([commit](https://github.com/1doiliemkhoet/tlauchcraft/commit/7fd46ed764c08142a57f983eed1d2a7ac389a6b7)); new Windows+Ubuntu CI verified. Baseline previous commit `1a5980574e431e67c7de1aa135a65099877293e6`.
- **Issue:** [#1](https://github.com/1doiliemkhoet/tlauchcraft/issues/1)
- **PR to develop:** [#13](https://github.com/1doiliemkhoet/tlauchcraft/pull/13) — OPEN, DRAFT, NOT MERGED.
- **Status:** `PR_OPEN_UNREVIEWED`; `IMPLEMENTED` shared Foundation code; `TESTED` Windows/Ubuntu CI; `NOT_VERIFIED` by Control.
- **Updated at:** 2026-11-13 (Chat 1).
- **Implemented functionality:** Rust 1.85.1 workspace with `tl-core` and `tl-storage`; shared serde v1 DTOs (`OfflineProfile`, `GameInstance`, loader/state, download/install/launch/LAN/skin/event/error types), stable local UUID and username validation/rename, isolated relative `instances/<UUID>` paths, SQLite schema v1 transaction, WAL/foreign keys, profile+instance CRUD, metadata-only deletion, SQLite backup API, path validation and typed/redacted public errors. Committed `Cargo.lock` for reproducible dependency resolution. This does not implement Minecraft game launch, installed packs or UI.
- **Changed files:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/tl-core/Cargo.toml`, `crates/tl-core/src/lib.rs`, `crates/tl-storage/Cargo.toml`, `crates/tl-storage/src/lib.rs`, `crates/tl-storage/tests/storage.rs`, `.github/workflows/rust.yml`, `docs/foundation/README.md`. Source changes isolated to feature branch, not develop.
- **Shared API impact:** `OfflineProfile.id` persistent local UUID independent of username (NOT the server-assigned offline UUID); `GameInstance.root_path` fixed to launcher-owned relative `instances/<UUID>`; public serialized DTOs and error vocabulary. Other chats must use Control-approved contract changes; interfaces not frozen by Control yet.
- **Latest update (2026-11-13):** Added three regression tests in `crates/tl-storage/tests/storage.rs`: backup refuses overwrite, higher schema version safely rejected, deletion of instance metadata preserves world fixtures. New commit [`7fd46ed`](https://github.com/1doiliemkhoet/tlauchcraft/commit/7fd46ed764c08142a57f983eed1d2a7ac389a6b7). [Follow-up CI run](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43446780516): Ubuntu cargo test/clippy/fmt PASS; Windows IN_PROGRESS at reporting time. No claim of new Windows PASS until confirmed.\n- **CI evidence:** [GitHub Actions run 43441813351](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43441813351), prior tested commit `1a5980574e431e67c7de1aa135a65099877293e6`, Windows and Ubuntu matrix `success`.
- **Test commands / observed results:** Latest [run 43446780516](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43446780516) for `7fd46ed`: `cargo test --locked --workspace --all-targets` => Windows **12 passed, 0 failed**, Ubuntu **12 passed, 0 failed** (4 core unit + 8 storage integration); `cargo clippy --locked --workspace --all-targets -- -D warnings` => **PASS** both; `cargo fmt --all -- --check` => **PASS** both. Includes profile/instance persistence, rename identity, safe paths, FK protection, backup, migration future-version guard, concurrent writes, world-preserving metadata deletion.
- **Real device / Minecraft / modpack / LAN evidence:** `NOT_TESTED` for real Minecraft installation, game launch, world lifecycle, modpack import, two-machine LAN, skins and Windows installer. GitHub-hosted Windows CI is not a real on-device Minecraft E2E.
- **Acceptance gates:** `G0: IMPLEMENTED + TESTED, Control review PENDING`; `G1: CI TESTED (12/12 Windows + 12/12 Ubuntu, fmt/clippy PASS), Control gate NOT_VERIFIED`; `G2–G8: NOT_TESTED / NOT_VERIFIED by Chat 1`.
- **Blockers / dependencies / risks:** Requires Control contract review/PR merge before Chat 2–7 depend on shared APIs. No automatic compare-and-swap/lost-update protection across concurrent edits (concurrency test is not a lost-update proof). Schema v2 migration downgrade/rollback recovery not tested. Symlink escape prevention at actual game/archive IO boundaries remains the caller's responsibility; storage path checks alone are insufficient. No real Minecraft E2E yet.
- **Decisions requested from Control:** Approve or amend local UUID vs server UUID separation; launcher-relative instance path convention; serde DTO/error compatibility and public interface freeze; decide revision/CAS design for concurrent profile updates.
- **Next actions:** Control reviews PR #13, G0/G1 contract and CI evidence; address untested migration rollback / concurrency lost-update semantics in owned crates. Do not merge without Control authorization.

- **Control decision 2026-11-13:** PR #13 **MERGED** into `develop` at `ba79eba1`. G0 shared Foundation v1 contract ACCEPTED; G1 Windows Foundation CI ACCEPTED ([run](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43448481543)); all other gates NOT_VERIFIED. Follow-ups: [#15](https://github.com/1doiliemkhoet/tlauchcraft/issues/15) workspace integration, [#16](https://github.com/1doiliemkhoet/tlauchcraft/issues/16) game/server UUID mapping, [#17](https://github.com/1doiliemkhoet/tlauchcraft/issues/17) concurrency and backup hardening. Original report above is historical at its authored date.

<!-- CHAT-01:END -->

---

<!-- CHAT-02:START -->
## Chat 2 — Minecraft Core & Singleplayer

- **Branch:** `feat/tl-02-game-core`
- **HEAD:** [`5b68b4291d9af14794c71eeda0ac0fe175b7feed`](https://github.com/1doiliemkhoet/tlauchcraft/commit/5b68b4291d9af14794c71eeda0ac0fe175b7feed)
- **Issue / PR:** [#2](https://github.com/1doiliemkhoet/tlauchcraft/issues/2) / [#12 — Draft, OPEN, NOT MERGED](https://github.com/1doiliemkhoet/tlauchcraft/pull/12)
- **Status:** `PR_OPEN_UNREVIEWED`, `IN_PROGRESS`, `BLOCKED` — stale `Cargo.lock`, Windows tests not executed. G1 NOT_VERIFIED; G2–G4 NOT_TESTED.
- **Updated at:** 2026-11-13 (Chat 02, Control lockfile remediation)
- **Latest develop sync:** [`5b68b429`](https://github.com/1doiliemkhoet/tlauchcraft/commit/5b68b4291d9af14794c71eeda0ac0fe175b7feed) merged `develop` at `5a1fa14c933cba98c9b55280e8a92cf0a10a784c` as second parent; feature branch verified `ahead=11, behind=0` vs develop, without force-push or overwriting Chat 2 paths. Prior Foundation PR #13 already incorporated.
- **Implementation/changed paths:** `Cargo.toml` registers `crates/tl-game`; `crates/tl-game/Cargo.toml` references `tl-core` directly; `crates/tl-game/src/foundation.rs` validates Foundation v1 `OfflineProfile`/`GameInstance`/`LaunchRequest` identity and Vanilla context; `crates/tl-game/src/lib.rs` includes metadata resolver, argument builder, HTTPS staging downloader and Java process primitive; `crates/tl-game/src/metadata.rs` plans Mojang libraries/assets/native archives; `crates/tl-game/README.md` describes scope and limitations. No other chat's code modified.
- **Windows CI observed:** [Run 43454574300](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43454574300) for latest merge HEAD: registered-crate check PASS; `cargo fmt --all -- --check` PASS; `cargo clippy --locked --workspace --all-targets -- -D warnings` FAIL with actual error `Cargo.lock needs to be updated but --locked was passed to prevent this`; `cargo test --locked --workspace --all-targets` SKIPPED. Previous Windows run [43452641107](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43452641107) had the same lockfile blocker. No CI controls changed or relaxed.
- **Unresolved lockfile:** Existing `Cargo.lock` is Foundation-only and does not contain new `reqwest` / `sha1` dependencies of Chat 2. `cargo generate-lockfile` was **NOT_RUN**: the available execution environment has neither `cargo` nor `rustc` and outbound network/DNS is unavailable; the connected GitHub tool supports file commits but cannot execute Cargo. Lockfile was **NOT** hand-fabricated or committed. Rust test PASS is **NOT** claimed.
- **Required Windows repro commands (not run locally):** `cargo generate-lockfile`; `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace --all-targets`. Regenerate and commit the lockfile on a Windows machine with Cargo, then new PR CI must pass all three checks. Never remove `--locked` or skip Clippy.
- **Gates:** G0 shared Foundation merged, integration not reviewed by Control; G1 BLOCKED/NOT_VERIFIED; G2 real install NOT_TESTED; G3 real Minecraft launch/PID/menu NOT_TESTED; G4 world create/save/reopen NOT_TESTED. No Microsoft login or game binary redistribution.
- **Remaining work/risks:** Generate and commit correct Cargo.lock; resolve compilation/Clippy/test findings once dependency resolution is unlocked; finish native extraction, version metadata install and Java-to-game launch; run real Windows E2E with licensed content.
- **Control decision:** Keep PR #12 Draft/unmerged until full Windows CI (fmt, Clippy, tests) provides PASS evidence; G2–G4 remain independent real-device gates.
- **Next:** Run Cargo on Windows, commit/push Cargo.lock on Chat 2 feature branch, rerun CI and fix subsequent issues. Update this ledger with concrete run SHA/result after each event.

<!-- CHAT-02:END -->

---

<!-- CHAT-03:START -->
## Chat 3 — Mod Loader Adapters

- **Branch:** `feat/tl-03-loaders`
- **HEAD:** [`c1b26d0180c2c02023cdf76c0039fb08a95d0ec1`](https://github.com/1doiliemkhoet/tlauchcraft/commit/c1b26d0180c2c02023cdf76c0039fb08a95d0ec1) (checked 2026-11-13)
- **Issue:** [#3](https://github.com/1doiliemkhoet/tlauchcraft/issues/3)
- **PR:** [#8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8), OPEN DRAFT, **NOT MERGED**
- **Status:** `PR_OPEN_UNREVIEWED`; partial metadata planning code **IMPLEMENTED**, integration **BLOCKED**, real installation **NOT_VERIFIED**
- **Updated:** 2026-11-13 (Chat 3)
- **Functionality implemented:** isolated Rust `LoaderInstaller` trait and typed errors/spec/plan; official Fabric profile JSON parsing, Minecraft/loader ID/inheritance/mainClass validation, approved Maven host filtering, artifact paths, Java-major check; Forge Maven XML catalog and acquisition-only installer plan; NeoForge catalog parsing and fail-closed unsupported game-version mapping; Forge/NeoForge installer-profile processor inspection with coordinate, side, argument and Minecraft-version validation (inspection only, never executes Java processors). No Minecraft game launcher or installer execution in this crate.
- **Changed paths:** `crates/tl-loader/Cargo.toml`, `crates/tl-loader/src/{lib.rs,fabric.rs,maven.rs}`, `crates/tl-loader/tests/metadata.rs`, `crates/tl-loader/src/installer.rs` (6 files, only Chat 3 ownership).
- **Tests:** 11 fixture tests authored, **NOT_TESTED** (0 observed PASS). Suggested but **not executed**: `cargo test --manifest-path crates/tl-loader/Cargo.toml` and `cargo fmt --manifest-path crates/tl-loader/Cargo.toml -- --check`. No linked CI status.
- **E2E proof:** **NOT_TESTED / NOT_VERIFIED**: no Fabric mod loading, Forge/NeoForge processor output, artifact hash evidence, PID or game startup logs.
- **Gates:** G0 provisional/unapproved; G1 NOT_TESTED; G2/G3/G5 NOT_VERIFIED.
- **Dependencies / risks:** Chat 1 shared types + workspace; Chat 2 verified FileInstaller, GameRuntime and version metadata inheritance; official Forge/NeoForge installation processors not integrated. Fabric artifact integrity and compatible Java runtime verification required.
- **Control decisions:** approve loader-plan merge contract with Chat 2 and select Fabric + Minecraft + Java E2E test matrix; confirm authoritative NeoForge version mapping policy.
- **Next steps:** run Rust compilation/unit/fmt with evidence, harden metadata parsing, integrate installer via Chat 1/2 approved APIs, then on-device Fabric mod-load and Forge/NeoForge smoke tests.

<!-- CHAT-03:END -->

---

<!-- CHAT-04:START -->
## Chat 4 — Modpack ZIP Import

- **Branch:** `feat/tl-04-modpack`
- **Branch HEAD:** [`b10a83b66e0bdda33eb446b94f00a6b3fcffda39`](https://github.com/1doiliemkhoet/tlauchcraft/commit/b10a83b66e0bdda33eb446b94f00a6b3fcffda39)
- **Issue:** [#4](https://github.com/1doiliemkhoet/tlauchcraft/issues/4)
- **PR to develop:** [#10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10) — OPEN, DRAFT, UNREVIEWED, not merged.
- **Status:** `PR_OPEN_UNREVIEWED` / `IN_PROGRESS`; archive inspector `IMPLEMENTED` as source code; testing `NOT_TESTED`; end-to-end `BLOCKED` / `NOT_VERIFIED`.
- **Updated at:** 2026-11-13 (Chat 4)
- **Implemented source slice:** Rust crate `tl-pack` structurally recognizes CurseForge export ZIP (`manifest.json`), Modrinth `.mrpack` (`modrinth.index.json`) and Prism/MultiMC exported ZIP (`mmc-pack.json` + `instance.cfg`). Parses Minecraft/loader versions, CurseForge project/file identifiers, Modrinth hashes/download references/client environment and Prism components. Supports deterministic `overrides/` then `client-overrides/` precedence for Modrinth client staging. Checks archive entry paths, Windows case-folded duplicates, Unix symlink/special file modes, excessive decompression size/count/ratio; checks original ZIP SHA-256 before extracting supported overrides into a fresh staging path. Does not execute archive content.
- **Changed paths:** `crates/tl-pack/Cargo.toml`, `crates/tl-pack/src/lib.rs`, `crates/tl-pack/tests/archive.rs`, `crates/tl-pack/README.md`. Standalone package, shared API not modified, root workspace not modified.
- **Test commands / observed results:** `cargo test --manifest-path crates/tl-pack/Cargo.toml` — `NOT_TESTED` (not executed; Rust/Cargo unavailable in prior execution environment). `cargo fmt --manifest-path crates/tl-pack/Cargo.toml --check` — `NOT_TESTED`. Eight hermetic test cases committed, no passing test count claimed. No CI evidence available in this report.
- **Real device / Minecraft / modpack evidence:** `NOT_TESTED`; no real download, installation, runtime process PID, menu, worlds or LAN traces. `READY` is deliberately not produced by this parser/staging slice.
- **Acceptance gates:** G0 contract pending Control integration review; G1 `NOT_VERIFIED`; G2 install `NOT_VERIFIED`; G3 launch `NOT_VERIFIED`; G5 modpack `NOT_VERIFIED`; others not asserted.
- **Blockers / dependencies / risks:** Chat 1 `GameInstance`/`InstanceStore`, Chat 2 downloader with permitted HTTPS redirect/DNS/private-IP protection and checksum/cache/retry, Chat 3 LoaderInstaller/Java version compatibility. Authorized CurseForge API access required; no CDN bypass. Dependency graph, full server-overrides support, lockfile, atomic publish/rollback, cancellation/resume, real downloaded mod hashes and real launch are pending. No code execution evidence yet.
- **Pending Control decisions:** Freeze concrete `PackImporter` ↔ `InstanceStore`/`FileInstaller`/`LoaderInstaller` interface and integrate root workspace after Chat 1; confirm approved CurseForge source policy; decide readiness gates for P0.
- **Next actions:** Harden archive validation and stage handling within `tl-pack`, run Cargo tests and Windows checks where toolchain is accessible, integrate dependencies only after shared contracts settle, perform genuine CurseForge + Modrinth installation/launch E2E before declaring VERIFIED.

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
