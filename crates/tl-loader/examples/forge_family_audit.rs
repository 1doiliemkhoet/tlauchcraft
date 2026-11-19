//! CI-only metadata audit: *never executes* a downloaded Forge installer.
//! The caller must download and SHA1-verify the official installer ZIP first.
use serde_json::Value;
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(
            "usage: forge_family_audit <minecraft> <forge> <profile.json> <version.json>".into(),
        );
    }
    let profile_text = fs::read_to_string(&args[3])?;
    let derived: Value = serde_json::from_slice(&fs::read(&args[4])?)?;
    if args[1] == "1.20.1" && args[2] == "47.4.0" {
        tl_loader::forge_family::audit_forge_1201_metadata(&profile_text, &derived)?;
        println!("FORGE_1201_METADATA_VERIFIED_NOT_PLAYABLE");
        return Ok(());
    }
    let release = tl_loader::forge_family::approved_forge_release(&args[1], &args[2])?;
    release.verify_metadata(&profile_text, &derived)?;
    println!(
        "PINNED_FORGE_FAMILY_RUST_VERIFIED {} {}",
        release.minecraft, release.forge
    );
    Ok(())
}
