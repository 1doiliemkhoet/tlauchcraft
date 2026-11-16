# P0 — Shared Forge 1.16.5 LAN acceptance, Issue #31

**Gate G6: NOT_VERIFIED until real two-client gameplay is observed.** This document reuses the single shared Forge/modpack run from Chat 3 + Chat 4. It does not re-run previously accepted Vanilla 1.21.1 download tests or Forge preflight.

## Handoff required from Chat 4 (machine-readable, no secrets)

For both PC A (host) and PC B (peer), Chat 4 should supply the actual installed instance root, the **same manifest archive SHA-256**, Minecraft version **1.16.5**, the manifest's exact Forge loader version (never assume 36.2.29), a verified publish receipt, and a complete list of installed gameplay/mod/config file paths, SHA-256, side and gameplay flag. An incomplete installer is `NEEDS_ACTION` rather than success.

`tl_lan::forge_handoff::InstalledPackReceipt` defines the initial typed adapter. `fingerprint_installed_pack(root, receipt)` recomputes file SHA-256 from real installed bytes and refuses missing/tampered paths. `compare_two_clients` enforces **exact client-to-client parity**, including client-only mods, side metadata, version and hash. Both clients must independently pass. This does **not** authorize `G5`, fabricate a runtime receipt or prove Minecraft boot. Chat 4 must only set `installed_and_verified=true` after its own verified install, and `loader_execution_verified=true` after Chat 3's real outputs have been verified. A caller-supplied bool alone is not cryptographic attestation.

For evidence, retain each instance ID, source manifest SHA, package and Forge version, SHA-256 inventory, source commit HEAD, actual file verification errors and the *same run_id* on every stage. Avoid checking files in user-owned legacy instances or touching existing saves.

## Phase 0 — hard isolation (no LAN)

Forge installer processor execution in the approved FLARE-VM stays **network-off, clipboard/USB/shared mounts off**, until Chat 3 and Control accept its execution. This LAN procedure **must not** re-enable any guest NIC or weaken containment. Use a separate trusted Windows LAN environment only **after** the isolated processor stage.

## Phase 1 — preflight (Control: <= 3 commands)

On the Windows host with the PR checked out:

```powershell
git switch feat/tl-05-lan-e2e
cargo test --locked --workspace --all-targets
Get-NetConnectionProfile
```

Confirm the Windows network profile is Private and physical network restricted to trusted peers. Unit tests are code-quality evidence, **not G6**. Chat 4 supplies identical manifests and independently byte-verified SHA-256 fingerprints; mismatches stop this gate. Offline UUID from Chat 1 is based on the **exact case-sensitive** username. Never replace `profile_id` with `offline_server_uuid` or silently rename a user's playerdata.

## Phase 2 — actual LAN session (one shared modded-world run)

The selected Forge 1.16.5 modpack must already have passed G2 (processor execution), G3 (real launch), G4 (re-opened modded save) and G5 (fully installed pack). Control starts **host PC A's verified modded instance** using TLaunchCraft, creates/loads a **new disposable test world**, chooses *Open to LAN* and records the port printed by Minecraft. Do not assume 25565: integrated-server LAN commonly selects a dynamic port. Select an existing private IP on PC A. Host and peer must have different stable offline usernames and distinct canonical server UUIDs. Both clients must be game-visible; TCP-only port tests do not count.

One concise network check on PC B (use actual host IP and displayed port):

```powershell
Test-NetConnection -ComputerName 192.168.1.10 -Port 51234
```

This is a **placeholder** command: replace both values with exact trusted Private LAN host IP and Minecraft Open-to-LAN port. No public bind, router port-forwarding, UPnP or automatic Windows Firewall modifications. If firewall change is needed, stop and request Control's explicit, narrowly scoped approval.

PC B launches the **same verified modpack** through TLaunchCraft and Direct Connects to host IP:port. Record actual join in the server/client `latest.log` and screenshots of both players at the same place. Exchange modded items/blocks and inventory; record shared chest and coordinates. Quit peer B, reconnect B with **identical username** and verify both player inventory and shared world state.

For modded world persistence, exit both clients cleanly, reopen PC A's **same world** through TLaunchCraft, Open to LAN again (new port possible), then rejoin peer B and verify modded blocks/items, chest and player inventory. This is the single shared A/B/C acceptance run; do not reinstall packs or restart Forge processors for G6.

## Phase 3 — evidence (read-only after all clients stopped)

On PC A, replace the path with the **disposable** world created for the test:

```powershell
Get-ChildItem '.\test-instance\saves\LAN-Test-World\playerdata' -Filter '*.dat' | Select-Object Name,Length,LastWriteTime
Get-ChildItem '.\test-instance\saves\LAN-Test-World\playerdata' -Filter '*.dat' | Get-FileHash -Algorithm SHA256
Get-FileHash '.\test-instance\saves\LAN-Test-World\level.dat' -Algorithm SHA256
```

Hash changes across saves are expected. Verify the **same player UUID filename** and actual in-game inventory; hashes alone cannot prove NBT contents. Integrated singleplayer host data may be stored in `level.dat` rather than a separate `playerdata/<uuid>.dat`. Record observed files instead of assuming both players have playerdata files. Preserve world, screenshots/logs, timestamps and run_id without uploading private saves or personal information.

### Verdicts (independently adjudicated)

- **A / G2-G3-G5:** Forge processor/install, complete pack publish and actual 1.16.5 game launch — owned by Chat 3/4, PASS only with their receipts and GUI logs.
- **B / G4:** same modded world reopened after clean exit — PASS only with actual save/reopen game screenshots and logs.
- **C / G6:** two independently launched modded clients in same LAN world, peer reconnects, inventory and world persist — PASS only after Control examines actual two-client evidence.

**Fail closed** if different loader versions, modpack manifest hashes, installed bytes, offline usernames, isolation stage or LAN binding. G6 remains NOT_VERIFIED even if unit tests, a loopback probe or two snapshots pass.
