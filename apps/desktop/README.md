# Chat 7 — tlauchcraft Windows desktop scaffold

**Status: shell only. No Minecraft backend is implemented or connected in this branch. No game E2E or Windows installer smoke test passed.**

## Ownership and architecture

- `src/App.tsx`: navigation, offline profiles, version/instance lists, pack archive picker, skin status, trusted LAN, progress/errors/logs.
- `src/backend.ts`: typed TypeScript boundary; denies operations unless negotiated features are enabled. No mock success callbacks.
- `src-tauri/src/main.rs`: Tauri 2 host returns explicit all-disabled capability matrix. No cross-crate import before Chat 1 contract lands.
- ZIP picker is OS-native. Browser drag-drop does not disclose a trustworthy native path; the app opens the import screen and asks to browse.
- UI labels separate skin preview / Singleplayer / LAN peer verification.
- No game assets, Java, offline accounts or credentials bundled.

## Windows developer workflow (PowerShell)

```powershell
cd apps/desktop
npm install
npm run test
```
```powershell
npm run build
npm run tauri dev
```
```powershell
npm run tauri build
Get-ChildItem .\src-tauri\target\release\bundle\nsis\
```

Prerequisites: Node.js/npm, stable Rust + MSVC Build Tools with Windows SDK, WebView2 and Tauri prerequisites. The final command creates an **unsigned** NSIS installer only if build succeeds on Windows; a clean Windows machine smoke test remains mandatory. No auto-update enabled until signing/update/rollback policy is agreed.

## Backend integration proposal to Control (NOT YET IMPLEMENTED)

The v1 contract specifies traits and model fields, **not concrete Tauri command signatures**, so Chat 7 proposes this command surface (Control/Chat 1 must approve before enabling capabilities):

| Feature | Proposed Tauri commands | Dependencies |
|---|---|---|
| profiles | `profile_list`, `profile_create { username }` | Chat 1 |
| instances, versions | `instance_list`, `version_list` | Chats 1,2 |
| modpacks | `pack_import { archivePath }` | Chat 4 |
| game | `game_launch { request }` | Chat 2 |
| downloads | `operation_cancel { operationId }` | Chats 1,2,4 |
| skins | `skin_list` and additional edit/preview operations TBD | Chat 6 |
| lan | `lan_status`; host/join TBD | Chat 5 |
| settings | storage/Java/RAM settings TBD | Chats 1,2 |

Events are listened to under the exact dot-separated names from `docs/control/API_CONTRACT_V1.md`; payload validation is minimal until Chat 1 provides frozen shared schemas. Backend facade must set feature flag true only when the command is actually registered and verified. Command failures are shown as errors, never treated as successful jobs.

**Important: Tauri command invocations are future adapter targets, not existing handlers.** No frontend import/downloading/launch implementation. Extra commands, Rust type exports, event payload schema, cancellation semantics and logs transport require Control approval.

## Verification

Run `npm run test`, `npm run build`, `npm run tauri build` on Windows. Tests are hermetic validation of capability gating, ZIP extension filtering and percent bounds, **not E2E**. When real backend lands, add typed integration tests for command errors, progress/cancel, keyboard accessibility, actual game launch process, modpack install and Windows clean-machine installer. Record logs, artifact hash, toolchain and machine details in PR evidence.
