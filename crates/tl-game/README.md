# tl-game — Minecraft Java game core (Chat 2)

**State:** partial implementation; no Minecraft menu/world E2E has been executed. This crate intentionally lives outside the root workspace until Chat 1 establishes the shared Rust interfaces.

## Existing code

- `lib.rs`: inherited JSON version resolution, ordered OS/feature rule evaluation, arguments from metadata (no hardcoded main class), Java major probe, safe argv process launch, SHA1-verified HTTPS downloads to a temporary file and rename.
- `metadata.rs`: client jar, asset index, asset objects, logging configuration, library/native classifier install planning with relative-path validation.
- Hermetic unit tests: inheritance cycle, rule order, arguments, Java version parsing, asset layout, traversal rejection, SHA mismatch and path-with-spaces argv.
- No Microsoft login, token handling, game binaries, auto-accepting untrusted mirrors, or writes to user saves.

## Local test commands (PowerShell)

From repository root, after Rust has been installed:

```powershell
cd crates/tl-game
cargo test
cargo fmt -- --check
```

Standalone crate can run `cargo test --manifest-path crates/tl-game/Cargo.toml` from the repository root. Once Chat 1 publishes `Cargo.toml` workspace membership, use `cargo test -p tl-game` instead.

## Pending before claiming production readiness

1. Integrate Chat 1 concrete `OfflineProfile`, `GameInstance`, typed errors and operation events. Do not duplicate their public model.
2. Add Mojang manifest fetching, native archive extraction and sanitization, legacy argument lexer, complete launch placeholders, library classpath assembly and Java runtime provisioning. Current `LaunchConfig` expects an already prepared instance.
3. Downloader currently retries whole files and fails safely on a corrupt existing target; no ranged resume, cache index, cancellation of a concurrent running job, per-host semaphore, complete symlink ancestor walk, full Windows directory-atomic guarantees, or repair transaction yet.
4. OS version rules currently fail closed except exact text matches; needs tested regular-expression version handling.
5. Native compatibility and loader-specific metadata vary by version. Do not claim Fabric/Forge/NeoForge support here.
6. G1: compile, clippy and tests on Windows CI. **Not yet performed in this conversation** (Rust toolchain unavailable in execution environment).
7. G2–G4: permitted real Minecraft install; Java executable/version, checksums, PID/stdout/stderr, screenshot/log of main menu, new world/save/reopen, backup integrity. **Not verified**.
8. Provenance / entitlement: local profiles are not a Minecraft license. Never redistribute Mojang binaries or claim an offline profile grants access.

## Proposed boundary with Chat 1

Keep `tl-game` independent until Chat 1 contract is ready. Adapter layer should accept `GameInstance` path/version and `OfflineProfile` UUID/name, and emit typed `download.progress`, `download.verified`, `java.resolved`, `game.started`, `game.exited` events. Control must review API changes before integration.

## Test evidence policy

A green parser test cannot satisfy G3 or G4. Attach observed artifacts with hashes and reproducible commands for each gate; do not make simulated logs.
