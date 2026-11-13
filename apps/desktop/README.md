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
Prerequisites: Node/npm, Rust 1.88, Visual Studio C++/Windows SDK, WebView2 and Tauri prerequisites.

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

**Lockfile:** Windows GitHub Actions created and pushed `apps/desktop/package-lock.json` using Node 22/npm 10 from real npm registry (commit `a672f58`). The first Windows CI `43486294720` reported `npm ci` PASS, but `npm test` FAIL (8/10). Subsequent PR commits adjust failing tests; check latest workflow run on PR #14 for current evidence. Neither an npm install PASS nor TypeScript build PASS by itself establishes Minecraft or installer E2E.

**Windows CI:** `.github/workflows/desktop-windows.yml` runs on PR to develop for desktop paths (single `windows-latest` job). Commands: `npm ci`, `npm test`, `npm run build`, `cargo check` from `src-tauri`. Does not package NSIS on each commit. For reproducibility, run with Node 22 and Rust/MSVC toolchain; do not claim PASS before inspecting job logs.

## Outstanding typed commands / Control decisions

Chat 1 needs to approve and maintain the Tauri facade interface. Chat 2 currently provides metadata/download/launch primitives, not a ready-to-use profile-bound launch manager that resolves a fully installed instance and supervises it. Do NOT wire a premature `game_launch` command simply because `tl-game::launch` exists.

Unimplemented command proposals: `version_list`, `pack_import`, `game_launch`, `operation_cancel`, `skin_list`, `lan_status`. Actual implementations from Chats 2–6 and Control contract approval required. Control must decide API errors/event cancellation and signing/update/rollback policy.

No Java/game payloads or credentials bundled. No Microsoft auth or entitlement bypass. Offline multiplayer must remain trusted LAN-only. No G1–G8 acceptance claimed.
