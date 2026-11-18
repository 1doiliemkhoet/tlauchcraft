//! CI-only metadata audit: *never executes* a downloaded Forge installer.
//! The caller must download and SHA1-verify the official installer ZIP first.
use serde_json::Value;
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 5 {
        return Err("usage: forge_family_audit <minecraft> <forge> <profile.json> <version.json>".into());
    }
    let release = tl_loader::forge_family::approved_forge_release(&args[1], &args[2])?;
    let profile_text = fs::read_to_string(&args[3])?;
    let derived: Value = serde_json::from_slice(&fs::read(&args[4])?)?;
    release.verify_metadata(&profile_text, &derived)?;
    println!("PINNED_FORGE_FAMILY_RUST_VERIFIED {} {}", release.minecraft, release.forge);
    Ok(())
}
