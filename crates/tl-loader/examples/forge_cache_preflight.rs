//! Read-only CLI for verified Forge cache preflight on Control Windows.
//! Usage: cargo run --locked -p tl-loader --example forge_cache_preflight -- <profile.json> <version.json> <config.json> <report-new.json>
//! No Java is launched and no Minecraft instance is modified.
use std::{env,fs,fs::OpenOptions,io::Write,path::Path};
use tl_loader::forge_evidence_cache::{preflight_official_forge, ForgeCacheImport};
fn main() {
    if let Err(e) = perform() {
        eprintln!("FORGE_CACHE_PREFLIGHT_FAILED: {e}");
        std::process::exit(2);
    }
}
fn perform() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 5 { return Err("expected profile.json version.json config.json new-report.json".into()); }
    for path in &args[1..4] {
        if !Path::new(path).is_file() { return Err(format!("missing input: {path}").into()); }
    }
    let profile = fs::read_to_string(&args[1])?;
    let derived = serde_json::from_str(&fs::read_to_string(&args[2])?)?;
    let options: ForgeCacheImport = serde_json::from_str(&fs::read_to_string(&args[3])?)?;
    let (import, prepare) = preflight_official_forge(&profile, &derived, &options);
    let result = serde_json::json!({
        "status": "READ_ONLY_NO_JAVA_NOT_INSTALLED",
        "import": import,
        "prepare": {
            "state": prepare.state,
            "unresolved": prepare.unresolved,
            "error": prepare.error,
            "processor_plan_count": prepare.request.as_ref().map_or(0, |r| r.processors.len()),
            "java_executed": false,
            "instance_ready": false
        },
        "stage_created": options.stage_root.exists(),
    });
    let json = serde_json::to_string_pretty(&result)?;
    let mut report = OpenOptions::new().create_new(true).write(true).open(&args[4])?;
    report.write_all(json.as_bytes())?;
    println!("{json}");
    Ok(())
}
