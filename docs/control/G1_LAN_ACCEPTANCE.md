# TLAUCHCRAFT — Chat 5 Trusted LAN Windows G1 Acceptance

**Reviewed:** 2026-11-14
**PR:** [#9](https://github.com/1doiliemkhoet/tlauchcraft/pull/9)
**Accepted head:** `551c295cad8853240fe0a5a50dc4eed7cb4de20a`
**Merge commit:** [8c5f6fe0f8fa61b923250caafa95f0efe95cc891](https://github.com/1doiliemkhoet/tlauchcraft/commit/8c5f6fe0f8fa61b923250caafa95f0efe95cc891)
**Evidence:** [Windows CI run 43478418741](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43478418741)

## Accepted scope
- `tl-lan` crate is included in the root Windows Rust workspace alongside `tl-core`, `tl-storage`, `tl-game` and `tl-loader`.
- Trusted-private-LAN configuration and validation, offline-mode warnings, server process and filesystem primitives, port probe, modpack fingerprint compatibility checks.
- Windows workflow: workspace registration, rustfmt, clippy `-D warnings`, cargo workspace tests all SUCCESS on head `551c295`.
- Test logs: 44 passed total (4 core, 12 game, 20 LAN, 8 storage), 0 failed.

## Explicitly not accepted
- **G6 NOT_VERIFIED**. No two real Windows PCs running modded Minecraft with same pack and loader, joining/rejoining and checking world, inventory/playerdata, or crash/restart behavior.
- Unit tests do not prove Windows Firewall settings, user consent, real Java server launch or LAN traffic isolation.
- Offline-mode allows username spoofing; do not expose the server to public networks.
- Issue [#16](https://github.com/1doiliemkhoet/tlauchcraft/issues/16): internal profile UUID differs from server offline-mode player UUID. Renaming can alter on-server playerdata/inventory until tested/migrated.
- Cosmetic peer visibility G7 NOT_VERIFIED.

## Next steps
Chat 6 skins must sync post-merge develop before integration (shared Cargo workspace/lockfile), rerun Windows CI. Chat 2 and Chat 5 cooperate on real G6 two-PC testing once G2/G3 real game workflow exists. Chat 4 modpack must also preserve the new `tl-lan` workspace member when fixing the lockfile.

Acceptance limited to library code + automated Windows tests; no user-facing launcher functional readiness claim.
