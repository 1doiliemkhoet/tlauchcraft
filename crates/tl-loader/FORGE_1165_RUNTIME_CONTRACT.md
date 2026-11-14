# Forge 1.16.5 Processor Runtime Contract (Chat 3 → Chat 4)

## What this implements

`tl_loader::runtime::run_verified_processors(&ProcessorRunRequest) -> ProcessorRunReport` executes pre-approved **Java 8** processor tools from checksum-verified local JARs. It never downloads artifacts, unpacks arbitrary archives, modifies an instance/world, or calls a shell. It returns explicit `Verified` or `Failed` and processor exit codes, logs and SHA-1-verified staging outputs. Verified means **only this processor stage**, not an installed Forge game.

## Trust boundary / required caller inputs

1. Chat 4 or a Control-approved adapter extracts the official Forge 1.16.5 installer JAR into a fresh private directory (reject traversal/symlinks). Pin installer SHA-1 from [official Forge downloads](https://files.minecraftforge.net/net/minecraftforge/forge/index_1.16.5.html); never take a downloaded checksum from an untrusted mirror.
2. Read `install_profile.json`, `version.json`, the `data` table, library descriptors and `processors` **from that verified installer**. Resolve Maven `[coordinate]` references to local, checksum-verified dependency files and `{TOKEN}` to safe staging paths/hashes. `inspect_install_profile` offers initial compatibility validation; it does not itself resolve these tokens.
3. Supply each `VerifiedProcessorJar` with its local path, SHA-1, canonical Maven source, and the approved mainClass matching the processor coordinate. Do not treat a downloaded manifest's free-form mainClass as trusted; the runner uses a narrow legacy 1.16.5 entrypoint allowlist. The Java 8 executable must be explicitly selected/probed; no system PATH fallback.
4. Supply complete **expanded argument vector** and **declared SHA-1 output paths inside staging** for each client processor. No network operations are permitted inside the runner; the caller must cache all library, mapping and Minecraft inputs before launching any processor.
5. `stage_root` must be a new, dedicated private directory outside any user game/world or shared cache; `run_verified_processors` refuses an existing stage. On processor failure, the stage is not committed; it remains for log inspection. The caller performs explicit cleanup after collecting evidence.
6. Only once the runner returns `Verified`, **the caller** validates the inherited Forge version JSON, generated libraries, mainClass, Java requirements and exact complete file manifest. Chat 4 / FileInstaller owns the final transactional commit/rollback into a separate game instance. Nothing in this API returns `InstanceState::Ready` or a `G2 PASS`.

## Forge 1.16.5 metadata mechanics

A modern 1.16.5 Forge installer includes `install_profile.json` and a derived `version.json`. The profile specifies the target Minecraft release, artifact coordinates, data substitutions, ordered processor JARs/classpaths, side filters and expected generated outputs (often hashes through `data` tokens). Client-only processors must run in declared order; a processor output may be input to a later processor. The derived version inherits the Mojang 1.16.5 parent and supplies Forge libraries / launch mainClass; use `tl_game::inherit` rather than inventing a separate launch command. Never assume universal JAR alone is the final playable client.

The public official page lists 1.16.5 Forge Recommended 36.2.34 and Latest 36.2.42. The matrix for Rustic Waters II **must use the exact Forge version from the pack manifest**, not blindly choose latest. Forge's published Java compatibility guidance is Java 8 for 1.16.5 and older.

## Known missing integration

- Extraction of the verified Forge installer ZIP and parsing/resolution of all `data` entries, per-processor side rules and declared official hashes. Requires a separate approved adapter; no third-party binaries are bundled.
- Chat 2's `tl_game::install_file` currently restricts source domains to Mojang. Control must approve a dedicated official Forge Maven downloader extension with explicit origin/sha and immutable cache semantics. Chat 3 does not edit `tl-game`.
- Real Forge 1.16.5 processor execution, installation evidence and a modded Minecraft launch have **not** been performed. Fixture tests validate failure handling and safety, not an E2E installation.

## Handoff acceptance for Chat 4

Integration must provide: `minecraft_version`, `loader.version`, official installer checksum/provenance, full verified dependency list, `ProcessorRunRequest`, raw `version.json` plus inherited parent, verified output list, staged instance destination and logs. Chat 4 should fail `NeedsAction` or `Failed` if any required input/hash/processor is unavailable, and never mark `Ready` on mere metadata or spawn success.
