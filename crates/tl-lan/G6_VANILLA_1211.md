# Chat 5 — Vanilla 1.21.1 LAN G6 evidence procedure

**Gate:** NOT_VERIFIED. This is a test harness and an on-device operator procedure, not a claim that two Minecraft clients have connected.

## Preconditions

- Start from the **verified** Vanilla 1.21.1 client created by Chat 2 on Windows. Do not substitute a mock, a GUI-only launcher, or a random downloaded server JAR.
- On the trusted private LAN only, prepare a **legitimately obtained** Minecraft Java 1.21.1 dedicated-server JAR under a new, empty and isolated server directory. Never overwrite existing worlds. Operator must read/accept Minecraft EULA themselves.
- For local-only smoke checks, TCP loopback is suitable for port diagnostics, but the existing trusted-LAN server API deliberately disallows loopback as a **server bind**. A real host must bind to a specific RFC1918 adapter IP. Do not use `0.0.0.0`.
- On each PC, use a distinct username containing 3–16 ASCII letters/numbers/underscores. Preserve its exact case through every rejoin. Chat 1's internal `OfflineProfile.id` is NOT the filename key for `world/playerdata`.

### Stage A — one PC, non-destructive preflight

From the repo worktree:

```powershell
git switch feat/tl-05-lan-e2e
cargo test --locked -p tl-lan
Get-NetConnectionProfile
```

The test suite uses **fixtures only**, not a Minecraft server. Ensure the network adapter for hosting is **Private** and under an intentionally trusted subnet. If not, do **not** start an offline server.

Inspect exact host address and port (example 25565):

```powershell
Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -match '^(10\.|192\.168\.|172\.)' }
Get-NetTCPConnection -LocalPort 25565 -ErrorAction SilentlyContinue
Get-FileHash .\path\to\verified-modpack-lockfile -Algorithm SHA256
```

Last command applies to modded packs; Vanilla uses a matching Minecraft version and loader=`vanilla`, version empty, not a fabricated modpack lockfile. For each run log the source metadata/verified version and player names, never a downloaded JAR in Git.

### Stage B — one-PC localhost and LAN-address smoke

Create a disposable world and manually start a legitimate dedicated server with `online-mode=false` **only** after reviewing that the specific interface IP and Windows Private firewall policy restrict access to trusted devices; whitelist cannot prevent offline username impersonation. Never auto-add firewall rules or router forwarding.

Before using the game, check that the server is listening on the intended interface only:

```powershell
Get-NetTCPConnection -State Listen -LocalPort 25565 | Select-Object LocalAddress,LocalPort,OwningProcess
Test-NetConnection -ComputerName 192.168.1.10 -Port 25565
Get-Process java* -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,Path
```

Replace `192.168.1.10` with actual RFC1918 host address. TCP reachability is only a **preflight**; it cannot prove Minecraft login or inventory persistence.

Start actual Minecraft 1.21.1 client (Chat 2 verified pipeline). Use **Multiplayer → Direct Connection → host-ip:25565**. Capture real logs for server readiness, player login, and observed offline UUID. Exercise the disconnect and rejoin flow with the **exact same** username. The fixture `SessionTrace` is not proof of a server session.

After all clients disconnect and the server confirms a **clean stop**, inspect existing playerdata (read-only):

```powershell
Get-ChildItem '.\isolated-server\world\playerdata' -Filter '*.dat' | Select-Object Name,Length,LastWriteTime
Get-FileHash '.\isolated-server\world\level.dat' -Algorithm SHA256
Get-ChildItem '.\isolated-server\world\playerdata' -Filter '*.dat' | Get-FileHash -Algorithm SHA256
```

**Do not compare hashes across legitimately changed saves as if equality were required.** The test must show the same server UUID/playerdata filename, existing inventory/items and world state *inside Minecraft* after rejoining. The helper `snapshot_stopped_playerdata` reads existing bytes and never writes the world; it is only an integrity regression fixture, not a Minecraft/NBT validator.

### Stage C — Control two-PC G6

1. **PC A host:** run dedicated Vanilla 1.21.1 server on an approved Private RFC1918 address, same verified version, `online-mode=false` only for trusted LAN, and user-approved EULA. No public firewall exceptions, UPnP, NAT or port forwarding.
2. **PC A/B clients:** each starts 1.21.1 with different, fixed offline usernames. Verify that both can join simultaneously, place blocks, trade items into a chest and see changes in the same world. Record server-side observed UUID and Java/client logs.
3. **Disconnect and reconnect PC B:** confirm inventory, chest blocks, spawn, position and UUID. Disconnect both, clean-stop server, back up world, record `level.dat` and `playerdata/<uuid>.dat` filenames, timestamps and hashes. Restart same world/server, rejoin both, and verify persisted state in-game. Never change username, game UUID or world file names to force a match.
4. **Negative cases:** wrong username case, wrong 1.21.1 version, wrong loader, missing/changed gameplay mod hash on modded build, port occupied, wrong LAN interface, failed TCP connection, and duplicate nickname. Separate preflight rejection from actual Minecraft handshake refusal.

## Evidence delivery and gate

Save a redacted observation record including client/server Minecraft version, Java version, server PID, process exit code, bind/port, trusted network verification, both usernames anonymized consistently, observed server UUIDs, verified lockfile/hash, exact client/server start and join timestamps, world/playerdata paths anonymized, hashed snapshots, screenshot of both clients in the same world, modded item interaction (when testing modpack), and reconnect/restart results.

**Unit tests and one-PC/loopback runs do NOT grant G6**. Control grants G6 only after reviewing actual two-PC interaction and save/rejoin evidence. Do not commit raw playerdata, game binaries or personal logs into the repository.
