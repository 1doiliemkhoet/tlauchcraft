# Chat 4 — CurseForge Installer E2E (partial integration)

## Status: NOT_READY / G5_NOT_VERIFIED

This branch starts a real CurseForge Core API dependency resolver and downloader/staging foundation for **Rustic Waters II 1.16 / Minecraft 1.16.5 / Forge**. No Rustic Waters II archive or launch evidence is available to this chat. There is no Forge processor executor in the current tl-loader crate, so no GameInstance is ever marked READY by this work.

### Implemented code

- `crates/tl-pack/src/curseforge.rs`: resolves every manifest projectID/fileID using `https://api.curseforge.com/v1/mods/{project}/files/{file}` and the official download-url endpoint. The API key is sourced from the **local environment** `CURSEFORGE_API_KEY`; neither key nor requested metadata is persisted with credentials. Reports individual file availability, permission denied, malformed metadata, or manual action. Does not invent CDN paths or mirror restricted files.
- `crates/tl-pack/src/install.rs`: creates a fresh override staging directory, downloads only approved official HTTPS CDN origins, disables redirects, bounds file size, supports cancellation through `AtomicBool`, verifies expected file length and SHA-1 when supplied, computes a SHA-256 evidence digest, and refuses collisions with existing overrides. Returns `NEEDS_ACTION`, `FAILED` or `CANCELLED` — never READY.
- Desktop Tauri commands `pack_resolve`, `pack_stage_curseforge`, and `pack_cancel` run the resolver and verified staging off the UI thread. A read-only dependency breakdown, an explicitly **non-playable** staging action, coarse phase progress, cancellation and full blocked/failed outcomes are displayed after ZIP inspection. Full `modpacks` installation capability remains disabled.

### How to configure approved API access on Windows

Use a developer-approved CurseForge API key. Do not put keys in the repo, logs, desktop frontend arguments or screenshots.

```powershell
$env:CURSEFORGE_API_KEY = Read-Host 'CurseForge API key'
cargo test --locked --workspace --all-targets
```

A production secret storage integration is pending. Local environment variables are an interim developer configuration; don't leave them in shell history.

### Pending to obtain G5

1. Install Minecraft 1.16.5 from approved sources, verify game libraries and assets.
2. Implement an explicitly reviewed Forge 1.16.5 installer processor executor (not present in `tl-loader`). Verify every Forge library and output; use the Java version required by the actual Forge installer and game.
3. Persist per-instance Java path, `Xms` and `Xmx` and wire to the real `tl-game` runtime. The existing `tl-game` `LaunchConfig` supports `-Xms/-Xmx` but desktop doesn't expose launch yet.
4. Extend the current transient Tauri staging operation to durable install jobs, byte/file-level progress, prompt cancellation during API resolution, reconnect/restart, lockfile and atomic publish on full verification. Do not advertise staged files as installed/launchable.
5. Add authorized manual import UI for restricted files with checksum verification and complete dependency reconciliation; never skip a required blocked mod.
6. Run Windows Cargo lock generation after adding `reqwest` and `sha1` dependencies to `tl-pack`, then fmt/Clippy/tests. Validate actual Rustic Waters II archive and Minecraft main menu, world and logs.

### Reproduction / no fabricated success

```powershell
cargo generate-lockfile
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

Desktop is a separate Cargo workspace, so its `src-tauri/Cargo.lock` also requires genuine Windows Cargo update and Tauri validation when dependencies change.

**No real download, Minecraft launch, modpack-ready status or G5 PASS is claimed by this document.**
