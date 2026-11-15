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

## 2026-11-15 — Typed graph preparation API (new)

```rust
use tl_loader::forge_graph::{
    prepare_forge_1165, ForgeBindings, BoundValue, PrepareReport, PrepareState
};
let result: PrepareReport =
    prepare_forge_1165(verified_install_profile_json, &verified_version_json, &bindings);
match result.state {
    PrepareState::Verified => {
        // result.request holds a *prepared*, non-executed ProcessorRunRequest.
        // DO NOT execute until Control approves the full official source/checksum
        // and the caller independently supplies a private OS-isolated run context.
    }
    PrepareState::NeedsAction => { /* resolve and verify result.unresolved */ }
    PrepareState::Failed => { /* show result.error and abort without touching instance */ }
}
```

- `ForgeBindings` is caller-owned. It supplies exact Minecraft/Forge version, explicit Java path, **new private staging root**, timeout, `artifacts` keyed by Maven coordinate (each as `VerifiedProcessorJar`), and explicit `data` bindings. No network download or Java spawning occurs during preparation.
- `BoundValue::VerifiedExternal(VerifiedProcessorJar)` is subject to local SHA-1 validation **before** a request is created. Supported sources are canonical Forge Maven/Maven Central/Minecraft library paths, and Mojang's checksum-addressed client JAR. Maven references with `@zip`/`@txt` are recognized for input resolution.
- `BoundValue::Staged(relative_path)` points inside `stage_root`; absolute, parent traversal, colon, and backslash are rejected. All processor output paths must bind to a staged path and supply a 40-hex output hash. Duplicate outputs are refused.
- `BoundValue::Literal` is for scalar arguments only; not a file path or an unverified Maven reference. Unbound `{KEY}`, `[Maven]`, missing client data, unapproved processor coordinates, and missing output hashes cannot produce `Verified`.
- The client graph respects upstream processor order and excludes server-only steps. It never trusts a `mainClass` field from installer metadata; the runtime maps approved tool coordinates to audited Java entrypoints. A missing artifact causes `NeedsAction`, not an auto-download.
- **Current limits:** Full installer archive extraction and official 1.16.5 manifest byte-for-byte fixture are not available in this environment. The representative `tests/fixtures/forge_1165_representative.json` is **schema-based, synthetic**, not a claim of official extraction. No bundled Forge binaries; no actual processor execution.
- **Official provenance checkpoint:** Forge's official version page publishes SHA-1 `ab306b654c44d659ce69da5d4f87590b66dc91e8` for `forge-1.16.5-36.2.34-installer.jar`. Treat this digest as a gate for archive inspection, **not** permission to execute. Official Maven path: `https://maven.minecraftforge.net/net/minecraftforge/forge/1.16.5-36.2.34/forge-1.16.5-36.2.34-installer.jar`. Actual archive contents not inspected here because runtime networking to that host failed.
- **Caution:** A private filesystem staging directory is not an OS sandbox. If Control approves execution, a separate restricted process/container/OS policy and safe file-write boundary are still needed. The `ProcessorRunReport::Verified` state applies only to validated processor outputs, never a complete Forge installation.

### Chat 2/4 dependency contract

Chat 2's official verified downloader must provide read-only immutable artifacts from an explicit Forge/Mojang Maven allowlist with expected upstream checksums (never arbitrary mirrors); Chat 3 remains loader-only and does not modify `tl-game`. Chat 4 must extract the **already checksum-verified** official installer (including `data/*.lzma`) safely, bind client-only data references, satisfy all processor dependencies, and commit a complete staged instance transactionally only after output verification. All compatibility with Rustic Waters II must be chosen from that pack's manifest. G2/G3/G5 remain NOT_VERIFIED.

## P0 real-installer read-only inspection (Control handoff)

The official Forge downloads page pins the **1.16.5-36.2.34 installer SHA-1** to `ab306b654c44d659ce69da5d4f87590b66dc91e8`:
<https://files.minecraftforge.net/net/minecraftforge/forge/index_1.16.5.html>.

