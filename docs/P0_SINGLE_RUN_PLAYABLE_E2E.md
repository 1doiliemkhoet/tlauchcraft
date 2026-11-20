# P0 Single-Run Playable E2E — TLaunchCraft

**Integration branch:** `integration/p0-playable-single-run`. **Status: INTEGRATED CODE / NOT YET PLAYABLE / NOT YET TESTED IN GAME.**

## Provenance

This branch combines, in one integration tree, changes from:
- Chat 3 / PR #28: `512f5d44704db20d5dc50f4634ffd9324c549112` — Forge processor runtime (execution safety, NOT actual installed Forge).
- Chat 4 / PR #25: `6f8460226e0d09d268695da93d48bb57385ecf93` — CurseForge resolver/staging (NOT final installed pack).
- Chat 5 / PR #29: `7079bf737f8d5ffea69e813064c08291d7c32b49` — offline LAN identity parity/preflight (NOT two-client joined).
- Base `develop`: `edb42a915c10610e74b7743b812a86bcdd7a9372`.

Only shared modified file across these PRs is root `Cargo.lock`. Its three **local workspace crate** dependency blocks are reconciled from the corresponding PRs: `tl-loader` from #28, `tl-pack` from #25, `tl-lan` from #29. All other changed blobs come from the owning PR; no competing code-file edits. This **does not** prove Rust compilation or API contract compatibility. Do not auto-merge into `develop`.

## Hard blockers before the ONE real user test

1. Chat 3 + Chat 4 implement and connect **verified official Minecraft 1.16.5 + exact manifest-specified Forge installer** → fully resolved processor inputs → execution with explicit user approval and containment appropriate for its trust boundary → verified generated outputs → inherited Forge version launch spec.
2. Chat 4 implement complete authorized mod downloads/manual-file reconciliation, transactionally publish one **real, complete installed instance**; no `ready=true` from manifest or staging alone.
3. Desktop Tauri runtime must point **Play** to the above instance and use matching Java/runtime, save path, Forge mainClass, game args and offline username.
4. Chat 5 consume a machine-generated installed pack receipt; create matching second client install/fingerprint for **trusted private LAN**, stable offline usernames/UUID and confirmed game join.
5. Windows Cargo `fmt`, `clippy --locked`, `test --locked --workspace --all-targets`, desktop Tauri build and smoke tests must pass on the **combined integration HEAD** before inviting Control for the single game session.

**VirtualBox guestcontrol automation P1 deferred.** No VM redo. Do not weaken third-party execution provenance/checksum/approval boundaries to bypass it. Never mark processor or pack PASS from CI/preflight alone.

## Single Control acceptance session — one run_id, zero duplicate prep

Use **one new, disposable instance**, one chosen licensed/authorized modpack ZIP (candidate Rustic Waters II, only if all required files legally obtainable), exact Forge version from its real `manifest.json`, two distinct offline/non-premium accounts, two game clients on a **trusted private network**. No Microsoft login or public server.

1. **Install once:** Inspect pack ZIP → pin real manifest/archive SHA → resolve required mod artifacts and safe Forge installer/Java → execute/publish Forge+pack. Verify installed file inventory on both clients.
2. **Launch once:** From actual TLaunchCraft desktop UI click Play, observe Forge+modpack main menu and mod list, capture PID/log and no transient console, enter a **new disposable test world**.
3. **World persistence:** Place identifiable modded block/item, save and exit, reopen the **same world** in the same launcher instance and confirm it remains.
4. **LAN proof:** Host opens that same world via Open to LAN; record actual dynamic port, peer joins via private IP:port using second offline username; prove both players visible, shared modded item/chest state. Peer disconnects/rejoins with stable UUID, then verify persistence again.
5. **One evidence bundle:** same `run_id`, exact integrated commit, Minecraft/Forge/Java versions, pack manifest SHA, installed inventories, per-stage timestamps/verdicts, sanitized launcher/game logs, screenshot of main menu and both players, and test-world metadata. No tokens, world archives, pirated binaries or credentials in repo.

**Acceptance:** A/G2-G3-G5 requires actual Forge installed+pack ready+game launch; B/G4 requires world save+reopen; C/G6 requires real two-client modded LAN join+reconnect. Status of any missing link is `NOT_VERIFIED`; do not test separate chat branches or redownload the same vanilla assets.
