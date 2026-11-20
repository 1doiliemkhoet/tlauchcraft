# G7.1 candidate audit (NOT_VERIFIED)

Baseline 9542acf30a055d4cdf163d0c7527b9c8bb58491b. PR #30 provider, tests and docs carried forward verbatim; no old PR closure.

Candidate catalog:
- Forge 1.16.5: [14.26.1-ForgeV1](https://modrinth.com/mod/customskinloader/version/wLkp5dpJ).
- Forge 1.20.1: [14.26.1-ForgeV2](https://modrinth.com/mod/customskinloader/version/CcAsu9W3).
- Source: [v14.26.1](https://github.com/xfl03/MCCustomSkinLoader/tree/v14.26.1). LocalSkin Legacy supports `model` default/slim through SkinSiteProfile and UserProfile. Config file is CustomSkinLoader/CustomSkinLoader.json. Default sources may precede LocalSkin; an explicit local-only loadlist is required for the intended privacy mode. Do not infer Slim from PNG alone.

Run `cargo run --locked -p tl-pack --example skin_candidate_audit` on Windows with network access. It checks exact release IDs/loader/version, no required/incompatible API dependencies, primary file identity, CDN origin, byte length, SHA1/SHA512 from the provider and JAR manifest; prints observed SHA256. Review binary metadata and licensing before pinning artifact digests or enabling installation. No bytes are published or executed.

Current status: download-only audit implementation, not yet run; Forge provider integration, local-only config lifecycle, verified inventory, UI and F5 acceptance pending. Existing Fabric adapter behavior is unchanged. No edits to LAN Properties or import pipeline.
