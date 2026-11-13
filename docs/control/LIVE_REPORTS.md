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
| [Chat 1](#chat-1--foundation--persistence) | Foundation & Persistence | **PR_OPEN · unreviewed** | [#13](https://github.com/1doiliemkhoet/tlauchcraft/pull/13) | NOT_REPORTED | NOT_VERIFIED |
| [Chat 2](#chat-2--minecraft-core--singleplayer) | Minecraft Core & Singleplayer | **PR_OPEN_UNREVIEWED · IN_PROGRESS** | [#12 (draft)](https://github.com/1doiliemkhoet/tlauchcraft/pull/12) | AUTHORED_NOT_TESTED | NOT_VERIFIED |
| [Chat 3](#chat-3--mod-loader-adapters) | Mod Loader Adapters | **PR_OPEN · unreviewed** | [#8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8) | NOT_REPORTED | NOT_VERIFIED |
| [Chat 4](#chat-4--modpack-zip-import) | Modpack ZIP Import | **PR_OPEN · unreviewed** | [#10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10) | NOT_REPORTED | NOT_VERIFIED |
| [Chat 5](#chat-5--trusted-lan-multiplayer) | Trusted LAN Multiplayer | **PR_OPEN · unreviewed** | [#9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9) | NOT_REPORTED | NOT_VERIFIED |
| [Chat 6](#chat-6--offline-skin-manager) | Offline Skin Manager | **PR_OPEN · unreviewed** | [#11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11) | NOT_REPORTED | NOT_VERIFIED |
| [Chat 7](#chat-7--desktop-ui--packaging) | Desktop UI & Packaging | **PR_OPEN · unreviewed** | [#14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14) | NOT_REPORTED | NOT_VERIFIED |

---

## Individual reports

<!-- CHAT-01:START -->
## Chat 1 — Foundation & Persistence

- **Branch:** `feat/tl-01-foundation`
- **Snapshot branch HEAD:** `1a5980574e431e67c7de1aa135a65099877293e6` (Control snapshot on 2026-11-13; refresh this when reporting)
- **Issue:** [#1](https://github.com/1doiliemkhoet/tlauchcraft/issues/1)
- **PR to develop:** [#13](https://github.com/1doiliemkhoet/tlauchcraft/pull/13)
- **Status:** `PR_OPEN_UNREVIEWED` — branch and PR exist; no code acceptance implied.
- **Updated at:** 2026-11-13 (Control setup)
- **Summary / implemented functionality:** NOT_REPORTED_BY_CHAT
- **Changed paths / API impact:** NOT_REPORTED_BY_CHAT
- **Test commands and observed results:** NOT_REPORTED_BY_CHAT
- **Real device / Minecraft / modpack evidence:** NOT_TESTED_OR_NOT_REPORTED
- **Acceptance gates G0–G8:** NOT_VERIFIED (all; update relevant gates individually)
- **Blockers / dependencies / risks:** NOT_REPORTED_BY_CHAT
- **Pending decisions for Control:** Review PR and require supported test evidence
- **Next actions:** Chat 1 updates its own section with concrete results, commits and evidence URLs.

<!-- CHAT-01:END -->

---

<!-- CHAT-02:START -->
## Chat 2 — Minecraft Core & Singleplayer

- **Branch:** `feat/tl-02-game-core`
- **Branch HEAD:** [`9ec18994bfed00178f441bb2b412ff9d0285a362`](https://github.com/1doiliemkhoet/tlauchcraft/commit/9ec18994bfed00178f441bb2b412ff9d0285a362) (6 commits / 4 files / 547 additions in draft PR as checked)
- **Issue:** [#2](https://github.com/1doiliemkhoet/tlauchcraft/issues/2)
- **PR to develop:** [#12 — DRAFT, OPEN, NOT MERGED](https://github.com/1doiliemkhoet/tlauchcraft/pull/12)
- **Status:** `PR_OPEN_UNREVIEWED`, `IN_PROGRESS`; source primitives are **IMPLEMENTED** in the feature branch, not verified as a working launcher.
- **Updated at:** 2026-11-13 (Chat 2)
- **Implemented in source:** Standalone Rust `tl-game` crate with version inheritance resolver/cycle detection, ordered OS+feature metadata rules, version-dependent JVM/game arguments, asset paths, download planning (client jar/library/native archive/assets/log config), SHA1 validation, HTTPS origin restrictions, whole-file retries/staging/cancellation checks, Java major parsing/probe, OS argv-based child launch with stdout/stderr files. Does NOT implement complete metadata-to-game launch pipeline, Java provisioning or true verified Singleplayer.
- **Changed paths:** `crates/tl-game/Cargo.toml`, `crates/tl-game/src/lib.rs`, `crates/tl-game/src/metadata.rs`, `crates/tl-game/README.md`. Only Chat 2-owned crate modified; no shared Chat 1 contract or root workspace edits.
- **Test commands / actual outcomes:** Hermetic Rust tests **AUTHORED_NOT_TESTED**. Suggested Windows commands: `cargo test --manifest-path crates/tl-game/Cargo.toml`; `cargo fmt --manifest-path crates/tl-game/Cargo.toml -- --check`. **Actual executions: none.** No claim of passed tests or compiler validation.
- **Real device / Minecraft / modpack evidence:** **NOT_TESTED**. No verified downloaded Minecraft client, game PID, console trace confirming menu, in-game screenshot, Singleplayer world created/saved/reopened, or artifact hashes.
- **Acceptance gates:** G0 `BLOCKED` pending Chat 1 concrete Rust API; G1 `NOT_TESTED`; G2 `NOT_TESTED`; G3 `NOT_TESTED`; G4 `NOT_TESTED`. All `NOT_VERIFIED` by Control. Other gates outside Chat 2 ownership.
- **Blockers / risks:** Chat 1 root Cargo workspace and shared types/events not yet integrated; no verified Windows toolchain test; downloader lacks range resume/cache/repair transaction and robust symlink-ancestor checks; rule OS-version regex unimplemented; missing native extraction, full classpath/assets launch placeholders, manifest fetch, lifecycle monitor, verified compatibility and real-world E2E. Existing tests are no substitute for G1–G4.
- **Pending decisions for Control:** Approve shared-type adapter once Chat 1 API is published; require CI/tests and real Windows G2–G4 evidence before acceptance. Draft PR #12 must remain unmerged.
- **Next actions:** Compile/repair code on supported toolchain, finish metadata→verified install→Java→game runtime with safe native extraction, add hermetic integration fixtures, integrate Chat 1 public contract without changing another owner's crate, and capture real-world Windows menu/save/reopen evidence.

<!-- CHAT-02:END -->

---

<!-- CHAT-03:START -->
## Chat 3 — Mod Loader Adapters

- **Branch:** `feat/tl-03-loaders`
- **Snapshot branch HEAD:** `1177b93b99ad447dd258f6c9d0cd2277f4989bc0` (Control snapshot on 2026-11-13; refresh this when reporting)
- **Issue:** [#3](https://github.com/1doiliemkhoet/tlauchcraft/issues/3)
- **PR to develop:** [#8](https://github.com/1doiliemkhoet/tlauchcraft/pull/8)
- **Status:** `PR_OPEN_UNREVIEWED` — branch and PR exist; no code acceptance implied.
- **Updated at:** 2026-11-13 (Control setup)
- **Summary / implemented functionality:** NOT_REPORTED_BY_CHAT
- **Changed paths / API impact:** NOT_REPORTED_BY_CHAT
- **Test commands and observed results:** NOT_REPORTED_BY_CHAT
- **Real device / Minecraft / modpack evidence:** NOT_TESTED_OR_NOT_REPORTED
- **Acceptance gates G0–G8:** NOT_VERIFIED (all; update relevant gates individually)
- **Blockers / dependencies / risks:** NOT_REPORTED_BY_CHAT
- **Pending decisions for Control:** Review PR and require supported test evidence
- **Next actions:** Chat 3 updates its own section with concrete results, commits and evidence URLs.

<!-- CHAT-03:END -->

---

<!-- CHAT-04:START -->
## Chat 4 — Modpack ZIP Import

- **Branch:** `feat/tl-04-modpack`
- **Snapshot branch HEAD:** `736ff790e140cef0d32fbe6213aa0d26f34529f1` (Control snapshot on 2026-11-13; refresh this when reporting)
- **Issue:** [#4](https://github.com/1doiliemkhoet/tlauchcraft/issues/4)
- **PR to develop:** [#10](https://github.com/1doiliemkhoet/tlauchcraft/pull/10)
- **Status:** `PR_OPEN_UNREVIEWED` — branch and PR exist; no code acceptance implied.
- **Updated at:** 2026-11-13 (Control setup)
- **Summary / implemented functionality:** NOT_REPORTED_BY_CHAT
- **Changed paths / API impact:** NOT_REPORTED_BY_CHAT
- **Test commands and observed results:** NOT_REPORTED_BY_CHAT
- **Real device / Minecraft / modpack evidence:** NOT_TESTED_OR_NOT_REPORTED
- **Acceptance gates G0–G8:** NOT_VERIFIED (all; update relevant gates individually)
- **Blockers / dependencies / risks:** NOT_REPORTED_BY_CHAT
- **Pending decisions for Control:** Review PR and require supported test evidence
- **Next actions:** Chat 4 updates its own section with concrete results, commits and evidence URLs.

<!-- CHAT-04:END -->

---

<!-- CHAT-05:START -->
## Chat 5 — Trusted LAN Multiplayer

- **Branch:** `feat/tl-05-lan`
- **Snapshot branch HEAD:** `a2b81a1235c3a93b1c13d3c6248ffe20a93e1601` (Control snapshot on 2026-11-13; refresh this when reporting)
- **Issue:** [#5](https://github.com/1doiliemkhoet/tlauchcraft/issues/5)
- **PR to develop:** [#9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9)
- **Status:** `PR_OPEN_UNREVIEWED` — branch and PR exist; no code acceptance implied.
- **Updated at:** 2026-11-13 (Control setup)
- **Summary / implemented functionality:** NOT_REPORTED_BY_CHAT
- **Changed paths / API impact:** NOT_REPORTED_BY_CHAT
- **Test commands and observed results:** NOT_REPORTED_BY_CHAT
- **Real device / Minecraft / modpack evidence:** NOT_TESTED_OR_NOT_REPORTED
- **Acceptance gates G0–G8:** NOT_VERIFIED (all; update relevant gates individually)
- **Blockers / dependencies / risks:** NOT_REPORTED_BY_CHAT
- **Pending decisions for Control:** Review PR and require supported test evidence
- **Next actions:** Chat 5 updates its own section with concrete results, commits and evidence URLs.

<!-- CHAT-05:END -->

---

<!-- CHAT-06:START -->
## Chat 6 — Offline Skin Manager

- **Branch:** `feat/tl-06-skins`
- **Snapshot branch HEAD:** `551a2a21cdcd82fef68625c2538cf286096f910a` (Control snapshot on 2026-11-13; refresh this when reporting)
- **Issue:** [#6](https://github.com/1doiliemkhoet/tlauchcraft/issues/6)
- **PR to develop:** [#11](https://github.com/1doiliemkhoet/tlauchcraft/pull/11)
- **Status:** `PR_OPEN_UNREVIEWED` — branch and PR exist; no code acceptance implied.
- **Updated at:** 2026-11-13 (Control setup)
- **Summary / implemented functionality:** NOT_REPORTED_BY_CHAT
- **Changed paths / API impact:** NOT_REPORTED_BY_CHAT
- **Test commands and observed results:** NOT_REPORTED_BY_CHAT
- **Real device / Minecraft / modpack evidence:** NOT_TESTED_OR_NOT_REPORTED
- **Acceptance gates G0–G8:** NOT_VERIFIED (all; update relevant gates individually)
- **Blockers / dependencies / risks:** NOT_REPORTED_BY_CHAT
- **Pending decisions for Control:** Review PR and require supported test evidence
- **Next actions:** Chat 6 updates its own section with concrete results, commits and evidence URLs.

<!-- CHAT-06:END -->

---

<!-- CHAT-07:START -->
## Chat 7 — Desktop UI & Packaging

- **Branch:** `feat/tl-07-desktop`
- **Snapshot branch HEAD:** `9be7001d7c3ed6fc3f94d82639340d0d8c6fe46b` (Control snapshot on 2026-11-13; refresh this when reporting)
- **Issue:** [#7](https://github.com/1doiliemkhoet/tlauchcraft/issues/7)
- **PR to develop:** [#14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14)
- **Status:** `PR_OPEN_UNREVIEWED` — branch and PR exist; no code acceptance implied.
- **Updated at:** 2026-11-13 (Control setup)
- **Summary / implemented functionality:** NOT_REPORTED_BY_CHAT
- **Changed paths / API impact:** NOT_REPORTED_BY_CHAT
- **Test commands and observed results:** NOT_REPORTED_BY_CHAT
- **Real device / Minecraft / modpack evidence:** NOT_TESTED_OR_NOT_REPORTED
- **Acceptance gates G0–G8:** NOT_VERIFIED (all; update relevant gates individually)
- **Blockers / dependencies / risks:** NOT_REPORTED_BY_CHAT
- **Pending decisions for Control:** Review PR and require supported test evidence
- **Next actions:** Chat 7 updates its own section with concrete results, commits and evidence URLs.

<!-- CHAT-07:END -->

---

## Control log

- 2026-11-13 — Live ledger created; all seven PRs currently open; evidence not independently reviewed. Control owns merges and gate verification.
