# G7.1 candidate audit (NOT_VERIFIED)

Baseline 9542acf30a055d4cdf163d0c7527b9c8bb58491b. PR #30 provider, tests and docs carried forward verbatim; no old PR closure.

Candidate catalog:
- Forge 1.16.5: [14.26.1-ForgeV1](https://modrinth.com/mod/customskinloader/version/wLkp5dpJ).
- Forge 1.20.1: [14.26.1-ForgeV2](https://modrinth.com/mod/customskinloader/version/CcAsu9W3).
- Source: [v14.26.1](https://github.com/xfl03/MCCustomSkinLoader/tree/v14.26.1). LocalSkin Legacy supports `model` default/slim through SkinSiteProfile and UserProfile. Config file is CustomSkinLoader/CustomSkinLoader.json. Default sources may precede LocalSkin; an explicit local-only loadlist is required for the intended privacy mode. Do not infer Slim from PNG alone.

Run `cargo run --locked -p tl-pack --example skin_candidate_audit` on Windows with network access. It checks exact release IDs/loader/version, no required/incompatible API dependencies, primary file identity, CDN origin, byte length, SHA1/SHA512 from the provider and JAR manifest; prints observed SHA256. Review binary metadata and licensing before pinning artifact digests or enabling installation. No bytes are published or executed.

Current status: download-only audit implementation, not yet run; Forge provider integration, local-only config lifecycle, verified inventory, UI and F5 acceptance pending. Existing Fabric adapter behavior is unchanged. No edits to LAN Properties or import pipeline.

## Integrated candidate implementation

Audit run [44708993805](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/44708993805) completed successfully:
- 1.16.5: 199437 bytes SHA256 9ff705d44c6543a3e7861841a1b081c3b7f4a148ba575ba7d97066c507eddf45.
- 1.20.1: 153797 bytes SHA256 b3484b9254e1f409d1b1402d3ad7ac751833e22d9dc069017bbaa405760b77fc.

The audited binary SHA256 is now pinned in addition to official metadata checks. Folder-only import opt-in adds the verified JAR to the private snapshot. Publisher verifies it as a pinned JAR override and the SQLite receipt anchor covers it. Play selects the provider only from the rehashed, anchored receipt. Local-only Legacy config is generated before Java launch; no auth modification. Skin studio imports PNG, previews the atlas, selects model and assigns profile/instance. It does not provide 3D preview, LAN synchronization, or claim runtime visibility. A single model applies to the local Legacy source until G7.2 is proven.

Windows acceptance: select profile, open Skin studio, Import PNG, choose Classic or Slim, Use for profile. Import installed Forge folder with Custom Skin checked (LAN Properties optional independent). Select the resulting instance, Play, enter world, F5. Capture actual appearance and CustomSkinLoader/CustomSkinLoader.log. Exit and restart; change texture and model and retest. Test an unmodified existing instance without reimport, and both versions. Do not add JARs manually. If UI reports an unmanaged config, do not overwrite/delete it blindly; report the error. CI and in-game status are separate.
