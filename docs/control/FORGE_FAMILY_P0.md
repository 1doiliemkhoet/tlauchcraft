# TLaunchCraft — Forge installer family P0 (18 Nov 2026)

## Scope

**No universal modpack claim.** The currently implemented full Windows Forge runtime
uses the Java 8 / Minecraft **1.16.5** four-processor chain. The exact manifest-selected
Forge version is passed through official installer acquisition, SHA-1 validation,
processor preparation/execution, complete generated JAR inventory and Play verification.

| Minecraft | Forge | Official installer SHA-1 | State |
| --- | --- | --- | --- |
| 1.16.5 | 36.2.29 | `7d247279cf06eed6ffcf609f36d11e34c70350f3` | **IN_WORLD_VERIFIED** (Windows Rustic Waters II screenshot) |
| 1.16.5 | 36.2.34 | `ab306b654c44d659ce69da5d4f87590b66dc91e8` | Adapter wired, official metadata audited; gameplay **NOT_VERIFIED** |
| 1.16.5 | 36.2.42 | `e09ecf910e4d5eae12fb3564d9b7de212c1958b2` | Adapter wired, official metadata audited; gameplay **NOT_VERIFIED** |

The official SHA-1 list was checked against Forge's
[1.16.5 downloads index](https://files.minecraftforge.net/net/minecraftforge/forge/index_1.16.5.html).
The GitHub workflow `.github/workflows/forge-family-audit.yml` fetched each canonical
installer, rehashed the **actual archive**, extracted only `install_profile.json` and
`version.json` as data, and recorded independently pinned metadata hashes.
It **never executes** a Java process. The 36.2.42 profile changes the approved
first processor from `installertools:1.2.6` to `installertools:1.3.0`; the
other three approved client processors retain their reviewed identities.

## Supported import workflows

1. Import installed CurseForge folder + original profile ZIP (locally observed mod hashes).
2. CurseForge archive with provider-authorized mod downloads, subject to API permissions.

Both use the selected Forge version from the archive manifest and refuse unreviewed
versions **before starting host Forge Java execution**. The approved Windows runner
still requires explicit consent, real Java 8, checksum-pinned Mojang and Forge inputs,
Windows Job Object containment, immutable processor output proofs and strict
published artifact rehash on READY/Play. It must not touch the source CurseForge
world folder. The successful 36.2.29 instance remains valid and unchanged.

## Not yet supported

- Other Forge Minecraft families (1.12, 1.18, 1.19, 1.20 and later): distinct
  installer schemas/processor contracts and Java requirements need separate
  reviewed adapters.
- Fabric, NeoForge or Quilt **installation+play**. Metadata parsing is not a
  playable installer; do not convert it to READY based on parsing alone.
- Cross-version or cross-modpack LAN. Multiplayer needs same loader, versions
  and compatible mods and an actual two-client acceptance run.

## Acceptance before claiming new versions playable

- Windows Rustfmt, Clippy, all workspace tests, Tauri checks and NSIS smoke.
- Real Windows Forge 36.2.34 and 36.2.42 processor execution + READY artifacts,
  followed by screenshot proving actual modded Minecraft world/menu.
- Save / exit / relaunch / reopen world, then offline LAN between two clients.
- If a release-specific processor graph or upstream checksum differs, fail
  closed and add a new audited adapter instead of bypassing verification.

## Next phase

Implement reviewed, version-driven adapters for additional Minecraft families,
and then separate Fabric and NeoForge installers. Do not hardcode Minecraft 1.16.5
in generic game/runtime or UI logic.
