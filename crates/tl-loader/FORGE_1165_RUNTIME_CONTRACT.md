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
