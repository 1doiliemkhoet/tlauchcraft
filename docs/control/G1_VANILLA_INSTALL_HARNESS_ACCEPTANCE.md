# CONTROL — Chat 2 Windows Vanilla install/harness source acceptance

**Reviewed:** 2026-11-14
**PR:** [#19](https://github.com/1doiliemkhoet/tlauchcraft/pull/19)
**Accepted feature HEAD:** `395590618a307f5300671d88d84483fca3d3e29c`
**Merged:** [`f12a8676`](https://github.com/1doiliemkhoet/tlauchcraft/commit/f12a867663394b47be8f5caff28a0cd89609d782)
**Windows CI:** [run 43596210965](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43596210965)

## Accepted — G1 source and automated Windows tests, not gameplay

- 7-crate root Cargo workspace preserved; `cargo fmt --all -- --check`, strict `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo test --locked --workspace --all-targets` PASS, **92 tests passed, 0 failed**.
- Windows-only Vanilla metadata resolution and version SHA-1 validation, Mojang client/library/native/archive/asset-index and object planning, existing-file integrity verification, Java version probe and classpath/argument construction.
- Replaced permissive native `tar.exe` extraction with a bounded two-phase PowerShell/.NET ZIP DLL-only reader. ZIP safety fixtures include traversal, links, and duplicate filenames. This is automated fixture coverage, **not** a complete native compatibility or security audit.
- `VanillaIdentity::from_offline_profile` aligns game UUID with `tl_core::derive_vanilla_offline_identity` while maintaining separation from the internal profile ID; username case and rename effects remain cautionary.
- Manual `vanilla_e2e` Windows harness emits download checksums, PID and stdout/stderr paths and world snapshot metadata, **without automatically asserting a visible game menu or re-opened world**.
- No game or Java binaries or user credentials distributed in this repository.

## Gates NOT_VERIFIED

- **G2 NOT_VERIFIED:** No observed real Vanilla client/assets/libraries/natives install end-to-end on an authorized Windows PC, with verified Java/runtime.
- **G3 NOT_VERIFIED:** No genuine in-game Minecraft main menu screenshot and correlated client/game log proving successful launch. Process spawn alone is insufficient.
- **G4 NOT_VERIFIED:** No real-world create/save/quit/relaunch/reopen proof with the same world state.
- The main desktop Play capability is **not enabled by this PR**. It does not claim a complete usable launcher.
- Real-world compatibility, security, entitlements and auth remain outside this automated test acceptance.

## Next acceptance work

Run the harness on a real interactive Windows 10/11 machine with a suitable authorized Minecraft client and compatible Java, record SHA-1 data, PID/log, actual menu screenshot and world before/after evidence. Keep data in a disposable test instance and redact tokens. Use a new PR/evidence issue after this G1 scope merge.
