# tl-pack (Chat 4)

Independent archive inspection and restricted override staging crate. This is an **early implementation slice**, NOT an integrated modpack installer.

## What works in code
- Structural classification for CurseForge export, Modrinth .mrpack, Prism/MultiMC instance ZIP.
- Metadata, game/loader and file references parsed without executing pack content.
- Reject path traversal, absolute/Windows paths, duplicate names (case-insensitive), symlinks/special types, oversized files and suspicious compression ratios.
- Stage only override files into a new isolated directory, never into existing saves. Reject changed archive after inspect.
- Declare all unresolved required external downloads as NEEDS_ACTION, never READY.

## Critical missing integrations
- Chat 1 storage and typed GameInstance/InstanceStore.
- Chat 2 FileInstaller + streaming HTTPS downloader with DNS/redirect/private-IP SSRF validation, SHA1/SHA512 checks, retries, cancellation, cache.
- Chat 3 LoaderInstaller + Java compatibility and launch verification.
- CurseForge API credentials and authorized download policy integration; never guess CDN URLs from numeric IDs.
- Stage is NOT an atomic published instance; final lockfile, rollback, cancellation and recovery remain pending.
- Modrinth `client-overrides/` support and optional file decision UI pending.
- Full mod dependency graph, Minecraft version compatibility checks, actual pack import and game start pending.

## Isolated verification

```powershell
cd crates/tl-pack
cargo test
cargo fmt --check
```

No game/launcher E2E result is claimed. See PR description and Control G0–G5 acceptance gates.
