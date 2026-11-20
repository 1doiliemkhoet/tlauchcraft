//! Offline Forge P0 receipt exporter for Chat 4. Never invokes Java,
//! runs Forge processors, downloads files or modifies existing instances.
//! Usage: cargo run --locked -p tl-loader --example forge_runtime_receipt --
//!        <new-input.json> <new-receipt.json>
//! Input is a versioned bundle of Control-approved processor execution evidence
//! and official installer/Mojang metadata.
use serde::Deserialize;
use serde_json::Value;
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use tl_loader::{
    forge_playable::{
        verify_forge_runtime_for_pack, PackForgeSelection, VerifiedForgeJava, VerifiedForgeLibrary,
    },
    runtime::{ProcessorRunReport, ProcessorRunRequest, VerifiedProcessorJar},
};

#[derive(Debug, Deserialize)]
struct ReceiptInput {
    schema: String,
    normalized_pack: Value,
    install_profile: Value,
    derived_version: Value,
    mojang_parent: Value,
    processor_request: ProcessorRunRequest,
    actual_execution: ProcessorRunReport,
    java: VerifiedForgeJava,
    official_maven: Vec<VerifiedProcessorJar>,
    installer_or_generated: Vec<VerifiedForgeLibrary>,
}

fn main() {
    if let Err(message) = perform() {
        eprintln!("FORGE_RUNTIME_RECEIPT_DENIED: {message}");
        std::process::exit(2);
    }
}

fn perform() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || !Path::new(&args[1]).is_file() {
        return Err("usage: forge_runtime_receipt <input.json> <new-receipt.json>".into());
    }
    let output = PathBuf::from(&args[2]);
    if output.exists() || output == PathBuf::from(&args[1]) || !output.is_absolute() {
        return Err("output path must be new and absolute; never overwrite evidence".into());
    }
    let input: ReceiptInput = serde_json::from_slice(&fs::read(&args[1])?)?;
    if input.schema != "tl-forge-runtime-receipt-input/v1" {
        return Err("unrecognized Forge runtime receipt evidence schema".into());
    }
    let selected = PackForgeSelection::from_normalized_pack_json(&input.normalized_pack)?;
    let profile = serde_json::to_string(&input.install_profile)?;
    let receipt = verify_forge_runtime_for_pack(
        &selected,
        &profile,
        &input.derived_version,
        &input.mojang_parent,
        &input.processor_request,
        &input.actual_execution,
        &input.java,
        &input.official_maven,
        &input.installer_or_generated,
    )?;
    if !receipt.execution_verified || receipt.instance_ready || receipt.game_launched {
        return Err("internal execution/instance/game status invariant violated".into());
    }
    let bytes = serde_json::to_vec_pretty(&receipt)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Forge handoff receipt exceeds 8 MiB".into());
    }
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    destination.write_all(&bytes)?;
    destination.sync_all()?;
    println!(
        "FORGE_PROCESSOR_OUTPUTS_VERIFIED_FOR_CHAT4: {} ({} libraries, {} processors, instance_ready=false)",
        output.display(),
        receipt.libraries.len(),
        receipt.processor_count
    );
    Ok(())
}
