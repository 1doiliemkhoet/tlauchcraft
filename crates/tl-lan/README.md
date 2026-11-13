# Chat 5 — trusted LAN integration notes

## Delivered in this branch (isolated crate)

`crates/tl-lan` is a standalone Rust library pending Chat 1's shared type freeze and workspace inclusion. It provides:

- Exact Minecraft/loader version compatibility checks and SHA-256 *fingerprint comparison*. The hashes supplied by callers MUST be computed from verified files by Chat 4; the current module does **not** hash the filesystem or attest game-loader handshakes.
- Side-aware gameplay mod comparison. Client-only cosmetics/shaders and server-only mods can differ.
- LAN-only RFC1918 IPv4 bind guard, deliberate operator acknowledgement of offline identity spoofing and Windows Private firewall checks, and manual Direct Connect address generation.
- Create-new `server.properties` with explicit `online-mode=false`, RCON disabled, whitelist preference and local bind.
- Launch an **already installed** local JAR through a separately supplied Java runtime, capture console logs, record PID, request graceful `stop` and wait for actual exit. This generic JAR path is **not** a working Forge/NeoForge/Fabric server installer; some loaders require scripts/classpath args and a loader-specific adapter.
- Refuse username changes without an explicit backup/migration step; cold world backup after caller-verified stop, without modifying game worlds.

## Security and lifecycle

Offline servers do not authenticate players. Any peer who can reach the port may claim another nickname; `white-list=true` does not prevent nickname spoofing. Require an isolated trusted LAN and firewall scoped to a Private network and approved subnet. Never configure router NAT, UPnP, port forwarding, public binds, unsolicited Windows Firewall rules, or RCON automatically. Reject `0.0.0.0` even when it appears convenient for LAN.

The operator must **personally accept the Minecraft server EULA** (do not auto-write `eula=true`). The host is responsible for obtaining game/server distributions from legitimate sources and checking Java/loader version, pack lockfile and archive integrity.

Client LocalProfile UUID is NOT the same as the offline-mode server-assigned UUID. Standard offline-mode identity commonly depends on the **exact username**; changing it may change `world/playerdata` and inventory. Pin username per server, do not overwrite or migrate playerdata silently.

## API integration to request from Control

1. Chat 1: adapt `LanServerSpec`, `PackFingerprint`, `CompatibilityReport` and `LanError` into shared versioned types; add persisted approved LAN network policy and stable profile-to-server username mapping.
2. Chat 2: provide runtime process supervisor that can accept loader-specific dedicated-server launch recipes and report verified ready/exit/save logs; do not assume `java -jar` works for all mod loaders.
3. Chat 3: emit verified Fabric/Forge/NeoForge **server** installation/launch plans for a chosen Minecraft/Java version.
4. Chat 4: supply SHA-256-attested lockfile, mod side declarations and verified gameplay/config parity. Wrongly marked side or gameplay flags must not silently bypass verification.
5. Chat 7: obtain explicit user warnings/confirmation, show compatibility blockers, firewall checklist, server console, Direct Connect IP:port, and world backup controls.

## Windows reproducible unit commands

Run from a local clone on this branch:

```powershell
git switch feat/tl-05-lan
cargo test --manifest-path crates/tl-lan/Cargo.toml
cargo clippy --manifest-path crates/tl-lan/Cargo.toml --all-targets -- -D warnings
```

The Rust test fixtures cover identical/mismatched hashes, cosmetic and server-only mods, loader drift, path traversal, public bind rejection, explicit permission and username rename protection. Tests are not a substitute for a real Java server.

## Pending real E2E gate G6

Requires two Windows PCs, valid Minecraft installations, the same **verified** modpack/gameplay files, exact matching Minecraft/loader versions, a supported dedicated-server distribution and user acceptance of its EULA.

1. Private IPv4 LAN, verify `Get-NetConnectionProfile` and explicit scoped firewall rule; confirm no public-facing route or router forwarding. Record the server bind and port.
2. Obtain/install legitimate server artifacts via Chat 2/3/4, start server, inspect `console.log` for loader initialization and player joins, and check port conflict behavior.
3. Both real clients Direct Connect; test gameplay mod items, shared blocks, chests and inventory.
4. Disconnect each client, stop server gracefully, hash/sample backed-up `level.dat`, `region` and `playerdata` files, restart server and reconnect to validate same inventory, world, positions and player IDs. Compare with an intentionally incorrect pack as negative test.
5. Preserve server/client logs, pack lockfile hashes, commands, timestamps, both machine identities anonymized, and photos or screen captures.

**Status:** G0 provisional; code pushed. G1/G6 **not verified** here: this execution environment has no `cargo`/`rustc` and no two Minecraft test PCs. Never report LAN_VERIFIED until Control examines actual E2E evidence.
