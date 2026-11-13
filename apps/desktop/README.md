# Chat 7 — Windows Tauri desktop

Status: **source integrated with 6 Rust modules; Windows tests and Tauri check tracked by PR #14 Windows CI**. This is not a Minecraft E2E report.

## Integration (2026-11-14)
- Branch incorporates develop Foundation `tl-core`, `tl-storage`, and Game Core `tl-game`.
- Tauri host uses actual `tl-core` `OfflineProfile`, `GameInstance` and `CONTRACT_VERSION` types; local SQLite persistence uses `tl-storage` in the platform-specific app data directory, preserving IDs across launches.
- Registered commands: `desktop_capabilities`, `profile_list`, `profile_create { username }`, `instance_list`, `pack_inspect { archivePath }` (read-only metadata only).
- `profiles`, `instances` and `pack_inspection` capabilities are enabled only for those registered commands. `versions`, `game`, `modpacks` (full installation), `lan`, `skins`, `settings`, `downloads` remain **disabled** pending real facade integration. Instance listing does NOT imply instance creation/install/launch.
- Serde wire enum values are snake_case (`ready`, `vanilla`, `neo_forge`), reflected in TS contract.
- Event listener handles v1 typed event envelopes; no fake timed progress, no fabricated backend success.
- Archive picker is native; no ZIP extraction in UI. Drag-drop prompts native selection rather than trusting browser paths.

## Local Windows validation (PowerShell)
Prerequisites: Node 22/npm 10, **Rust stable 1.90+** for the independent desktop crate, Visual Studio C++/Windows SDK, WebView2 and Tauri prerequisites. The root Rust workspace retains its separate pinned toolchain.

```powershell
cd apps/desktop
npm ci
npm test
```
```powershell
npm run build
npm run tauri dev
```
```powershell
npm run tauri build
Get-ChildItem .\src-tauri\target\release\bundle\nsis\
```

**Lockfiles:** Windows GitHub Actions generated and committed `apps/desktop/package-lock.json` using npm 10 (commit `a672f58`) and `src-tauri/Cargo.lock` using Rust Cargo stable after a successful Windows check (commit `b400420`). Neither was hand-generated.

**Windows CI:** `.github/workflows/desktop-windows.yml` runs on PR to develop for desktop paths (single `windows-latest` job). Commands: `npm ci`, `npm test`, `npm run build`, `cargo +stable check` from `src-tauri`. [Windows run 43488830996](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43488830996) passed all these checks (9/9 Vitest) and generated the Cargo lockfile. Pinning `tauri 2.11.6` with compatible runtime/wry avoids conflicts with the Foundation-pinned `serde 1.0.219`. Does not package NSIS on each commit. For reproducibility, run with Node 22 and Rust/MSVC toolchain; do not claim PASS before inspecting job logs.

## Outstanding typed commands / Control decisions

Chat 1 needs to approve and maintain the Tauri facade interface. Chat 2 currently provides metadata/download/launch primitives, not a ready-to-use profile-bound launch manager that resolves a fully installed instance and supervises it. Do NOT wire a premature `game_launch` command simply because `tl-game::launch` exists.

Unimplemented command proposals: `version_list`, `pack_import`, `game_launch`, `operation_cancel`, `skin_list`, `lan_status`. Actual implementations from Chats 2–6 and Control contract approval required. Control must decide API errors/event cancellation and signing/update/rollback policy.

No Java/game payloads or credentials bundled. No Microsoft auth or entitlement bypass. Offline multiplayer must remain trusted LAN-only. No G1–G8 acceptance claimed.
