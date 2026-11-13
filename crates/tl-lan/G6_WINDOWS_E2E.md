# G6 Windows two-machine LAN protocol (NOT EXECUTED)

**Status: NOT_TESTED.** Run only after Chat 1–4 deliver a runnable, verified vanilla/Fabric server and actual Windows launcher client. No fabricated pass. Do not publish screenshots/logs containing player IPs, local account names or sensitive paths.

## Preflight / safety, on Windows host A

Use a private switch/router and two PCs with separate offline usernames. Never enable router port forwarding, NAT, UPnP, RCON or public firewall rules. Offline Minecraft UUIDs are spoofable on networks reachable by adversaries; whitelist is insufficient protection by itself. Accept the Minecraft EULA manually after reviewing the license and source.

```powershell
Get-NetConnectionProfile
Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -match '^(10\.|192\.168\.|172\.)' }
Get-NetTCPConnection -State Listen -LocalPort 25565 -ErrorAction SilentlyContinue
```

Confirm target NIC is Windows **Private**, host IP is actually RFC1918 and 25565/TCP free. Choose a server bind equal to the host's specific private adapter IP (never `0.0.0.0`). Scope firewall to approved **Private** NIC and source IP/subnet using administrator-reviewed Windows UI; do not make an automatic open rule.

## Both PCs — runtime & modpack parity

Both clients must install the *same legitimate* Minecraft version, supported Java runtime, loader exact version and verified gameplay-relevant mods/config. Server must use the matching dedicated loader recipe, not assume all modded servers accept `java -jar`. Compare Chat 4 lockfile hashes on both PCs, excluding explicitly client-only cosmetic/shader files and server-only mods. Record exact game/loader versions and lockfile hash. Never copy third-party game binaries without permission.

```powershell
java -version
Get-FileHash '.\path-to-verified-pack-lockfile' -Algorithm SHA256
Test-NetConnection -ComputerName 192.168.1.10 -Port 25565
```

Replace example address and lockfile path with actual operator-approved local values. A failed `Test-NetConnection` indicates TCP/firewall/bind/path trouble; a successful TCP probe **does not** prove Minecraft handshake, loader compatibility or player identity.

## Game session — G6 acceptance sequence

1. Host A starts isolated dedicated server after explicit offline-mode warning/EULA acceptance. Capture PID, Java version, server `latest.log`, listening bind/port and zero port-forwarding configuration. Require game **READY** log, not merely process spawned.
2. Client A and B use *different, stable* offline usernames and Direct Connect to host A's RFC1918 IP:port. Capture client logs, screenshot of both present, server player join records; verify usernames and observed server UUIDs. No claim that app profile UUID equals server UUID.
3. Craft or obtain a modded item on both clients, place modded blocks, exchange items in a chest, then move to known world coordinates. Save screenshots, block metadata/test item IDs and inventory slots.
4. Disconnect B, reconnect B, verify inventory, position, chest contents and placed blocks persist; then disconnect both. Issue `stop` in server console; wait for clean termination and **only then** back up `level.dat`, `region/`, `playerdata/`, plus modded dimension data. Generate per-file SHA256 evidence for before/after snapshots without altering saves.
5. Restart server, reconnect both A and B, verify same items/world/player inventory; confirm playerdata filename identity and timestamp/hash changes consistent with saves. Never assume byte hashes remain identical after legitimate world saves.
6. Negative tests: mismatch required gameplay mod SHA256; mismatch loader version; same username for A/B; port already occupied; public bind; Windows Public network or blocked port. Refuse misleading compatibility promises before connection. Distinguish preflight vs real Minecraft disconnect reason.
7. Profile rename: backup stopped server first; attempt name change and confirm Chat 5 blocks it until Control-approved migration workflow. Keep original username unchanged for acceptance. Compare observed server player UUID against expected offline-mode mapping only as a separate **test**, not a guaranteed algorithm across mod/plugin server environments.

## Evidence to retain for Control

A record per PC containing time, anonymized machine ID, game and loader version, Java path/version, pack lockfile SHA256, game client PID/log path and server log PID/path. Capture host bind/port, firewall Private/subnet inspection, loader server startup evidence, successful two-player in-game interaction, disconnect/reconnect and save/restart outcomes, and any resulting client/server exceptions. Put hashes, timestamps and safe redacted logs in `evidence/` via Control-approved PR. Do **not** commit purchased game assets, playerdata snapshots, identifiers or raw proprietary mod JARs.

## Gate criteria

**G6 remains NOT_VERIFIED** unless Control independently inspects real two-PC traces demonstrating join, mod interaction, reconnect, server restart and persistence of world + inventory. Cargo unit tests or socket-only probes are never G6 evidence.
