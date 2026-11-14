#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use std::collections::BTreeMap;
use tl_core::{CoreError, GameInstance, OfflineProfile, CONTRACT_VERSION};
use tl_storage::{default_data_dir, isolated_data_dir, InstanceStore, ProfileStore, SqliteStore};
use tl_pack::{inspect_archive, PackMetadata};
use tl_pack::curseforge::{configured_api_key, resolve_curseforge_files, CurseForgeResolveReport, OfficialApiClient};
use tl_pack::install::{stage_curseforge_downloads, InstallStatus, StageReport};
use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};

use std::path::Path;

#[derive(Serialize)]
struct Capabilities {
    schema_version: u32,
    provider: &'static str,
    features: BTreeMap<&'static str, bool>,
}

#[tauri::command]
fn desktop_capabilities() -> Capabilities {
    // Only advertise registered and persistently backed operations.
    let features = ["profiles", "instances", "versions", "modpacks", "pack_inspection", "skins", "lan",
        "game", "settings", "downloads"]
        .into_iter()
        .map(|feature| (feature, feature == "profiles" || feature == "instances" || feature == "pack_inspection"))
        .collect();
    Capabilities { schema_version: CONTRACT_VERSION, provider: "tl-core + tl-storage", features }
}

fn open_store() -> Result<SqliteStore, CoreError> {
    SqliteStore::open(&default_data_dir()?)
}

// Typed Rust payloads come from Foundation, not duplicated in desktop.
#[tauri::command]
fn profile_list() -> Result<Vec<OfflineProfile>, String> {
    open_store().and_then(|store| store.list_profiles()).map_err(|e| e.to_string())
}

#[tauri::command]
fn profile_create(username: String) -> Result<OfflineProfile, String> {
    let profile = OfflineProfile::new(username).map_err(|e| e.to_string())?;
    let mut store = open_store().map_err(|e| e.to_string())?;
    store.create_profile(&profile).map_err(|e| e.to_string())?;
    Ok(profile)
}

#[tauri::command]
fn instance_list() -> Result<Vec<GameInstance>, String> {
    open_store().and_then(|store| store.list_instances()).map_err(|e| e.to_string())
}

/// Inspection is read-only; never extracts, installs, or launches modpack payloads.
#[tauri::command]
fn pack_inspect(archive_path: String) -> Result<PackMetadata, String> {
    let path = Path::new(&archive_path);
    if !path.is_absolute() || !path.is_file() {
        return Err("archive must be an existing absolute file".into());
    }
    inspect_archive(path).map_err(|e| e.to_string())
}

/// Resolve official CurseForge file metadata, but NEVER claim installation or READY.
/// Runs off the desktop UI thread and keeps the API credential out of frontend args.
#[tauri::command]
async fn pack_resolve(archive_path: String) -> Result<CurseForgeResolveReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let archive = Path::new(&archive_path);
        if !archive.is_absolute() || !archive.is_file() {
            return Err("archive must be an existing absolute file".to_string());
        }
        let metadata = inspect_archive(archive).map_err(|e| e.to_string())?;
        let api_key = configured_api_key().map_err(|e| e.to_string())?;
        resolve_curseforge_files(&metadata, &OfficialApiClient, &api_key)
            .map_err(|e| e.to_string())
    }).await.map_err(|_| "CurseForge resolver task failed".to_string())?
}

static PACK_JOBS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn pack_jobs() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    PACK_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Serialize, Clone)]
struct PackProgress {
    contract_version: u32,
    operation_id: String,
    instance_id: Option<String>,
    timestamp: String,
    phase: String,
    message_key: String,
    details: BTreeMap<String, String>,
    percent: Option<f32>,
}

fn emit_pack_progress(app: &AppHandle, id: &str, phase: &str, message: &str, percent: Option<f32>) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();
    let _ = app.emit("download.progress", PackProgress {
        contract_version: CONTRACT_VERSION,
        operation_id: id.to_owned(),
        instance_id: None,
        timestamp,
        phase: phase.to_owned(),
        message_key: message.to_owned(),
        details: BTreeMap::new(),
        percent,
    });
}

/// Stage explicitly authorized CurseForge downloads in an isolated new folder.
/// This is NOT instance publication; Forge installation and READY remain blocked.
#[tauri::command]
async fn pack_stage_curseforge(
    app: AppHandle,
    archive_path: String,
    operation_id: String,
) -> Result<StageReport, String> {
    if operation_id.len() < 8 || operation_id.len() > 64
        || !operation_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("invalid operation id".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut jobs = pack_jobs().lock().map_err(|_| "job registry unavailable")?;
        if jobs.contains_key(&operation_id) {
            return Err("operation is already running".into());
        }
        jobs.insert(operation_id.clone(), cancel.clone());
    }
    let id = operation_id.clone();
    let worker = tauri::async_runtime::spawn_blocking(move || -> Result<StageReport, String> {
        let archive = Path::new(&archive_path);
        if !archive.is_absolute() || !archive.is_file() {
            return Err("archive must be an existing absolute file".into());
        }
        emit_pack_progress(&app, &id, "RESOLVING", "pack.api_resolve", None);
        let metadata = inspect_archive(archive).map_err(|e| e.to_string())?;
        let key = configured_api_key().map_err(|e| e.to_string())?;
        let resolved = resolve_curseforge_files(&metadata, &OfficialApiClient, &key)
            .map_err(|e| e.to_string())?;
        if cancel.load(Ordering::Relaxed) {
            return Ok(StageReport {
                status: InstallStatus::Cancelled,
                installed: Vec::new(), blocked: Vec::new(), failures: Vec::new(),
                staging_path: String::new(), ready: false,
            });
        }
        emit_pack_progress(&app, &id, "DOWNLOADING", "pack.staging", None);
        let root = isolated_data_dir(
            &default_data_dir().map_err(|e| e.to_string())?.join("instances"),
        ).map_err(|e| e.to_string())?;
        let stage = root.join(format!("{id}.staging"));
        let report = stage_curseforge_downloads(
            archive, &metadata, &resolved, &stage, &cancel,
        ).map_err(|e| e.to_string())?;
        emit_pack_progress(&app, &id, "NEEDS_ACTION", "pack.not_ready", Some(100.0));
        Ok(report)
    }).await;
    if let Ok(mut jobs) = pack_jobs().lock() {
        jobs.remove(&operation_id);
    }
    worker.map_err(|_| "pack staging task failed".to_string())?
}

/// Cancel a running staging transfer without deleting unrelated user data.
#[tauri::command]
fn pack_cancel(operation_id: String) -> Result<(), String> {
    let jobs = pack_jobs().lock().map_err(|_| "job registry unavailable")?;
    let token = jobs.get(&operation_id).ok_or("operation not running")?;
    token.store(true, Ordering::Relaxed);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            desktop_capabilities,
            profile_list,
            profile_create,
            instance_list,
            pack_inspect,
            pack_resolve,
            pack_stage_curseforge,
            pack_cancel
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tlauchcraft desktop");
}