**Windows PowerShell, at the repository root, three commands:**

```powershell
$installer = "$PWD\\forge-1.16.5-36.2.34-installer.jar"; Invoke-WebRequest -Uri 'https://maven.minecraftforge.net/net/minecraftforge/forge/1.16.5-36.2.34/forge-1.16.5-36.2.34-installer.jar' -OutFile $installer
(Get-FileHash -LiteralPath $installer -Algorithm SHA1).Hash.ToLowerInvariant()
& .\\crates\\tl-loader\\scripts\\inspect-forge-1165.ps1 -Installer $installer -MetadataDir "$PWD\\forge-1165-metadata" -ReportPath "$PWD\\forge-1165-readonly-report.json"
```

The SHA-1 printed in command 2 **must match** the pinned value. The script **re-verifies** SHA-1 before inspecting any ZIP entries, refuses unsafe/duplicate names, bounds uncompressed ZIP bytes and manifest sizes, reads only two JSON entries, and optionally extracts **only these two metadata JSON files** into a newly created metadata directory. No executable JAR or archive contents are extracted, no Java is launched, no game files are modified. Existing destinations are refused.

The JSON report contains installer filename, verified SHA-1, size, actual `install_profile.version`, derived runtime version, all/client processor counts, processor JAR/classpath/arguments, output references, library counts, and data-token keys. Control must attach that JSON plus the two manifests as evidence and compare them against the `forge_1165` parser before authorizing any processor execution. This is **inspection evidence only**, not G2 or G5.

### Explicit safety and integration gaps

- The new `forge_graph::prepare_forge_1165` resolves `[MavenCoordinate]` from either a `BoundValue::VerifiedExternal` override or an entry in `ForgeBindings.artifacts`. Both must have a canonical source and valid local checksum. Absent artifacts produce `NeedsAction`; wrong hash produces `Failed`.
- The current Forge processor runner should **not** be enabled to run third-party JARs until Control approves executable hashes/versions, OS isolation, a complete graph and the full input/output manifest. The staging directory alone is not an OS sandbox.
- The existing hand-authored representative JSON fixture is **synthetic**, not copied from the 36.2.34 JAR. Control must provide the read-only report/metadata before we can report real processor counts or claim real installer-profile support.
- `tl-game` downloader changes and Chat 4 transactional modpack installation remain owned by those chats. No unauthorized module changes in this PR.

## Control P0, Forge 36.2.29 (2026-11-15)

**Selected target is now 1.16.5–36.2.29**, NOT 36.2.34. The previous 36.2.34 instructions in this document are preserved as historical context only. Official Forge downloads independently lists installer SHA-1 `7d247279cf06eed6ffcf609f36d11e34c70350f3` for 36.2.29. The user-reported local evidence from Control is: `1.16.5-forge-36.2.29`, `inheritsFrom=1.16.5`, `mainClass=cpw.mods.modlauncher.Launcher`, **4/4 client processors**, **39 profile libraries**.

The inspector at `crates/tl-loader/scripts/inspect-forge-1165.ps1` now pins this version and SHA-1, refuses mismatched checksums, checks the observed profile and processor/library counts, and exports a dependency + processor inventory. This script cannot execute any processor. It reads the two JSON manifest entries from the ZIP only after hash verification, optionally writing them into a new metadata directory.

**Windows Control (up to 3 commands, PowerShell at repository root):**

```powershell
git fetch origin; git checkout feat/tl-03-forge-runtime; git pull --ff-only origin feat/tl-03-forge-runtime
(Get-FileHash 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -Algorithm SHA1).Hash.ToLowerInvariant()
& .\crates\tl-loader\scripts\inspect-forge-1165.ps1 -Installer 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -MetadataDir "$PWD\forge-36229-metadata" -ReportPath "$PWD\forge-36229-inspection.json"
```

The printed SHA-1 **must equal** `7d247279cf06eed6ffcf609f36d11e34c70350f3`; command 3 verifies it independently. The third command creates the report and two exported manifests and refuses pre-existing destinations. Do **not** copy the JAR into the repository or commit evidence/artifacts containing private absolute paths without review.

