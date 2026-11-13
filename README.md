# tlauchcraft — Minecraft Offline / LAN Launcher Research

> Research and specifications only. **No launcher implementation or E2E validation yet.**

## Goal

Build a functional Minecraft Java launcher focused on **local/offline player profiles**, **Singleplayer**, real **modpack installation and launch** (Fabric, Forge, NeoForge), and **LAN play between machines**. No Microsoft/Xbox/Premium login integration in scope.

## Acceptance criteria

- Start a real Minecraft Java client with a local profile.
- Create, save and reopen Singleplayer worlds.
- Install, resolve dependencies and launch real modpacks.
- Support host/join over trusted LAN, with verified mod compatibility.
- Preserve world and player identity through restarts, with logs and crash diagnostics.
- Treat each behavior as unverified until demonstrated by actual runs. A UI mock is insufficient.

## Study basis

Research from static inspection of TLauncher 2026 starter/installer and historical TLauncher 2.923 bytecode. Findings from the historical bytecode are not automatically proof of behavior in the 2026 binary. Any reverse-engineered references must be verified before implementation.

**Legal/security note:** Local/offline profiles do not confer a Minecraft license. This repository does not include third-party EXEs, JARs, Minecraft game assets, access tokens, credentials or any copyrighted binary payloads. Restrict offline servers to trusted LANs because names/identities may be spoofed.

See `ROADMAP.md`, `PROVENANCE.md`, and `docs/` for the study materials as they are imported.

## Modpack archive import (P0)

Import modpacks from **CurseForge exported ZIP** (`manifest.json`), **Modrinth `.mrpack`** (`modrinth.index.json`), and later **Prism/MultiMC instance ZIP**. Each import must create an isolated runnable instance, validate loader/game/dependencies and clearly report unavailable mods. **Specification only; not yet implemented.**

- [Modpack Import Functional Contract v1](docs/specs/MODPACK_ARCHIVE_IMPORT_v1.md)

## Local Skins (P0 + P1)

Skin Manager for local/offline profiles: PNG import/export, Classic/Slim models, 3D preview, and real in-game skin application on supported Minecraft versions (P0). Pixel editor, optional custom capes and visibility between trusted LAN peers are P1. Local preview, Singleplayer rendering and LAN peer rendering have distinct verification gates. **Not yet implemented.**

- [Offline Skin Manager & Editor Functional Contract](docs/specs/OFFLINE_SKIN_MANAGER_v1.md)

## Multi-chat Control Room

Seven feature chats are coordinated by Control. The **live shared status report** is [docs/control/LIVE_REPORTS.md on develop](https://github.com/1doiliemkhoet/tlauchcraft/blob/develop/docs/control/LIVE_REPORTS.md). [Control rules](docs/control/CONTROL_ROOM.md) and [task prompts](docs/control/tasks/) are maintained in Git. Code changes must go through PRs; only this live report document is updated directly on develop.
