#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use std::collections::BTreeMap;
use tl_core::{CoreError, GameInstance, OfflineProfile, CONTRACT_VERSION};
use tl_storage::{default_data_dir, isolated_data_dir, InstanceStore, ProfileStore, SqliteStore};
use tl_pack::{inspect_archive, PackMetadata};
use tl_pack::curseforge::{configured_api_key, resolve_curseforge_files, CurseForgeResolveReport, OfficialApiClient};
use tl_pack::install::{InstallStatus, StageReport};
use tl_pack::transaction::{stage_transaction, TransactionState};
use std::sync::{Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};
use std::path::Path;
use std::sync::Arc;
mod java_settings;
mod game_ops;

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
        .map(|feature| (feature, feature == "profiles" || feature == "instances" || feature == "pack_inspection" || feature == "settings" || feature == "game"))
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

/// Reconcile only crash-interrupted native Forge publish intents, never
/// trust an arbitrary frontend READY flag or a private staging directory.
#[tauri::command]
async fn instance_list() -> Result<Vec<GameInstance>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let instances=open_store().and_then(|store|store.list_instances())
            .map_err(|e|e.to_string())?;
        let mut recovered=false;
        for item in &instances {
            if item.loader.kind==tl_core::LoaderKind::Forge
                && item.state==tl_core::InstanceState::Verifying{
                match game_ops::reconcile_published_forge(item){
                    Ok(true)=>recovered=true,
                    Ok(false)=>{},
                    Err(err)=>eprintln!("[forge-recovery] instance={} blocked: {}",item.id,err),
                }
            }
        }
        if recovered {
            open_store().and_then(|store|store.list_instances()).map_err(|e|e.to_string())
        } else {
            Ok(instances)
        }
    }).await.map_err(|e|format!("instance listing worker failed: {e}"))?
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

#[tauri::command]
async fn java_settings_get(instance_id: String) -> Result<java_settings::JavaSettings,String> {
    tauri::async_runtime::spawn_blocking(move || java_settings::read(&instance_id)).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn java_settings_save(instance_id:String,xms_mb:u32,xmx_mb:u32,java_path:String,expected_revision:i64)->Result<java_settings::JavaSettings,String>{
    tauri::async_runtime::spawn_blocking(move || java_settings::save(&instance_id,xms_mb,xmx_mb,java_path,expected_revision)).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn java_memory_info() -> Result<java_settings::MemoryInfo,String>{
    tauri::async_runtime::spawn_blocking(java_settings::system_memory).await.map_err(|e|e.to_string())?
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
        // A durable journal is written BEFORE any network or extraction I/O.
        // Re-running the same ID is refused and recovery never treats a
        // previous private stage as a verified playable installation.
        let staged = stage_transaction(&root, &id, archive, &metadata, &resolved, &cancel)?;
        let status = match staged.journal.state {
            TransactionState::Cancelled => InstallStatus::Cancelled,
            TransactionState::Failed => InstallStatus::Failed,
            _ => InstallStatus::NeedsAction,
        };
        let phase = match status {
            InstallStatus::Cancelled => "CANCELLED",
            InstallStatus::Failed => "FAILED",
            InstallStatus::NeedsAction => "NEEDS_ACTION",
        };
        emit_pack_progress(&app, &id, phase, "pack.awaiting_verified_forge", None);
        Ok(StageReport {
            status,
            installed: staged.installed,
            blocked: staged.blocked,
            failures: staged.failures,
            staging_path: staged.staging_path.display().to_string(),
            ready: false,
        })
    }).await;
    if let Ok(mut jobs) = pack_jobs().lock() {
        jobs.remove(&operation_id);
    }
    worker.map_err(|_| "pack staging task failed".to_string())?
}

/// Verify and import a user-selected restricted mod into an existing private
/// staging operation. Official API provenance is fetched again in the backend;
/// UI-supplied hashes/file metadata are NEVER trusted.
#[tauri::command]
async fn pack_import_restricted(
    archive_path: String,
    operation_id: String,
    project_id: u64,
    file_id: u64,
    local_path: String,
) -> Result<tl_pack::install::InstalledMod, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if operation_id.len() < 8 || operation_id.len() > 64
            || !operation_id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err("invalid staging operation ID".into());
        }
        let archive = Path::new(&archive_path);
        if !archive.is_absolute() || !archive.is_file() {
            return Err("source pack ZIP must be an existing absolute file".into());
        }
        let manifest = inspect_archive(archive).map_err(|e| e.to_string())?;
        if !manifest.files.iter().any(|f| {
            f.project_id == Some(project_id) && f.file_id == Some(file_id)
        }) {
            return Err("restricted mod does not belong to original pack manifest".into());
        }
        let key = configured_api_key().map_err(|e| e.to_string())?;
        let plan = resolve_curseforge_files(&manifest, &OfficialApiClient, &key)
            .map_err(|e| e.to_string())?;
        let approved = plan.files.iter().find(|f| f.project_id == project_id && f.file_id == file_id)
            .ok_or("official API did not confirm this project/file pair")?;
        if approved.availability == tl_pack::curseforge::FileAvailability::Available {
            return Err("file is available through approved CDN; manual import not necessary".into());
        }
        let root = isolated_data_dir(
            &default_data_dir().map_err(|e| e.to_string())?.join("instances"),
        ).map_err(|e| e.to_string())?;
        let stage = root.join(format!("{operation_id}.staging"));
        tl_pack::manual_import::import_restricted_mod(
            Path::new(&local_path), &stage, approved, &AtomicBool::new(false),
        )
    }).await.map_err(|_| "restricted import worker failed".to_string())?
}

/// Independently re-hash every staged mod using freshly resolved official
/// CurseForge metadata. The result never claims loader or instance READY.
#[tauri::command]
async fn pack_staged_receipt(
    archive_path: String,
    operation_id: String,
) -> Result<tl_pack::staged_receipt::StagedPackReceipt, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if operation_id.len() < 8 || operation_id.len() > 64
            || !operation_id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err("invalid pack operation ID".into());
        }
        let archive = Path::new(&archive_path);
        if !archive.is_absolute() || !archive.is_file() {
            return Err("source archive must be an existing absolute file".into());
        }
        let manifest = inspect_archive(archive).map_err(|e| e.to_string())?;
        let key = configured_api_key().map_err(|e| e.to_string())?;
        let approved = resolve_curseforge_files(&manifest, &OfficialApiClient, &key)
            .map_err(|e| e.to_string())?;
        let root = isolated_data_dir(
            &default_data_dir().map_err(|e| e.to_string())?.join("instances"),
        ).map_err(|e| e.to_string())?;
        tl_pack::staged_receipt::audit_curseforge_staging(
            &root.join(format!("{operation_id}.staging")),
            &manifest,
            &approved,
        )
    }).await.map_err(|_| "pack verification worker failed".to_string())?
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
        .manage(Arc::new(game_ops::GameState::default()))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            desktop_capabilities,
            profile_list,
            profile_create,
            instance_list,
            pack_inspect,
            pack_resolve,
            pack_stage_curseforge,
            pack_import_restricted,
            pack_staged_receipt,
            pack_cancel,
            java_settings_get,
            java_settings_save,
            java_memory_info,
            game_ops::instance_create_vanilla,
            game_ops::game_install,
            game_ops::game_preflight,
            game_ops::game_cancel,
            game_ops::game_status,
            game_ops::game_launch,
            game_ops::game_stop
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tlauchcraft desktop");
}