### Chat 4 acceptance boundaries

- API: `forge_graph::prepare_forge_1165(&str, &serde_json::Value, &ForgeBindings) -> PrepareReport`. `PrepareState::NeedsAction` reports required artifact coordinates/data entries, and **never emits an executable request** when dependencies are missing. `PrepareState::Verified` means *request assembled and local input checks passed*, not Forge installed.
- Only after Control approves every executable JAR's official source, checksum and restricted execution context, Chat 4 may submit `ProcessorRunRequest` to `runtime::run_verified_processors`. It then checks `ProcessorRunReport::outcome == ProcessorOutcome::Verified` and independently confirms final game version inheritance and generated library manifest before transactional commit. No user world path may be a staging root.
- The 4-processor unit graph regression is **constructed using the Control-confirmed processor count**, *not* a dump of the official 36.2.29 processors, classpath coordinates or output hashes. Those individual values remain unknown until Control exports the verified `install_profile.json` and `version.json`. Do not substitute invented coordinates for real dependencies.
- Forge Maven downloads remain Chat 2/Control-owned: only canonical official URLs and digest-pinned immutable local cache are acceptable. Any downloaded executable processor needs explicit authorization, and private staging **does not imply OS sandboxing**.
- Real installer extraction on Control, real processor invocation, exact generated output hashes, Forge launch and Rustic Waters II gameplay remain **NOT_VERIFIED** in Chat 3 until produced evidence is attached.

## General loader runtime adapter and Chat 4 evidence (post PR #27 develop sync)

`tl_loader::loader_runtime` is the **version-aware extension point** for a loader runtime without hard-coding `36.2.29` into shared installation types:

```rust
use tl_loader::loader_runtime::{
    forge_adapter_for, LoaderRuntimeAdapter, RuntimeState,
    LoaderRuntimeVerification, verification_from_prepare, verification_from_execution,
};

let adapter = forge_adapter_for("1.16.5", "36.2.29")?;
let plan = adapter.prepare(verified_profile, &verified_derived_version, &bindings);
let evidence = verification_from_prepare(
    tl_loader::LoaderKind::Forge, "1.16.5", "36.2.29", &plan
);
// Even PrepareState::Verified yields RuntimeState::NeedsAction:
// Java processor execution requires separate Control approval and OS isolation.
```

- The common `LoaderRuntimeAdapter` interface accepts metadata and verified `ForgeBindings`. `forge_adapter_for` enables the **legacy 1.16.5 / 36.2.x Forge family only**. Unimplemented Minecraft/Forge versions return `UnsupportedVersion` deliberately. Fabric, NeoForge, Quilt runtime adapters are NOT implemented by this change; presence of metadata parsers elsewhere in `tl-loader` is not runtime E2E support.
- `verification_from_execution` cross-checks the runner outcome, completed processor count, zero exit codes and exact **number** of declared output hashes. The underlying `run_verified_processors` checks actual SHA-1 bytes of each output. Runtime verification is not a replacement for rechecking final staged files, dependency inventory, version inheritance and transactional instance commit in Chat 4. `LoaderRuntimeVerification.instance_ready` is **always false**.
- Missing processor executable and classpath dependencies now accumulate into `PrepareState::NeedsAction` diagnostics instead of returning early on the first missing tool. Graph resolution never triggers downloads and never starts Java.
- The SHA-1-pinned 36.2.29 Windows inspector and user's confirmed 4 processors / 39 libraries are **provenance and shape evidence only**. No official exported metadata was available in this chat to validate four *specific* processors. The previous synthetic fixture remains marked synthetic; it cannot be used to assert E2E.
- Existing `runtime::run_verified_processors` remains gated by the caller: there is no new executable invocation in `loader_runtime`. Neither staging directory nor checksum validation is an OS sandbox. Before any third-party JAR runs, Control must approve explicit sources/hashes and a restricted process context.
- Baseline merge commit includes Chat 7's merged Windows Vanilla/UX improvements; no changes to `tl-game`, `tl-pack`, Desktop, profile storage or user worlds.

