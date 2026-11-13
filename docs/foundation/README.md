# Chat 1 Foundation v1
Rust workspace with tl-core DTOs and tl-storage SQLite persistence. This is not a Minecraft launcher or game E2E.
## Ownership
Chat 1 owns root Cargo manifests, tl-core, tl-storage and CI. Chat 2–6 depend on tl-core, Chat 7 consumes serialized DTOs through a future Tauri command adapter.
## Persistence
Use isolated %LOCALAPPDATA%/tlauchcraft or XDG_DATA_HOME/tlauchcraft. SQLite WAL, FK and schema migration 1. Instance files and worlds are NEVER deleted by storage. Profile UUID persists across rename. Caller must warn/backup before any LAN nickname change.
## Proposed contract clarifications for Control
- GameInstance.root_path always "instances/<UUID>" relative to app data, not arbitrary absolute.
- OfflineProfile.id is a local persistent identity, never the server-assigned offline UUID.
- CoreError categories are safe for display; never expose underlying SQLite errors.
- All UI and game runtime implementations remain outside Chat 1.
## Test commands
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
## Status
Local cargo unavailable at authoring; rely on CI for actual build/test evidence. Cargo.lock is committed and CI uses --locked for dependency consistency. Do not claim G1 until real CI succeeds.
