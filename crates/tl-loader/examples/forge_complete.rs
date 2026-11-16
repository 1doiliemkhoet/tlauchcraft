//! Read-only, no-Java Chat4/Chat3 post-execution receipt bridge for single-run E2E.
//! Takes Chat4's staged nonready manifest, the exact approved prepared Forge
//! processor plan, REAL completed execution evidence and immutable Maven files.
//! Only succeeds after SHA-1 + byte-size verification on local source files.
//! Never moves/publishes files or starts a process.
use serde_json::Value;
use std::{env, fs, fs::OpenOptions, io::Write, path::PathBuf};
use tl_loader::{
    forge_graph::PrepareReport,
    forge_playable::{build_forge_playable_handoff, PackForgeSelection},
    runtime::ProcessorRunReport,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("FORGE_COMPLETION_NOT_VERIFIED: {err}");
        std::process::exit(2);
    }
}

fn read_json(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = env::args().collect();
    if argv.len() != 9 {
        return Err("usage: forge_complete <Chat4 staged pack manifest.json> <official install_profile.json> <official derived_version.json> <official Mojang parent.json> <approved prepare.json> <execution.json> <Maven roots.json> <NEW receipt.json>".into());
    }
    let selection = PackForgeSelection::from_chat4_staged_manifest(&read_json(&argv[1])?)?;
    let profile = fs::read_to_string(&argv[2])?;
    let derived = read_json(&argv[3])?;
    let parent = read_json(&argv[4])?;
    let approved: PrepareReport = serde_json::from_slice(&fs::read(&argv[5])?)?;
    let execution: ProcessorRunReport = serde_json::from_slice(&fs::read(&argv[6])?)?;
    let roots: Vec<PathBuf> = serde_json::from_slice(&fs::read(&argv[7])?)?;
    let request = approved
        .request
        .as_ref()
        .ok_or("approved processor request missing")?;
    let receipt = build_forge_playable_handoff(
        &selection, &profile, &derived, &parent, &approved, request, &execution, &roots,
    )?;
    if !receipt.can_stage_new_instance || receipt.instance_ready {
        return Err("receipt did not satisfy Chat4 staged-but-not-ready contract".into());
    }
    let bytes = serde_json::to_vec_pretty(&receipt)?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&argv[8])?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    println!(
        "FORGE_PROCESSORS_AND_FINAL_JARS_VERIFIED_FOR_CHAT4: minecraft={}, forge={}, steps={}, generated_jars={}, official_libraries={}, instance_ready=false, java_executed_by_this_function=false",
        receipt.minecraft_version,
        receipt.forge_version,
        receipt.verified_processor_count,
        receipt.generated_artifacts.len(),
        receipt.official_libraries.len()
    );
    Ok(())
}
