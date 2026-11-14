# Offline identity contract — Issue #16
The persistent `OfflineProfile.id` is an internal application identifier. It is not the Minecraft Java offline client UUID and does not identify server playerdata.

For vanilla Java `online-mode=false`, the server commonly assigns `UUID.nameUUIDFromBytes(UTF8("OfflinePlayer:" + username))` (MD5 digest, RFC 4122 version 3 / variant bits). The algorithm preserves username **case**. The launch `effective_game_uuid` returned by this module is an explicit offline-mode proposal, **not proof** that any loader, proxy, plugin or tested MC version uses it. Chat 2 owns actual launch arguments, Chat 5 owns observed server UUID and playerdata.

Use `derive_vanilla_offline_identity(profile)` to obtain distinct named fields. Use `preview_rename_impact(profile,new_name)` *before* changing a username. On changed offline UUID, warn and require a safe stopped-world backup; do not silently rename playerdata or promise inventory continuity. Existing serialized profiles and instances are unchanged (no new mandatory fields).

The server remains limited to a trusted LAN; offline names can be spoofed. Never enable public exposure by default. G6 requires two real Windows peers with logs, resolved UUIDs, saves and rejoin evidence, performed by Chat 5. Integration and migration plans require Control approval.
