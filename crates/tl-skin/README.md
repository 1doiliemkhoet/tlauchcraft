# tl-skin — isolated offline appearance core (Chat 6)

This crate is registered in the root Rust workspace and consumes the canonical `tl-core::SkinSelection`/`SkinModel` and stable `OfflineProfile` UUID. Persistence currently uses an isolated local skin catalog; callers must explicitly save updated `OfflineProfile` via `tl-storage::ProfileStore` and reconcile `skin_id` before launch.

## Implemented surface

- Strict PNG import: supports 64×64 and legacy 64×32, normalizes decoded pixels into RGBA8,
  restricts compressed input and decoded memory, rejects invalid PNG/dimensions.
- Re-encode/export, SHA-256 integrity verification, isolated app-managed image storage;
  list/import/rename/duplicate/delete with explicit confirmation.
- JSON catalog for profile selection, Classic/Slim choice, optional cape ID and per-instance override.
- Framework-neutral pixel paint/sample/erase/undo/redo.
- Provider capability metadata and explicit, **separate** launcher-preview, local-in-game
  and peer-in-game visibility states. Nothing here injects skins into Minecraft.
- Consent-gated bounded PNG payload helper for a future trusted LAN transport; it does
  not open a socket or transmit automatically.

## NOT implemented / do not claim as passed

- A rendered rotatable/zoomable 3D preview, UV mesh, full pixel tools,
  legacy anatomical UV conversion, cosmetic archive packs and cape storage.
- A version-tested in-game skin provider integration. No Minecraft JAR patching.
- LAN transport / compatibility handshake / cache invalidation / friend-visibility E2E.
- Singleplayer screenshots or two-machine screenshot verification.

A provider capability object only states a declared capability, not a tested result.
Evidence fields must reference real logs/screenshots before Control marks the relevant gate.
Offline player identity is not authenticated and can be spoofed on untrusted LANs.

## Local tests

From repo root (after a Rust toolchain is installed):

```powershell
cargo test --locked -p tl-skin
cargo fmt --all -- --check
```

The root workspace and Cargo.lock are updated on the Chat 6 feature branch. Integration owners: Chat 1 (typed storage contract),
Chat 5 (LAN transport) and Chat 7 (editor/preview renderer).
