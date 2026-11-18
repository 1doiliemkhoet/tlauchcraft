//! CI-only non-executable validation of official Forge and NeoForge metadata.
//! Files are extracted as data from already SHA1-pinned official installer
//! archives by p1-multiloader-metadata-audit.yml. No Java is ever launched.
use std::{env, fs, process};
use tl_loader::loader_matrix::{inspect_reviewed_installer, inspect_reviewed_neoforge};

fn main() {
    if let Err(error) = inspect() {
        eprintln!("MULTILOADER_OFFICIAL_RUST_AUDIT_FAIL: {error}");
        process::exit(1);
    }
}

fn inspect() -> Result<(), String> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 6 {
        return Err("usage: cross_generation_audit <forge|neoforge> <minecraft> <loader> <profile.json> <version.json>".into());
    }
    let profile = fs::read_to_string(&args[4]).map_err(|e| e.to_string())?;
    let version = fs::read_to_string(&args[5]).map_err(|e| e.to_string())?;
    match args[1].as_str() {
        "forge" => {
            let release = tl_loader::loader_matrix::reviewed_cross_generation(&args[2], &args[3])
                .map_err(|e| e.to_string())?;
            let proof = inspect_reviewed_installer(
                &args[2],
                &args[3],
                release.installer_sha1,
                &profile,
                &version,
            )
            .map_err(|e| e.to_string())?;
            if proof.instance_ready || proof.ready_to_execute {
                return Err("metadata inspection incorrectly granted execution or READY".into());
            }
            println!(
                "FORGE_OFFICIAL_RUST_INSPECTION_PASS mc={} loader={} java={} processors={}",
                args[2], args[3], proof.java_major, proof.selected_processor_count
            );
        }
        "neoforge" => {
            let release = tl_loader::loader_matrix::neoforge_candidate(&args[2], &args[3])
                .map_err(|e| e.to_string())?;
            let proof = inspect_reviewed_neoforge(
                &args[2],
                &args[3],
                release.installer_sha1,
                &profile,
                &version,
            )
            .map_err(|e| e.to_string())?;
            if proof.instance_ready || proof.ready_to_execute {
                return Err("metadata inspection incorrectly granted execution or READY".into());
            }
            println!(
                "NEOFORGE_OFFICIAL_RUST_INSPECTION_PASS mc={} loader={} java={} processors={}",
                args[2], args[3], proof.java_major, proof.selected_processors
            );
        }
        _ => return Err("unknown loader kind".into()),
    }
    Ok(())
}
