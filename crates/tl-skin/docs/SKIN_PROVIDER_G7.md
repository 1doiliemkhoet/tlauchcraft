# Chat 6 — Offline skin provider adapter (G7 still NOT_VERIFIED)

## Vanilla Java 1.21.1 limitations

A selected PNG inside a launcher **does not** become an in-game skin automatically. Vanilla Java uses profile/texture properties normally associated with online services and texture signatures. Offline/non-premium profile IDs are not authenticated skin credentials; vanilla's offline default skin is expected unless another supported client/server mechanism loads a custom texture. There is no Minecraft-JAR patch, online skin-service impersonation, or premium account bypass in this implementation.

Sources:
- https://minecraft.wiki/w/Skin
- https://github.com/xfl03/MCCustomSkinLoader
- https://github.com/xfl03/MCCustomSkinLoader/wiki/How-to-Use

## Implemented adapter — CustomSkinLoader LocalSkin

`tl_skin::provider::CustomSkinLoaderLocal` implements the separate `SkinProvider` trait. It is a **file preparation adapter, not a mod installer**.

- Only attempts local staging for **Fabric + Minecraft 1.21.1** with an explicitly supplied `customskinloader` artifact and exact SHA-256; the installed JAR must exist in the instance's `mods/` directory. The expected SHA must originate from a trusted, independently verified source; matching it to the local file alone does not establish mod identity, correct version or actual loader operation.
- Requires **explicit consent**. Does not download or install mods, change Minecraft JARs, touch saves/worlds or send data to a network.
- Copies the selected PNG from `SkinLibrary` to an isolated game directory's `CustomSkinLoader/LocalSkin/skins/<username>.png` (CustomSkinLoader's documented default path). Staging is opt-in and never writes to the shared Minecraft user directory on behalf of this API; caller owns safe instance path resolution.
- Checks `OfflineProfile.id` and `GameInstance.profile_id` match; respects `SkinLibrary.selection(profile.id, Some(instance.id))` and checks the Foundation profile catalog binding. `predicted_game_uuid` is for diagnostics only, not the internal stable UUID or an identity proof.
- Refuses symlinked paths and pre-existing unmanaged skins; uses a receipt and hash to detect external changes, and attempts rollback on failed updates. The caller must serialize concurrent writes and not run while Minecraft/provider is reading the file; crash-consistency and Windows junction protections need broader audit.
- **Classic** file staging supported; **Slim** is preserved in typed identity/selection but is **PreviewOnly / SlimModelNotConfirmed** until a tested provider-specific slim-model contract exists. Do not claim Slim arms render simply because a PNG was copied.
- A successful check/stage returns `Visibility::ProviderReadyUnverified`; it does not mutate `AppearanceStatus` to LocalVerified, and cannot grant peer verification.

### Explicit states

| Situation | State | Reason |
|---|---|---|
| Vanilla 1.21.1 offline | `PreviewOnly` | `UnsupportedVanilla` |
| No user consent | `PreviewOnly` | `ConsentRequired` |
| Missing or SHA-mismatched client mod | `PreviewOnly` | `ModMissingOrUntrusted` |
| Unsupported version/loader, or unconfirmed Slim model | `PreviewOnly` | specific compatibility reason |
| Safe, consented Fabric 1.21.1 local-file preparation | `ProviderReadyUnverified` | `ReadyUnverified` |
| Real skin visible to host in a genuine game, with evidence | `LocalVerified` | separate manual G7 gate, NOT set by adapter |
| Actual second player can see the skin on second client | `PeerVerified` | separate manual two-client G7 gate, NOT set by adapter |

### Unsupported / known limitations

- No mod installation and **no automatic version compatibility inference from arbitrary JAR filenames**; a caller must validate mod metadata/compatibility independently. End-to-end behavior of CustomSkinLoader in this particular 1.21.1 Fabric instance remains unverified.
- LocalSkin is primarily local client visibility. A peer will **not** automatically see your chosen PNG. The other client requires its own compatible provider and a permitted texture lookup/sharing mechanism; no broadcast or guaranteed LAN support is provided by this adapter.
- Vanilla 1.21.1 (no loader) is deliberately rejected rather than claiming custom skin. Forge and other Minecraft versions are not validated by this conservative adapter.
- CustomSkinLoader may prioritize online providers above LocalSkin in its load list; users must explicitly configure LocalSkin priority if necessary. The adapter never silently alters existing mod config.
- The existing skin library's 64×32 legacy atlas conversion and cape handling are outside this provider adapter and need separate verification.

## Required manual Singleplayer G7 acceptance (Fabric 1.21.1)

1. Back up the isolated instance. Install an official, compatible Fabric loader and a legitimate CustomSkinLoader client JAR from the upstream distribution **with user consent**. Independently check mod identity, game version and SHA-256; ensure its LocalSkin source is enabled/prioritized. Never copy a fake test fixture JAR into production.
2. Import a distinctive **Classic** PNG in the skin library, bind a Foundation profile, and call `check` then `stage`. Capture returned report and verify image exists in the intended instance directory. `ProviderReadyUnverified` is still not success.
3. Launch actual Fabric 1.21.1 using **the same instance game directory and the same exact username**. Open a Singleplayer world, press **F5** (toggle third person), capture screenshot showing the distinctive skin, the game version/loader and session logs (do not include private tokens or unrelated data).
4. Close Minecraft, relaunch **same profile**, reopen the same world, F5, capture second screenshot. Restart launcher, verify selection persisted, relaunch and capture third screenshot. Repeat with a second profile and after toggling an instance override.
5. Test missing provider, wrong version and unsupported Slim selection; launcher must display PreviewOnly and not claim runtime success.
6. Only Control may mark LocalVerified after reviewing actual screenshots, instance paths and provider logs; preview or PNG-on-disk alone is not evidence.

## Required LAN two-client acceptance (coordinate Chat 5)

Use trusted LAN only. Two independently configured clients and two actual players/PCs must join the same controlled server/world, using matching mod/version and consent settings. Record each client's screenshot showing the *other* player's distinctive skin, together with logs, the selected profile names, integrity hashes and cache-refresh/reconnect behavior. Check no skin bytes are sent without opt-in. Local-only success is not PeerVerified; a single screenshot from the host alone is insufficient.

## Integration with Chat 2/3/5/7

- Chat 2/3 own version-aware loader and mod installation, verified artifact metadata and the **prelaunch provider hook**, with typed `ProviderContext` (game dir, profile, instance, consent, mod digest).
- Chat 5 owns optional trusted LAN cosmetic transport/compatibility and peer tests, not this client's local file adapter.
- Chat 7 owns Tauri/Desktop UI and 3D preview; use typed `SkinProvider::check/stage` and display reasons without changing this crate's statuses.
- Preserve `OfflineProfile.id` separately from offline Minecraft UUID. Name changes alter game identity and LocalSkin file name; the adapter intentionally blocks overwriting stale managed records until user resolves the rename.

### Tests

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

The provider tests use synthetic JAR-header fixtures solely to exercise safe file staging and rejection; **they do not test real Fabric or CustomSkinLoader**. Only real game screenshots and two-client observation can satisfy G7.