### Control inspection commands (PowerShell, project root)

```powershell
git fetch origin; git switch feat/tl-03-forge-runtime; git pull --ff-only origin feat/tl-03-forge-runtime
(Get-FileHash 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -Algorithm SHA1).Hash.ToLowerInvariant()
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\crates\tl-loader\scripts\inspect-forge-1165.ps1 -Installer 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -MetadataDir "$PWD\forge-36229-metadata" -ReportPath "$PWD\forge-36229-inspection.json"
```

Run the last command only for this reviewed, trusted repository script. Process-scoped `-ExecutionPolicy Bypass` does not change machine/user policy persistently; it is not a recommendation to disable system-wide policy. Confirm printed SHA-1 `7d247279cf06eed6ffcf609f36d11e34c70350f3`. Existing metadata output directories must be renamed or moved before re-running because the inspector intentionally refuses overwrite.

## Control P0 — REAL Forge 36.2.29 metadata regression, processor DAG and provenance

Source **actual Control-uploaded `install_profile.json` + `version.json`**, from official installer `forge-1.16.5-36.2.29-installer.jar`, SHA-1 `7d247279cf06eed6ffcf609f36d11e34c70350f3`, 8,014,866 bytes / 355 ZIP entries. Sanitized fixtures in `tests/fixtures/forge_36229_official_{profile,version}.json` preserve **all** data keys, real processors, libraries, derived game launch arguments and SHA-1 metadata; only giant `icon` and unrelated UI fields were dropped. Do not relabel previous hand-authored fixture as official.

| Step | Trusted processor | Classpath | Declared SHA-1 outputs | Tracked intermediate |
| --- | --- | --- | --- | --- |
| 0 | `net.minecraftforge:installertools:1.2.6` | 17 | none | `MAPPINGS` via `--output` |
| 1 | `net.minecraftforge:jarsplitter:1.1.4` | 2 | `MC_SLIM`, `MC_EXTRA` | none |
| 2 | `net.md-5:SpecialSource:1.8.5` | 7 | none | `MC_SRG` via `--out-jar` |
| 3 | `net.minecraftforge:binarypatcher:1.0.12` | 11 | `PATCHED` | none |

Outputs without official SHA-1 are **not** treated as checksum-verified: the runner checks regular nonempty files, no symlink ancestors, records locally observed SHA-1 + producer index + size, and verifies downstream ordering. Where an expected official SHA-1 exists it is mandatory and checked before continuing. Typed Chat 4 handoff **rechecks** declared output file bytes and intermediate evidence; `instance_ready` is always false. A plan or successful process exit is not evidence that the final Forge instance exists, is launchable or is ready.

`SpecialSource:1.8.5` main class `net.md_5.specialsource.SpecialSource` is corroborated by the 1.8.5 published Maven POM and the exact official installer-profile SHA-1 `c670c493c436a885c872fa4810bc698628b95922`. The runtime accepts **only** that coordinate/main class/digest combination, not arbitrary main classes. **The executable JAR's MANIFEST.MF has not yet been byte-inspected on Control**, so Control must perform that final check before approving any execution. Main-class implementation allowlist is not process-execution authorization.

All processor/classpath/[Maven] inputs, including `@zip`, must match the **official profile library URL and SHA-1** as well as the bytes on disk, not merely a caller-supplied digest. `MINECRAFT_JAR` is an explicit `VerifiedExternal` bound to checksum-addressed official Mojang 1.16.5 client metadata (legacy `launcher.mojang.com` or `piston-data.mojang.com`); absence = `NeedsAction`. Never use arbitrary `.minecraft` paths or touch saves.

