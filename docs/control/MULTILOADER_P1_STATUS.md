# P1 multi-loader expansion — implementation and verification status

**Date:** 2026-11-18  
**Base:** `integration/p0-playable-single-run` (PR #32)  
**Feature:** `feat/p1-multiloader-adapters-20261118` (PR #56, DRAFT)

## Baseline preserved

The only currently demonstrated modded 3D-world happy path is **Rustic Waters II, Minecraft 1.16.5 / Forge 36.2.29**, verified on a real Windows machine 2026-11-18. Forge 36.2.34 and 36.2.42 have official reviewed installer metadata and CI, but their real-world play remains `NOT_VERIFIED`. This branch does not change the approved 1.16.5 processor runner or the Forge publisher.

## Implemented in PR #56 (source/metadata gates, not new playable instances)

| Target | Exact reviewed release | Java | Technical evidence | Runtime state |
| --- | --- | --- | --- | --- |
| Forge 1.12.2 | 14.23.5.2860 | 8 | Official installer SHA-1 `5e8a91f71ef3d1f77de3f8d3261aedcc2f551c9d`, legacy install profile (0 processors), version inheritance | **NEEDS_LEGACY_EXECUTOR** |
| Forge 1.18.2 | 40.2.21 | 17 | SHA-1 `5507eebfe3ca62545419e7aa8ee7b5dc559f0e26`, official 10-processor metadata graph | **NEEDS_APPROVED_17_EXECUTOR** |
| Forge 1.20.1 | 47.3.0 | 17 | SHA-1 `0deb8e547a9e3f098a4c9f082903499703c52f5d`, official 10-processor metadata graph | **NEEDS_APPROVED_17_EXECUTOR** |
| Fabric 1.20.1 | loader 0.16.10 | 17 | Official Meta JSON has 8 Maven libraries with SHA-1 and sizes; Rust profile validation and local Maven JAR SHA-1 rehash added | **NEEDS_PUBLISH/PLAY_INTEGRATION** |
| NeoForge 1.21.1 | 21.1.80 | 21 | Official installer SHA-1 `191c8211ffd2d8e754359d15e96a0441a66f2eac`, matching `neoforge-21.1.80` profile and 10 processors | **NEEDS_APPROVED_NEO_EXECUTOR** |

Source: official Forge download pages at files.minecraftforge.net, Fabric Meta API, official NeoForged Maven installer, and [live metadata CI](https://github.com/1doiliemkhoet/tlauchcraft/actions/workflows/p1-multiloader-metadata-audit.yml). **Metadata audit PASS:** [run 44246172080](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/44246172080), including Rust inspection of actual Forge 1.12/1.18/1.20 and NeoForge 1.21 installer metadata. NeoForge processor classpath uses explicit `@jar` Maven types; the inspected parser allows that one type only and rejects unsupported extensions. The audit downloads and extracts **data only**, never invokes `java -jar` or installer processors.

The source changes introduce `loader_matrix.rs`, `fabric_runtime.rs`, pinned SHA-1 planning for the listed releases, exact game/loader binding, Java requirement tracking, and fail-closed NeoForge planning. The audit of local Fabric artifacts requires SHA-1 to match the hash in the original official Fabric profile and does not grant `instance_ready`.

## Do not equate these gates

1. **Discovery:** the Maven / Fabric endpoint lists a release.
2. **Source verified:** exact official installer/profile version, SHA-1 and game binding match.
3. **Runtime verified:** all approved Java processors (or full Fabric classpath), dependencies, generated Minecraft JARs/assets/natives pass independent verification.
4. **Installed:** a complete, transactionally published modpack instance and durable receipt are rechecked before Play.
5. **Playable:** player starts the launcher, reaches a real Minecraft world and provides Windows game evidence.
6. **Persistence/LAN:** world survives restart and two offline users join a compatible modded LAN world.

**PR #56 currently advances gates 1–2 for Forge/NeoForge and a portion of gate 3 for Fabric's local library rehash; it DOES NOT complete gates 3–6 for new loaders.** In particular: `LoaderPlan.ready_for_metadata_merge` does not imply `GameInstance.state=Ready`.

## Remaining engineering work (P1 implementation acceptance)

### Forge 1.12.2

- Dedicated legacy launcher profile/universal JAR assembly rather than the modern four/ten-processor runners.
- Java 8, LaunchWrapper classpath and game arguments, official Forge library hashes, safe staging and rehash.
- Independent receipt/publisher and Desktop Play without touching the existing 1.16.5 path.

### Forge 1.18.2 and 1.20.1

- Separate Java 17 process policies, exact official 10-processor allowlist and output provenance/hashes.
- Mojang/Forge merged metadata, generated client artifacts and modern launch targets.
- Staged mod inventory, sealed execution proof and durable Ready/Play re-verification.

### Fabric

- Official Meta profile fetch with strict HTTPS, bounds and version matching.
- Download pinned Maven libraries from allowlisted sources and integrate the SHA-1 rehash proof already in tl-loader.
- Validate all inherited Mojang client/assets/natives; build a separate Fabric publish receipt and Desktop Play path.
- Real Windows modpack launch screenshot. A bare parsed Fabric JSON is not sufficient.

### NeoForge

- Verified 21.1.80 archive/version/profile binding already covered by metadata audit.
- Separate 10-processor allowlist, Java 21 execution policy, output checks and publish/Play bridge.
- No silent fallback to Forge 1.20.1 or inferred `20.4.x` compatibility.

## Regressions / release policy

The P0 Rustic Waters II instance must remain untouched. Unverified source hashes, untrusted URLs, symlinks, incomplete libraries, unknown processor graphs or missing mod downloads must return a clear `NeedsAction` or error — **never READY**. Keep feature work on draft PR #56 until all relevant GitHub Windows checks pass and real game evidence exists. Do not merge into develop without Control review.

### CI

- P1 official metadata verification: [GitHub Actions workflow](https://github.com/1doiliemkhoet/tlauchcraft/actions/workflows/p1-multiloader-metadata-audit.yml) — pinned archives and metadata only.
- Cross-generation Rust fixture tests: `cargo test --locked -p tl-loader --all-targets`.
- Full regression: `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace --all-targets`, Desktop/Tauri check, NSIS build/smoke.
- Gameplay acceptance: **NOT_VERIFIED** for all new target releases.


## Follow-up: installed profile inventory and Java selection

- Local installed folder includes bundled/manual JARs as hash-bound archive overrides, with 0/0 reserved for no provider identity. Original manifest IDs remain validated; no additional executable is implicitly trusted by provider SHA-1. Exact inventory and immutable hashes are verified again before publication and Play.
- Host execution consent explicitly covers all JARs in the selected installed folder.
- Java discovery now accepts majors 8, 17, 21, including standard vendor and managed Minecraft runtime locations. Installed-folder preview selects the required major for the target Minecraft versions. No download or silent Java installation.
- Preview reports unsupported loader adapter before staging. Forge 1.12.2, 1.18.2, 1.20.1, Fabric and NeoForge remain metadata-only; no real gameplay claim.
- Regression fixtures: bundled JARs, source mutation after snapshot, additional JAR after snapshot, profile preview compatibility matrix, Tauri transport. Windows CI and real profile acceptance pending.
