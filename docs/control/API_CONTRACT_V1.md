# TLAUCHCRAFT — Shared API Contract v1 (baseline design)

**Status:** provisional contract requiring implementation by Chat 1 and approval by Control. Do not mark implementations as existing.

## Storage and models
- `OfflineProfile { id: UUID, username: String, skin_id?: UUID, created_at, updated_at }`. UUID persists independently of mutable name and per-instance state. Validate allowed Minecraft username rules relevant to tested versions.
- `GameInstance { id: UUID, display_name, root_path, minecraft_version: String, loader: LoaderSpec, profile_id: UUID, state: InstanceState }`.
- `LoaderSpec { kind: Vanilla | Fabric | Forge | NeoForge, version?: String }`.
- `ModpackSource { type: CurseForgeZip | ModrinthMrpack | PrismZip | GenericZip, archive_sha256, source_metadata }`.
- `InstanceState { Empty, Inspecting, Planned, Downloading, Verifying, Ready, NeedsAction, Failed, Cancelled }`.
- `VerifiedFile { path: relative safe path, expected_sha1?, expected_sha512?, actual_sha256, origin, side: Client | Server | Both, required }`.
- `InstallReport { instance_id, installed[], skipped[], blocked[], failed[], bytes_downloaded, evidence_path }`.
- `LaunchRequest { instance_id, profile_id, java_path?, min_ram_mb?, max_ram_mb?, extra_jvm_args? }`; launch service returns `LaunchProcess { pid, started_at, log_path, termination_state }`.
- `LanServerSpec { instance_id, trusted_bind, port, whitelist, online_mode: false, server_root }`. May require explicit user confirmation; do not expose public interface by default.
- `SkinSelection { profile_id, skin_id, model: Classic | Slim, cape_id? }` plus provider support matrix.

## Trait / service boundaries (names and field types may be refined before freeze)
- `ProfileStore`: create/list/get/update/delete with safe migration and preserving identity.
- `InstanceStore`: stage/create/get/list/snapshot/delete with protection against deleting user data.
- `VersionCatalog`: list/resolve manifest and inherited version metadata, OS/arch rules.
- `FileInstaller`: plan/download/verify/cache/commit/cancel/repair; no silent download from unknown mirrors.
- `GameRuntime`: resolve Java, construct version-correct arguments and start/monitor process.
- `LoaderInstaller`: list compatibility, prepare metadata and install loader for selected version.
- `PackImporter`: inspect archive, plan install, resolve permitted sources, apply overrides, verify and commit instance.
- `LanService`: compatibility report, create/start/stop dedicated server, status and rejoin state.
- `SkinService`: import/export, assign, render previews, prepare provider for instance, report local/LAN visibility.
- `DesktopFacade`: explicit typed commands + progress event subscription; UI code does not implement game download or bypass backend validation.

## Event contract
Each event includes `operation_id`, `instance_id?`, `timestamp`, `phase`, `message_key`, `details` (redacted), and optional `percent`.
Event names: `profile.changed`, `instance.changed`, `download.progress`, `download.verified`, `pack.inspect`, `pack.needs_action`, `pack.ready`, `loader.installed`, `java.resolved`, `game.started`, `game.exited`, `lan.server_started`, `lan.peer_joined`, `skin.selected`, `skin.local_verified`, `skin.peer_verified`.
Errors are typed: `InvalidInput`, `UnsupportedVersion`, `NetworkFailure`, `IntegrityMismatch`, `UnauthorizedSource`, `NeedsManualDownload`, `CompatibilityConflict`, `DiskFull`, `Cancelled`, `LaunchFailed`, `UnsafeArchive`, `NotVerified`.
An event is a status transition, **not** proof of a successful real Minecraft launch until test evidence is attached.

## Ownership / change control
Chat 1 defines concrete Rust crate/types and tests; Chat 2–6 may ask for contract amendments via Control, never silently fork data shapes. Chat 7 binds only to shared typed APIs. Cross-cutting changes need a reviewed control decision (document key, compatibility, migration path).
