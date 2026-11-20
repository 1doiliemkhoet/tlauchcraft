# LAN Properties opt-in experiment

Baseline: Forge 1.20.1 / 47.4.0 (SkyFactory 5 target). Not an E2E pass.

- Opt in during installed CurseForge folder import. The source profile stays read-only.
- Reviewed candidate: https://modrinth.com/mod/lan-properties/version/pQ1OCU76 (Apache-2.0; upstream https://github.com/xhyrom/lan-properties).
- No automatic change to Minecraft account, privilege, or authentication settings.
- Fixed project/version/primary filename; strict HTTPS origins and bounds, provider SHA-1 and SHA-512, Forge metadata presence, and an archived SHA-256 override receipt. Required/incompatible dependencies and likely duplicate LAN mods block import.
- Re-import without the opt-in to create a clean instance. This is not a hot-removal operation on a published READY instance.
- Check correct Java 17 and actual SkyFactory 5 gameplay first. On an authorized installation use a disposable world and verify normal LAN hosting, joining, and reconnecting with two permitted sessions. Do not mark G6 verified from CI alone.
- Live Modrinth metadata/download and two-client Windows gameplay remain unverified in this change.