`BINPATCH.client=/data/client.lzma` has no Maven URL. The reviewed `scripts/extract-forge-binpatch.ps1` verifies the official installer SHA-1 **before ZIP access**, extracts exactly `data/client.lzma` into a **new evidence directory** (bounded size, refuses existing destinations and user world paths) and writes `binpatch-provenance.json`. A `BoundValue::VerifiedInstallerEntry(InstallerEntryEvidence)` requires matching installer SHA, on-disk BINPATCH SHA, file size and provenance report. **The caller must trust/approve the extraction procedure; the report itself is not cryptographically signed.** Without this evidence, `NeedsAction`. No JAR execution by this script.

`forge_provenance::audit_official_forge_metadata` inspects the 39 profile libraries and 26 derived libraries, authenticates canonical URL+Maven coordinate+SHA and explicitly lists source gaps for **Forge Universal SHA-1 `b6cec9bf1744a52467bf62a30ae827b772af69b8`** and **derived Forge runtime SHA-1 `42333c69e94ce972b2f589cf3d816ba80672d373`**, because both official entries have **empty download URLs**. No URL is guessed and neither artifact is treated as installed; Chat 4 must supply verified installer/generated library evidence. Preserve `version.json` game arguments and use `tl_game::inherit` rather than manufacturing launch command.

### Windows Control: max three PowerShell commands

```powershell
git fetch origin; git switch feat/tl-03-forge-runtime; git pull --ff-only origin feat/tl-03-forge-runtime
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\crates\tl-loader\scripts\inspect-forge-1165.ps1 -Installer 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -MetadataDir "$PWD\forge-36229-metadata-new" -ReportPath "$PWD\forge-36229-inspection-new.json"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\crates\tl-loader\scripts\extract-forge-binpatch.ps1 -Installer 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -DestinationDirectory "$PWD\forge-36229-binpatch-new"
```

Use fresh destination names on repeat runs. The `ExecutionPolicy Bypass` is **process-scoped** and applies only to reviewed repository scripts. The extracted material is never sent to a Java process by these commands. No existing world/profile is read or changed by the inspection/extraction scripts.

**Remaining blockers:** official Java 8 tool JARs/classpath/MCP ZIP/Mojang client bytes are not present in this Chat 3 runtime; Forge Universal and derived Forge runtime JARs require vetted local generation/extraction; actual processor execution needs Control-approved binary hashes **and OS-level containment**. G2/G3/G5 are **NOT_VERIFIED**. The adapter framework is loader/version-aware but **only this Forge legacy family is currently implemented**; no claim of Fabric/Quilt/NeoForge processor runtime or Forge E2E support.

## Final Control gate — 2026-11-15: approved execution / cancellation / Chat 4 handoff

The historical `run_verified_processors` entrypoint is now intentionally **fail-closed** (returns `ProcessorOutcome::Failed` without starting Java). It must not be used to infer an executable authorization from `PrepareState::Verified`. `run_approved_processors(request, permit, &AtomicBool)` is the explicit entrypoint. Its `ExecutionPermit` requires the **Control-approved executable SHA-1 inventory**, a private approved absolute parent directory, a fresh child stage, an absolute Java executable and a caller assertion that a **host-enforced restricted environment already exists**. It rejects `.minecraft`, `saves`, `worlds`, `instances` and `profiles` stage ancestry, validates symlink-free stage ancestry, and polls cancellation (including kill/wait of the running child). The normal timeout, exit code, input digest and output digest checks remain. **A permit field is not an OS sandbox**: the caller is responsible for actual Windows Job Object/process tree handling, ACLs, network policy and limited permissions. **Do not grant a permit until Control approves those controls**, including all tool digests and independently reviewed SpecialSource MANIFEST.

The typed `loader_runtime::installation_handoff` combines `LoaderRuntimeVerification`, `ForgeProvenanceAudit` and observed `VerifiedProcessorJar` entries. It carries Minecraft/loader kind and versions, **required Java 8**, output/intermediate evidence, fully SHA-1-verified local Maven libraries, outstanding `NeedsAction` dependencies and `can_publish_instance=false`. Even `ProcessorsVerified` **cannot publish** an instance. Chat 4 alone handles validated Forge inheritance, all generated files and libraries, atomic instance installation, rollback and later gameplay verification.

