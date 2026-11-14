# CONTROL — Foundation hardening / SQLite CAS and Offline UUID contract

**Reviewed:** 2026-11-14  
**PR:** [#18](https://github.com/1doiliemkhoet/tlauchcraft/pull/18)  
**Head:** `f34ca8baa395d4cf0d4317a5b40c92a23b268960`  
**Merged:** [`d1f99eb5`](https://github.com/1doiliemkhoet/tlauchcraft/commit/d1f99eb53e0587bf0680f635450454f02d2b8e07)  
**Windows CI:** [43583379799](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43583379799) — 81 tests PASS, workspace guard, fmt and strict Clippy PASS.

## Accepted source/build scope
- Root Cargo workspace retains all **seven** crates and genuine dependency lockfile; Tauri desktop remains separate.
- SQLite schema v1 → v2 migration introduces `revision` for profiles and instances, respecting legacy rows and data.
- Atomic optimistic compare-and-swap and stale-writer conflicts for profile/instance writes, with regression tests including multiple SQLite connections.
- Consistent SQLite backup to a temporary file with no-clobber publication and symlink/existing-target protections.
- Separate internal `OfflineProfile.id` versus derived Java vanilla offline-mode server UUID with deterministic MD5/nameUUID v3 semantics; username changes warn about possible playerdata/inventory changes.

## Not accepted
- Real two-PC Minecraft LAN test of offline UUID/playerdata and user rename: G6 NOT_VERIFIED.
- No claim of security against username spoofing, no online authentication, and no user-world migration performed.
- Windows tests verify code, not actual client launch or interactive UI.

## Follow-ups
- Chat 2 PR #19 must synchronize against merged Foundation contracts before final review; Minecraft G2–G4 still require real running client/world proof.
- Chat 5 live LAN two-PC scenarios needed to validate identity and inventory outcomes.
