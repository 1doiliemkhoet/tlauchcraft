//! Run from Windows after adding an approved local CURSEFORGE_API_KEY:
//! cargo run --locked -p tl-pack --example rustic_waters_e2e --
//!   "C:\\path\\Rustic-Waters-II.zip" "C:\\path\\private-stages" scan-job-001 [--stage-approved]
//!
//! This is a diagnostic harness, NOT proof of playability. It never launches Java,
//! runs Forge processors, publishes an instance or logs the API key.
use serde_json::json;
use std::{env, path::PathBuf, sync::atomic::AtomicBool};
use tl_pack::{
    curseforge::{configured_api_key, resolve_curseforge_files, OfficialApiClient},
    inspect_archive,
    transaction::{stage_transaction, TransactionState},
    PackSource,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("E2E harness: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 && args.len() != 5 {
        return Err(
            "usage: rustic_waters_e2e <archive.zip> <private-stage-root> <operation-id> [--stage-approved]".into(),
        );
    }
    let enable_staging = args.len() == 5 && args[4] == "--stage-approved";
    if args.len() == 5 && !enable_staging {
        return Err("unrecognized option; only --stage-approved is supported".into());
    }
    let archive = PathBuf::from(&args[1]);
    let private_root = PathBuf::from(&args[2]);
    let operation_id = &args[3];
    let metadata = inspect_archive(&archive).map_err(|_| "archive inspection failed")?;
    if metadata.source != PackSource::CurseforgeZip {
        return Err("expected a CurseForge ZIP".into());
    }
    // This harness is specifically for the Control-verified Rustic Waters II
    // 1.16 export. Refuse unrelated packs or a drifted Forge build.
    if metadata.game_version != "1.16.5"
        || metadata.loader.kind != "forge"
        || metadata.loader.version.as_deref() != Some("36.2.29")
        || metadata.files.len() != 200
    {
        return Err(
            "Rustic Waters II contract mismatch: expected Minecraft 1.16.5, Forge 36.2.29 and 200 manifest file references".into(),
        );
    }
    let required = metadata.files.iter().filter(|f| f.required).count();
    let optional = metadata.files.len() - required;
    let key = match configured_api_key() {
        Ok(value) => value,
        Err(_) => {
            println!(
                "{}",
                json!({
                    "mode": "inspect_only",
                    "archive_sha256": metadata.archive_sha256,
                    "pack_name": metadata.name,
                    "minecraft": metadata.game_version,
                    "loader": metadata.loader,
                    "total": metadata.files.len(),
                    "required": required,
                    "optional": optional,
                    "verified_downloads": 0,
                    "blocked": metadata.files.len(),
                    "blocked_items": metadata.files.iter().map(|f|json!({
                        "project_id":f.project_id,
                        "file_id":f.file_id,
                        "required":f.required,
                        "reason":"CurseForge API credentials unavailable; legal download unresolved"
                    })).collect::<Vec<_>>(),
                    "ready": false,
                    "status": "NEEDS_ACTION"
                })
            );
            return Ok(());
        }
    };
    let resolved = resolve_curseforge_files(&metadata, &OfficialApiClient, &key)
        .map_err(|_| "official CurseForge dependency resolution failed")?;
    let unresolved: Vec<_> = resolved
        .files
        .iter()
        .filter(|f| f.availability != tl_pack::curseforge::FileAvailability::Available)
        .map(|f| {
            json!({
                "project_id":f.project_id, "file_id":f.file_id,
                "required":f.required, "reason":f.reason,
                "availability":f.availability
            })
        })
        .collect();
    let dependency_actions: Vec<_> = resolved
        .dependency_actions
        .iter()
        .map(|d| {
            json!({
                "parent_project_id":d.parent_project_id,
                "parent_file_id":d.parent_file_id,
                "dependency_project_id":d.project_id,
                "required":d.required,
                "reason":d.reason
            })
        })
        .collect();
    let mut installed = Vec::new();
    let mut failures = Vec::new();
    let mut state = TransactionState::NeedsAction;
    if enable_staging {
        let report = stage_transaction(
            &private_root,
            operation_id,
            &archive,
            &metadata,
            &resolved,
            &AtomicBool::new(false),
        )?;
        state = report.journal.state;
        installed = report
            .installed
            .iter()
            .map(|m| {
                json!({
                    "project_id":m.project_id,
                    "file_id":m.file_id,
                    "path":m.relative_path,
                    "sha256":m.actual_sha256,
                    "size_bytes":m.bytes
                })
            })
            .collect();
        failures = report.failures;
    }
    println!(
        "{}",
        json!({
            "mode": if enable_staging { "private_stage" } else { "resolve_only" },
            "pack_name": metadata.name,
            "archive_sha256": metadata.archive_sha256,
            "minecraft": metadata.game_version,
            "loader": metadata.loader,
            "archive_entries": metadata.archive_entries,
            "total": metadata.files.len(),
            "required":required,
            "optional":optional,
            "resolved_official":resolved.resolved_count,
            "blocked_file_count": unresolved.len(),
            "blocked_items": unresolved,
            "dependency_actions":dependency_actions,
            "required_dependency_actions": resolved.dependency_actions.iter().filter(|d|d.required).count(),
            "optional_dependency_actions": resolved.dependency_actions.iter().filter(|d|!d.required).count(),
            "verified_downloads": installed.len(),
            "installed_files": installed,
            "failed_downloads": failures.len(),
            "failures": failures,
            "state": state,
            "forge_runtime_verified":false,
            "ready":false,
            "G5":"NOT_VERIFIED"
        })
    );
    Ok(())
}
