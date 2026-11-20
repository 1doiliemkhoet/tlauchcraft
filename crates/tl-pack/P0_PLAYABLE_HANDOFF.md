# CHAT 4 — P0 Pack Install/Play handoff

**Integration base:** `integration/p0-playable-single-run`, original SHA `5774bd4750a90b9141a0ec8718659aadaf0259f6`.

**Proposed PR:** #35 (Draft; no auto merge). Never alter `develop` or a previous Vanilla world.

## Machine-readable backend steps already implemented
1. `pack_inspect`: ZIP format, all CurseForge manifest project/file IDs, Minecraft and Forge versions, archive SHA-256.
2. `pack_resolve`: official CurseForge API (API key only at `api.curseforge.com`); explicit `NeedsManualDownload`, `PermissionDenied`, invalid metadata or missing hashes.
3. `pack_stage_curseforge`: private, create-new durable journal; official-CDN anonymous checked downloads; reject untrusted hashes and unsafe names; emit real progress phases with no fabricated byte percentages. `stage_transaction` persists interruption evidence.
4. `pack_import_restricted`: user chooses their locally obtained authorized JAR, backend re-resolves exact CurseForge fileID/projectID and independently verifies upstream SHA-1 and official byte count before no-clobber import.
5. `pack_staged_receipt`: backend independently rehashes staged bytes, returns `StagedPackReceipt` with Minecraft/Forge/ZIP SHA-256, each mod ID and observed SHA-256 plus required/optional blocked reasons. `instance_ready=false` regardless of inventory completeness.

The Desktop page exposes these phases and errors. **No usable modded instance is published and no Forge Play is enabled by these actions.** Vanilla flows remain untouched.

## Blocked production steps — do not bypass
- Chat 3 must complete independently verified **real Forge 1.16.5 processor execution** inside approved isolation, including 4 processor outputs, derived version JSON, libraries, Java 8 and hashes. Plan/preflight/Job Object tests alone are not execution evidence. Its current `LoaderInstallationHandoff.can_publish_instance=false` is deliberate.
- Only after loader-origin verified evidence can a backend-only adapter mint `VerifiedPublishPermit`, validate the *entire* stage including generated version/Java/game assets and atomic-publish a new game directory without overwriting saves.
- SQLite must register that verified instance with the existing offline profile, correct Forge version, runtime paths, JVM/classpath and RAM settings; `game_ops.rs` must then launch verified Forge metadata instead of its current Vanilla-only contract. This stage is **not implemented yet**.
- Issue #31 should coordinate one final Windows session: A) Forge + complete modpack → main menu; B) save/reopen world; C) two offline clients LAN host/join/reconnect. Independently mark gates; **no fixtures/CI count as gameplay**.

## Reference pack, not a hardcoded launcher limitation
Control inspected Rustic Waters II 1.16: Minecraft `1.16.5`, Forge `36.2.29`, 200 file references and 2,524 ZIP entries. The real ZIP remains on Control Windows and is not present in this GitHub runner. Downloadable/blocked/verified totals remain **NOT_MEASURED**; never substitute fake API fixture counts.

## Evidence and safety
- No CurseForge key, user world, mod binary or Minecraft cache is committed.
- Manual files with no independently published SHA-1/size remain `NeedsAction`.
- Existing Vanilla instances/worlds are untouched.
- Windows integration CI: https://github.com/1doiliemkhoet/tlauchcraft/actions/workflows/chat4-p0-pack-integration.yml