### Windows PowerShell 5.1 acceptance in three short steps

**Step 1 — SHA-1-pinned read-only inspection** (fresh destination names each attempt):

```powershell
git fetch origin; git switch feat/tl-03-forge-runtime; git pull --ff-only origin feat/tl-03-forge-runtime
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\crates\tl-loader\scripts\inspect-forge-1165.ps1 -Installer 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -MetadataDir "$PWD\forge-36229-metadata-final" -ReportPath "$PWD\forge-36229-inspection-final.json"
```

**Step 2 — BINPATCH provenance, extraction only:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\crates\tl-loader\scripts\extract-forge-binpatch.ps1 -Installer 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -DestinationDirectory "$PWD\forge-36229-binpatch-final"
```

**Step 3 — independently verify all reports/hashes, without extraction or Java:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\crates\tl-loader\scripts\verify-forge-evidence.ps1 -Installer 'C:\Users\admin\Downloads\forge-1.16.5-36.2.29-installer.jar' -InspectionReport "$PWD\forge-36229-inspection-final.json" -MetadataDirectory "$PWD\forge-36229-metadata-final" -BinpatchDirectory "$PWD\forge-36229-binpatch-final"
```

**Optional, only if Control has independently obtained the actual official SpecialSource 1.8.5 JAR:** `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\crates\tl-loader\scripts\verify-specialsource.ps1 -Jar '<approved-path-to-SpecialSource-1.8.5.jar>'`. It SHA-1-checks official `c670c493c436a885c872fa4810bc698628b95922` before reading `META-INF/MANIFEST.MF` and requires `net.md_5.specialsource.SpecialSource` plus the matching class entry. Until actually run against the real JAR on Control, **Main-Class remains NOT_VERIFIED from executable bytes**.

The PowerShell scripts do **not run Java** and do not modify existing Minecraft worlds or profiles. Failing/absent inputs never imply READY. `Forge Universal` (SHA-1 `b6cec9bf1744a52467bf62a30ae827b772af69b8`) and derived Forge runtime (SHA-1 `42333c69e94ce972b2f589cf3d816ba80672d373`) remain `NeedsAction` due to no publisher URL; never invent a Maven URL. Forge G2/G3 and Rustic Waters II G5 remain **NOT_VERIFIED**, irrespective of Windows unit tests.

## CONTROL P0 — Windows evidence-cache importer and restricted execution gate (2026-11-15)

