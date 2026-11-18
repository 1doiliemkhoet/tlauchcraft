# TLaunchCraft

**Minecraft Java launcher for Windows, focused on local/offline profiles, real modpack installations and trusted LAN play.**

> Status (18 November 2026): **Minecraft Vanilla 1.21.1 and Rustic Waters II (Minecraft 1.16.5 / Forge 36.2.29) have been launched into real 3D worlds on Windows.** This is *not* evidence that every modpack or loader is playable. Read the [verification matrix](#verified-support) before installing another pack.

TLaunchCraft is a Rust workspace + a React/Tauri 2 desktop app. It manages isolated Minecraft instances and local player profiles, downloads/verifies official Minecraft dependencies, prepares approved loader runtimes, and starts the actual game. No Minecraft or Forge binaries, proprietary mod archives, account credentials or API keys are committed to the repository.

## Verified support

| Target | Code / installer | Real Windows evidence | Notes |
| --- | --- | --- | --- |
| Vanilla Minecraft 1.21.1 | Implemented | **IN_WORLD_VERIFIED** | Local profile, actual world, saved world reopened |
| Forge 1.16.5 / 36.2.29, Rustic Waters II | Implemented | **IN_WORLD_VERIFIED** (18 Nov 2026) | Real Forge+modpack world, HUD/minimap visible |
| Forge 1.16.5 / 36.2.34 | Reviewed and CI-tested | **NOT_VERIFIED** | Official installer and metadata pinned; no game/world proof |
| Forge 1.16.5 / 36.2.42 | Reviewed and CI-tested | **NOT_VERIFIED** | Official installer and metadata pinned; no game/world proof |
| Forge 1.12.2 / 1.18.2 / 1.20.1 | **NOT_PLAYABLE** | **NOT_VERIFIED** | Different installer/runtime families require separate adapters |
| Fabric | Metadata parser only | **NOT_VERIFIED** | No verified end-to-end install/Play bridge |
| NeoForge | Maven metadata inspection only | **NOT_VERIFIED** | Installer + exact Minecraft compatibility verification not implemented |
| Quilt | Not implemented | **NOT_VERIFIED** | Not interchangeable with Fabric |

**No universal modpack compatibility claim.** A passing unit test, verified installer hash, or a READY-looking UI does not equal a successfully running world.

## Features

- **Desktop app:** React, TypeScript, Vite, Tauri 2, Windows NSIS packaging, SQLite-backed local profile/instance settings and dedicated launcher UI.
- **Vanilla:** official Mojang version metadata, checksummed libraries/client/assets, Windows natives, Java selection, real process launch and saved worlds.
- **Forge 1.16.5:** manifest-selected Forge release (36.2.29, 36.2.34, 36.2.42); approved official installer SHA-1/metadata, bounded Windows Job Object processor execution with explicit consent, output receipts, independent rehash and isolated immutable instance publication.
- **Modpack import:** CurseForge exported ZIP; can import an already installed CurseForge profile folder *with its original manifest ZIP* (reconcile local mod JARs by file identity, no need to fetch every mod again). Provider-authorized downloads require a configured API key and available distributions. Metadata parsing/planning also recognizes Modrinth `.mrpack`; that is **not** verified Fabric installation.
- **Offline identity & LAN foundation:** deterministic offline game UUIDs, pack compatibility checks and preflight. **Two real modded clients joined over LAN remain NOT_VERIFIED.**
- **Safety:** untrusted archives/path traversal checks, verified downloads and manifests, strict loader/version gates, explicit consent for installer Java execution, no forced execution of arbitrary upstream processors, no modifications to imported CurseForge saves.

## Getting started on Windows

Prerequisites: Windows 10/11 x64, a legally obtained Minecraft installation/license and modpack content, Node.js 22/npm, Rust toolchains (workspace Rust 1.88 and desktop stable per workflow), MSVC C++ build tools/Windows SDK, WebView2, and a compatible Java runtime. For Minecraft 1.16.5 Forge use **Java 8**; other game versions have different Java requirements.

Build from source (PowerShell):

```powershell
git clone https://github.com/1doiliemkhoet/tlauchcraft.git
cd tlauchcraft
git switch integration/p0-playable-single-run
cd apps/desktop
npm ci
npm test
npm run tauri dev
```

To build a Windows installer from the desktop directory:

```powershell
npm run tauri build -- --bundles nsis
Get-ChildItem .\src-tauri\target\release\bundle\nsis\*.exe
```

You can also use a CI-built NSIS installer from the [P0 Windows workflow](https://github.com/1doiliemkhoet/tlauchcraft/actions/workflows/p0-final-playable.yml) (check commit, status and artifact expiry before use). CI build artifacts are **not** guarantees that all runtime versions are playable.

### Import and play Rustic Waters II / another supported Forge 1.16.5 pack

1. Obtain the modpack legitimately through CurseForge. Keep the **original exported pack ZIP containing `manifest.json`**. For already installed packs, select its real installed CurseForge profile folder and that original ZIP separately; do not turn the installed folder into an arbitrary pseudo-CurseForge ZIP.
2. Create or select a local/offline profile in TLaunchCraft. Select the installed-profile import workflow (or authorized provider download workflow), original manifest ZIP, folder, and matching **real Java 8 executable**.
3. Inspect the pack metadata and selected Forge version. Only the exact reviewed Minecraft 1.16.5 / Forge **36.2.29, 36.2.34 or 36.2.42** combinations pass the current execution gate. The launcher must reject mismatches, missing mods or unapproved installer metadata instead of silently falling back to 36.2.29.
4. Approve the Forge processor execution. Allow stage/download/verification to finish, and press **Play** on the published instance. Each pack has its own isolated directory and world saves.
5. If Play fails, keep sanitized launcher/game logs and `crash-reports` for diagnosis. Never delete the existing working instance as a workaround.

**Known working Windows evidence:** Rustic Waters II / 1.16.5 / Forge 36.2.29 entered a rendered world on **2026-11-18**. The second and third Forge variants need their own real in-game verification.

## Architecture

```text
Windows Tauri/React desktop
  ├── tl-core / tl-storage: profiles, instance metadata, SQLite
  ├── tl-game: official Minecraft download/install/launch
  ├── tl-pack: CurseForge / Modrinth metadata, staging, inventory receipts,
  │            official approved Forge installer and isolated publisher
  ├── tl-loader: loader metadata, Forge processor graphs, version policy, audit
  ├── tl-lan: offline UUIDs / compatibility / LAN preflight
  └── tl-skin: local skin library / provider boundary
```

All adapter development must maintain the **Inspect → Resolve → Download/Verify → Stage → Execute approved installer (when needed) → Reverify → Publish → Play** distinction. Metadata inspection and staging must never transition an instance to READY without independently verified runtime evidence.

## Development and CI

Root Rust workspace:

```powershell
cargo +1.88.0 fmt --all -- --check
cargo +1.88.0 clippy --locked --workspace --all-targets -- -D warnings
cargo +1.88.0 test --locked --workspace --all-targets
```

Desktop checks, from `apps/desktop`:

```powershell
npm ci
npm test
npm run build
npm run tauri build -- --bundles nsis
```

See [Forge release audit](docs/control/FORGE_FAMILY_P0.md), [P0 E2E acceptance](docs/P0_SINGLE_RUN_PLAYABLE_E2E.md), [Control Room](docs/control/CONTROL_ROOM.md) and [live reports](docs/control/LIVE_REPORTS.md). The historic P0 planning document and old subproject READMEs may describe earlier incomplete phases; prefer latest commit-specific tests and real Windows evidence.

## Acceptance gates

- **IN_WORLD_VERIFIED:** actual Minecraft instance loads the modded 3D world (not just main menu or CI).
- **SAVE_VERIFIED:** alter world → save/exit → relaunch same instance → confirm persisted block and inventory.
- **LAN_VERIFIED:** two distinct offline users with compatible versions/mods join a world on a trusted private LAN, disconnect/reconnect and confirm shared world state.
- **NOT_VERIFIED:** any missing player-facing proof, even when CI and installer metadata checks pass.

## Security, permissions and project intent

Offline/local profiles are not a replacement for ownership or Minecraft licensing. Mod distribution is subject to the original providers' rights and API policies. TLaunchCraft does not implement Microsoft/Premium account authentication or authentication bypass; keep offline-mode multiplayer **only on trusted LAN/private servers** because offline usernames can be impersonated. Never commit credentials, API tokens, proprietary game binaries, downloaded mods or world backups.

This is an actively developed project, **not a finished universal launcher**.
