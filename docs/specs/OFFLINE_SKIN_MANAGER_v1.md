# Offline Skin Manager & Editor — Functional Contract v1

**Status:** Design only; no implementation or in-game verification.
**Scope:** Offline/local player profiles, Singleplayer, modpacks and LAN. Microsoft/Premium login remains out of scope.
**Priority:** P0 local import/preview/apply; P1 full editor, LAN sync, and custom capes.

## Objectives
Users can import, select and edit custom Minecraft Java skins without Microsoft authentication. Skin preview alone is not enough: the selected skin must be rendered in a *real* Singleplayer session. LAN visibility requires separate implementation and E2E tests on two physical PCs.

## Functional requirements

| ID | Priority | Feature | Acceptance |
|---|---|---|---|
| SK-01 | P0 | PNG importer | Support 64x64 and legacy 64x32, reject corrupt/oversized files and unsupported image formats |
| SK-02 | P0 | Local skin library | Save and rename skins, duplicate, delete with confirmation, store SHA-256 and export PNG |
| SK-03 | P0 | Classic / Slim | Choose 4-pixel or 3-pixel arms independently for each selected skin |
| SK-04 | P0 | 3D skin preview | Rotate/zoom model; preview head, body, arms, legs; toggle overlay layers |
| SK-05 | P0 | Offline profile linkage | Store selected skin and model under stable offline profile ID; persist across restart |
| SK-06 | P0 | Apply in game | Use game-version/loader-aware compatible skin adapter or mod; verify render in real Minecraft, not only UI |
| SK-07 | P0 | Provider compatibility | Warn clearly if vanilla/mod-loader/provider cannot display local skins; never claim success falsely |
| SK-08 | P1 | Pixel editor | Pencil, eraser, fill, eyedropper, mirror, palette, transparency overlays, undo/redo, layer grid |
| SK-09 | P1 | Legacy conversion | Convert 64x32 to 64x64 skin layout correctly when requested |
| SK-10 | P1 | LAN visible skins | Peers can see each other's skin via compatible server/client mod or opt-in LAN sync |
| SK-11 | P1 | Custom capes | Import/preview local cape images; apply and share only if compatible provider supports it; not official capes |
| SK-12 | P1 | Instance-specific override | Per-instance appearance optionally overrides global profile default |
| SK-13 | P1 | Cosmetic packs | Import/export a ZIP of skins and optional capes after validating archive; never alter mods or worlds |

## User flow
1. Profiles → Skins → Import PNG, Create Skin or Open Skin Library.
2. Validate the PNG and show Classic versus Slim previews.
3. Open the 3D viewer or pixel editor; allow undo/redo and layers.
4. Save or apply to the selected local/offline player profile.
5. Before launching a Minecraft instance, validate loader/version/provider support, then copy/link skin only into the appropriate cosmetics storage.
6. In-game display is recorded as VERIFIED only after actual Singleplayer evidence.
7. For LAN, display two separate states: Local Skin Visible and Visible To Friends. The second is verified only with evidence from both clients.

## Components and storage
- SkinLibrary: catalog metadata, image fingerprints, thumbnails, import/export lifecycle.
- SkinEditor: 64x64 image editing with outer layers, proper UV mapping and Classic/Slim models.
- ProfileAppearance: stable local profile ID, chosen skin ID, model type, optional cape ID.
- SkinProviderAdapter: game version, loader, client/server mod requirements, support flags and deployment method.
- LanAppearanceSync: opt-in local-only skin distribution or compatible server/client mod with cache invalidation.
- Keep cosmetics in application-managed storage separate from user saves and imported modpacks.
- Never assume a client-only skin mod will automatically make the skin visible to other LAN players.
- Do not modify Minecraft client JAR files or assume all versions/loaders have equivalent skin APIs.

## Security, privacy, and failure modes
- Reject broken PNGs, unsupported dimensions, excessive decompression, malicious archive members, path traversal and filenames with unsafe characters.
- Handle transparency only where the Minecraft skin format allows it, and validate model proportions.
- No automatic upload to public services. LAN transfer requires explicit opt-in and bounded file sizes.
- Offline names may be spoofed on untrusted networks. Skin assignment alone is not secure player identity.
- Missing provider, unsupported loader, malformed image or local network downtime must not prevent the rest of the launcher from working.
- Check the license of third-party skin support mods before bundling or redistribution.

## Known reference implementations (NOT proof of our implementation)
- OfflineSkins (Fabric, client-side local PNG): https://modrinth.com/project/offlineskins
- LocalSkins explicitly documents client-side-only visibility: https://github.com/asterdoescomputing/LocalSkins
- StraySkins advertises skins and capes in offline multiplayer, with version/loader constraints: https://modrinth.com/mod/strayskins
- Minecraft Java skin dimensions/layout: https://minecraft.wiki/w/Skin

## Acceptance tests (all unexecuted)
1. Import standard 64x64 PNG, preview and export original pixels.
2. Import 64x32 legacy PNG; conversion renders arms and legs correctly.
3. Reject invalid/corrupt/oversized PNG with actionable error.
4. Change Classic/Slim and overlay layers; save and reload settings.
5. Paint a pixel, undo/redo, export/reimport; output visually consistent.
6. Apply selected skin in real offline-profile Singleplayer on supported version/loader; screenshot proves skin rendered.
7. Restart launcher and Minecraft; local profile retains same skin.
8. Import CurseForge/Modrinth modpack; skin adapter compatibility checked without corrupting pack.
9. Two profiles/instances hold different skins without interfering.
10. Two actual LAN PCs, same supported skin provider; each sees the correct skin of the other.
11. Unsupported provider reports local-only preview or rendering limitation, rather than claiming LAN sync.
12. Disable skin sharing; verify no LAN appearance transfer.
13. Optional custom cape works only where provider supports it; never claim official cape entitlement.
14. Import cannot overwrite mods, config, worlds or unrelated paths.

**Design completion is not an implementation PASS.** No Microsoft, Xbox, Premium skin account integration.