**Actual Control Windows evidence (PR #28 comments):** installer/BINPATCH, SpecialSource SHA-1+Main-Class, Java 8u501 x64, 4 processors + MCP ZIP (5/5), unique classpath 33/33, Mojang 1.16.5 client SHA-1 `37fd3c903861eeff3bc24b71eed48f828b5269c8` (17,547,153 bytes), 65 official library records = **63 verified canonical remote artifacts, 2 generated/installer, 0 missing or invalid**. These facts are operator evidence, **not a Forge installation or execution result**.

**New read-only Rust importer:** `forge_evidence_cache::import_official_forge_cache(profile_json, derived, &ForgeCacheImport)` takes exact official manifests, one or more **user-selected absolute Maven cache roots** (each artifact must retain immutable Maven relative path under a root), the official pinned Mojang client, reviewed Java 8 location, BINPATCH installer/extraction attestation and a fresh private stage. It audits all library descriptors against canonical Maven URL, relative path, SHA-1 and **exact byte size**, rejects conflicting descriptors, duplicate files, path symlinks, tampered bytes and user-world staging. Missing files return `NeedsAction`; bad size/hash/source is `Failed`. It constructs `ForgeBindings` for all available verified processor JARs, classpath JARs, `@zip` references, `MINECRAFT_JAR`, `BINPATCH`, and staged MAPPINGS/MC_SLIM/MC_EXTRA/MC_SRG/PATCHED tokens. No cache importer performs downloads, Java or staging writes.

`preflight_official_forge` calls `prepare_forge_1165` and reports `PrepareState::{Verified,NeedsAction,Failed}`; the two **URL-empty** generated Forge libraries remain `NeedsAction` pending the actual processor outputs or authenticated installer evidence, so even verified 63/63 external artifacts never justify marking READY. `examples/forge_cache_preflight.rs` produces machine-readable JSON through `cargo run --locked -p tl-loader --example forge_cache_preflight -- ...`; the report destination must not preexist. CLI's `READ_ONLY_NO_JAVA_NOT_INSTALLED` status is invariant.

**Windows execution boundary:** The public Rust `run_approved_processors` path now **fails closed on Windows even if `restricted_environment_attested=true`**. A native `scripts/ForgeJobProcess.cs` helper implements Win32 suspended `CreateProcessW`, Job Object `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and active process limit; it assigns child to Job **before** resuming and closes Job on timeout/error to kill descendants. `new-forge-windows-sandbox.ps1` creates a `.wsb` with `Networking Disable`, clipboard/vGPU/other integration disabled, input **read-only** mount, fresh isolated writable output and host ACLs; it never launches the sandbox. `invoke-forge-sandbox-job.ps1` is a **Control-invoked guest-only** execution gate, requires exact executable SHA-1, stage under `C:\ForgeEvidence`, executable under `C:\ForgeInputs`, no default outbound network route, OS Windows Firewall outbound block, exclusive staging ACL, and Job Object. **No Forge Java execution has been approved or performed.** Sandbox guest firewall/ACL/effective isolation, secure movement of verified inputs into sandbox and actual processor output transport are **Control Windows real acceptance pending**; a generated `.wsb` and a passing benign Job Object test alone are not proof of completed containment. Do not set `ExecutionPermit.restricted_environment_attested=true` as a workaround. Production may need additional signed sandbox image/attestation and staged-cache integration before actual E2E.

**Windows CI** runs a *harmless* `cmd.exe /d /c exit 0` inside the native Win32 Job to verify suspended/assign/resume/wait and Job lifecycle, validates locked-down `.wsb` XML, tests refusal to overwrite existing stage and tests tampered Forge evidence; **no Java/Forge executable is ever run in CI**.

### Windows Control — 3 commands for cache preflight (PowerShell at repo root)

Before running, replace the five placeholders with the **actual absolute Windows paths** from Control's evidence cache; do not assume the current repo root is the Maven cache. Every Maven root must contain files below its official Maven relative hierarchy (e.g. `net/minecraftforge/...`). BinpatchDirectory is the earlier directory containing `client.lzma` and `binpatch-provenance.json`.

```powershell
git fetch origin; git switch feat/tl-03-forge-runtime; git pull --ff-only origin feat/tl-03-forge-runtime
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\crates\tl-loader\scripts\new-forge-cache-config.ps1 -MavenRoots 'C:\REPLACE\maven-classpath;C:\REPLACE\other-maven-cache' -ClientJar 'C:\REPLACE\client.jar' -MojangClientUrl 'https://launcher.mojang.com/v1/objects/37fd3c903861eeff3bc24b71eed48f828b5269c8/client.jar' -BinpatchDirectory 'C:\REPLACE\binpatch-evidence' -Java8 'C:\REPLACE\java8\bin\java.exe' -StageRoot 'C:\REPLACE\new-private-stage' -ConfigFile "$PWD\forge-cache-config-final.json"
cargo run --locked -p tl-loader --example forge_cache_preflight -- "$PWD\forge-36229-metadata-final\install_profile.json" "$PWD\forge-36229-metadata-final\version.json" "$PWD\forge-cache-config-final.json" "$PWD\forge-preflight-report-final.json"
```

Read JSON `forge-preflight-report-final.json`; `generated_or_installer=2` means expected **NeedsAction**, even if the machine has 63 verified sourceable libraries. A separate authorized Sandbox/Job Object execution acceptance is required before Control can approve actual Java processor E2E. Do not alter existing Minecraft worlds/profiles. Forge G2/G3/G5 are **NOT_VERIFIED**.
