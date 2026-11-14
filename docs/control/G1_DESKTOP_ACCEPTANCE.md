# TLAUCHCRAFT — Chat 7 Desktop Windows G1 Acceptance

**Control review:** 2026-11-14
**PR:** [#14](https://github.com/1doiliemkhoet/tlauchcraft/pull/14)
**Accepted HEAD:** `cb568dc771272d695d25a8f7b39a216ca54afb2e`
**Squash merge:** [4a14293a2ac52ad3f075d97c6c322e14151fe382](https://github.com/1doiliemkhoet/tlauchcraft/commit/4a14293a2ac52ad3f075d97c6c322e14151fe382)
**Final Windows CI evidence:** [run 43492092812](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43492092812)

## Accepted scope — G1 Desktop UI source and build
- Tauri 2 + React + TypeScript Windows desktop shell, with real commands registered in Rust facade: `desktop_capabilities`, `profile_list`, `profile_create`, `instance_list`, and **read-only** `pack_inspect`.
- Rust facade uses `tl-core` typed profile and instance models, `tl-storage` local SQLite persistence, and `tl-pack` verified archive inspection.
- Capabilities negotiation fails closed; unimplemented game launch, modpack install, versions, LAN hosting, in-game skins, and downloads remain disabled. Desktop preview does not fabricate success.
- Genuine committed dependency locks: `apps/desktop/package-lock.json`, `apps/desktop/src-tauri/Cargo.lock`. The desktop Tauri crate is an independent Cargo workspace pinned for Rust stable >=1.90; root Rust workspace remains separately pinned at 1.88.
- Windows GitHub Actions [run 43492092812](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43492092812) on accepted HEAD: committed-lock guards PASS; `npm ci` PASS; **Vitest 9/9 PASS**; `npm run build` PASS; `cargo +stable check --locked` with Windows MSVC PASS.
- Control removed CI token write permissions, push/auto-commit behaviors, and on-the-fly lockfile generation before final acceptance. PR workflow is **read-only** and verifies exact committed dependencies.

## NOT verified
- **G2/G3/G4:** Real vanilla Minecraft Java client installation, launch to menu, and Singleplayer world save/reopen.
- **G5:** Fully installed and playable Modrinth/CurseForge/Prism modpack with required artifacts and hashes.
- **G6:** Two real Windows PCs joining/rejoining trusted LAN with persistent world/playerdata/inventory.
- **G7:** Custom skin actually visible locally in Minecraft and to a LAN peer.
- **G8:** NSIS installer generation, installation and launch on a clean Windows PC. Tauri `cargo check` and Vite build are not packaging or clean-machine smoke tests.
- Runtime SQLite profile creation, native file picker, and read-only archive inspection require an actual packaged GUI smoke before claiming user-facing functionality on a clean PC.

## Next Control priority
- Foundation hardening [PR #18](https://github.com/1doiliemkhoet/tlauchcraft/pull/18): sync latest `develop`, preserve all merged workspace crates and lockfile, rerun Windows checks; review SQLite CAS/backup and offline identity.
- Vanilla Windows E2E [PR #19](https://github.com/1doiliemkhoet/tlauchcraft/pull/19): sync latest `develop`; distinguish mock/fixture tests from actual Minecraft Java installation, menu and world evidence. Functional G2–G4 acceptance requires real Windows gameplay evidence.
- Chat 7 follow-up must implement a controlled Windows NSIS build and clean-machine smoke, using a new branch/PR; no fake Play/Import/LAN/Skin buttons until real server code, accepted typed commands and E2E exist.

**Verdict: 7/7 primary modules are merged with source/build G1 acceptance only. This is not a playable Minecraft launcher release.**
