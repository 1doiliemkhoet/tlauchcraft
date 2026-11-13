# Modpack Archive Import — Functional Contract v1

**Status:** DESIGN ONLY — no implementation or E2E evidence.
**Priority:** P0 (core feature for Offline Singleplayer + LAN).
**Goal:** Import real modpack archives downloaded from CurseForge, Modrinth, and Prism/MultiMC. A pack must create an **independent runnable instance**, not just extract `.jar` files to one shared `.minecraft` folder. Local/offline profiles remain the only account flow in scope.

## Accepted formats

| Archive | Detection | Import behavior | Scope |
|---|---|---|---|
| CurseForge exported `.zip` | root `manifest.json` + `overrides/` | Read Minecraft version and loader, resolve `projectID/fileID` files via authorized CurseForge integration, then apply overrides | P0 |
| Modrinth `.mrpack` | root `modrinth.index.json` | Read `dependencies`, fetch `files[]` from permitted HTTPS URLs, verify SHA-1 / SHA-512, apply `overrides/` and `client-overrides/` | P0 |
| Prism / MultiMC exported `.zip` | `mmc-pack.json`, `instance.cfg`, usually `minecraft/` | Parse components, install supported loader, copy instance content with validation. Unsupported legacy patches must produce an explicit error | P1 |
| Generic custom `.zip` of `mods/`, `config/`, etc. | no known manifest | **Do not guess Minecraft/loader version**. Require user-selected game version/loader; import only supported data; clearly flag unknown or missing dependencies | P1 |

**Note:** `*.zip` is a container, not a guarantee that it is a complete modpack. File extension alone is not format detection. CurseForge archive downloads often include **references** to mods, not the binaries themselves.

## End-user flow

1. `Instances → Import Modpack` or drag-and-drop a local `.zip/.mrpack` (local path is P0; HTTPS URL import can be P1).
2. Inspect archive **without executing contained files**; detect format by manifest and validate metadata.
3. Display pack name, source, Minecraft version, loader/version, mod count, override contents, disk estimate and any warnings. Let user choose instance name and destination.
4. Create staging directory in isolated `instances/<instance-id>.staging/`, not in existing Minecraft installs or research workspaces.
5. Resolve loader + Java requirements. For CurseForge, resolve project/file identifiers via approved API; if distribution permission/authorization blocks download, mark the mod as missing and offer authorized manual import (no scraping or policy bypass).
6. Download allowed files with retry/backoff/cancellation and resume as feasible; verify hashes/sizes when provided; collect final hashes for lockfile.
7. Apply overrides in deterministic order. Do **not** overwrite user saves without explicit approval.
8. Validate dependencies, loader/MC compatibility and launch prerequisites.
9. Atomically publish successful instance; mark `READY` only when required files are present. On any required dependency failure, return `NEEDS_ACTION` or `FAILED`, never pretend import succeeded.
10. Allow `Play Singleplayer`, `Open instance folder`, `View logs`, and later `Create LAN server`. LAN support must validate server-side mods/config independently.

## Data model

- `PackSource`: `curseforge_zip | modrinth_mrpack | prism_zip | generic_zip`.
- `PackMetadata`: name, source, gameVersion, loaderType, loaderVersion, upstream pack ID/version if present.
- `Instance`: stable UUID, displayName, rootDir, offlineProfileId, status, updatePolicy.
- `ResolvedFile`: relative path, source/provider, project/file IDs or URL, expected and computed hash, side (`client/server/both`), required/optional, download state, rights/availability state.
- `InstanceLock`: pack metadata, resolved versions, file hashes, pack archive SHA-256, install timestamp, format version; **no credentials or account tokens**.
- `ImportReport`: downloaded/installed/skipped/blocked/failed items, actionable error messages; report suitable for sharing with friends without secrets.
- `ImportState`: `INSPECTING → PLANNED → DOWNLOADING → VERIFYING → APPLYING → READY`, alternatively `NEEDS_ACTION | FAILED | CANCELLED`.

## Required edge cases and security

- Reject absolute paths, `../`, Windows drive/UNC paths, symlink escapes, forbidden special files, duplicate paths (including Windows case-insensitive collisions), ZIP bombs, excessive file counts, enormous decompressed size, unsupported encryption, and malformed manifests.
- HTTPS only for remote downloads by default; validate redirects and prevent local-network / loopback access from untrusted archive URLs (SSRF protection).
- Never execute scripts/executables embedded in a pack. Treat pre/post-install hooks as unsupported unless explicitly reviewed.
- Respect mod licenses/CurseForge distribution settings and CDN access requirements. No hardcoded API keys in repository; credentials, if used, must be in local secure configuration.
- Explicitly distinguish `not redistributable`, `permission denied`, `not found`, `version mismatch`, `hash mismatch` and `network failure`.
- Isolate modpack instance from `saves/` in other instances. Back up an instance before update/repair.
- Preserve installed files and downloaded cache safely on cancellation; never delete unrelated user paths.

## Offline + LAN integration

- Importing a pack does **not** require a Microsoft/Premium account; it does not itself establish Minecraft license rights.
- One imported pack maps to an independent Minecraft client instance with a stable local profile.
- `Host/Join LAN` must compare relevant loader, game version, required gameplay mods and configs. Client-only shaders and resource packs need not match.
- Dedicated LAN server generation is a **separate phase**: filter server-appropriate mods, apply server overrides, generate server config and test with two actual devices.
- Stable profile UUID/identity, saved worlds and inventories must survive restart and reconnect; no false E2E PASS without game/log evidence.

## P0 acceptance tests (real files)

| ID | Scenario | Pass evidence |
|---|---|---|
| IMP-001 | Import valid CurseForge ZIP with Forge/Fabric | Valid manifest, supported files installed, instance marked READY, game reaches main menu |
| IMP-002 | Import valid `.mrpack` | All required files/hash verified, overrides applied, game reaches main menu |
| IMP-003 | Unavailable CurseForge mod | Missing item shown with project/file IDs and authorized manual resolution; NOT READY until fulfilled |
| IMP-004 | Unsupported loader or Minecraft version | Clear error before destructive changes |
| IMP-005 | Invalid ZIP / zip slip / zip bomb | Rejected without filesystem escape |
| IMP-006 | Two imported packs of different versions | Separate mods/config/worlds; both independently runnable |
| IMP-007 | Interrupted download / wrong checksum | Retry or FAILED; no broken instance claimed READY |
| IMP-008 | Offline Singleplayer world | Create, save, close and reopen actual Minecraft world |
| IMP-009 | Modpack LAN two devices | Compatible pack on both machines, join trusted LAN server, verify modded interaction & inventory persistence |
| IMP-010 | CurseForge API restriction | Honest NEEDS_ACTION state and no unauthorized mirror bypass |

## Sources for format decisions (checked 2026-11-13)

- CurseForge modpack export: https://support.curseforge.com/support/solutions/articles/9000197908-exporting-a-modpack-for-curseforge-project-submission
- CurseForge author distribution toggle: https://support.curseforge.com/support/solutions/articles/9000207877
- CurseForge CDN API key announcement (2026-06-10): https://blog.curseforge.com/introducing-api-key-authentication-for-curseforge-file-downloads/
- Modrinth format spec: https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack
- Prism import: https://prismlauncher.org/wiki/help-pages/Zip-import/
- MultiMC instance export: https://github.com/MultiMC/Launcher/wiki/Export-Instance

**Do not treat this contract as proof the launcher has been implemented.**
