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
/// Reports only key presence to the UI. Never returns the value, does not
/// validate permissions, and never logs or persists the credential.
#[tauri::command]
fn pack_api_key_configured() -> bool {
    configured_api_key().is_ok()
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


/// User-opted-in official Forge install: only a release-pinned adapter may run
/// Java on the Windows host. Job Object is NOT a file/network sandbox.
#[tauri::command]
async fn pack_install_official_forge(
    app: AppHandle,
    archive_path: String,
    operation_id: String,
    display_name: String,
    profile_id: String,
    java_path: String,
    host_execution_consent: bool,
) -> Result<GameInstance, String> {
    if !host_execution_consent {
        return Err("Explicit approval is required for official Forge Java host execution".into());
    }
    if operation_id.len() < 8
        || operation_id.len() > 64
        || !operation_id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid staged pack operation ID".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut jobs = pack_jobs().lock().map_err(|_| "pack operation registry unavailable")?;
        if jobs.contains_key(&operation_id) {
            return Err("this staged operation is already active".into());
        }
        jobs.insert(operation_id.clone(), cancel.clone());
    }
    let key = operation_id.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || -> Result<GameInstance, String> {
        #[cfg(not(windows))]
        {
            let _ = (app, archive_path, display_name, profile_id, java_path, cancel);
            Err("Official Forge host execution is only supported by the audited Windows adapter".into())
        }
        #[cfg(windows)]
        {
            let archive = Path::new(&archive_path);
            let java = Path::new(&java_path);
            if !archive.is_absolute() || !archive.is_file()
                || !java.is_absolute() || !java.is_file()
            {
                return Err("modpack archive and Java executable must be existing absolute files".into());
            }
            let profile_uuid = uuid::Uuid::parse_str(&profile_id)
                .map_err(|_| "Invalid offline profile UUID")?;
            let metadata = inspect_archive(archive).map_err(|e| e.to_string())?;
            if metadata.source != tl_pack::PackSource::CurseforgeZip
                || metadata.loader.kind != "forge"
            {
                return Err("Official Forge install requires a CurseForge Forge pack".into());
            }
            let loader_version = metadata.loader.version.as_deref()
                .ok_or("Original pack manifest has no selected Forge version")?;
            if metadata.game_version != "1.16.5" || loader_version != "36.2.29" {
                return Err(format!(
                    "No approved Windows host processor adapter for Minecraft {} / Forge {}; keep this pack staged",
                    metadata.game_version, loader_version
                ));
            }
            let base = default_data_dir().map_err(|e| e.to_string())?;
            let root = isolated_data_dir(&base.join("instances")).map_err(|e| e.to_string())?;
            let stage = root.join(format!("{}.staging", key));
            if !stage.is_dir() {
                return Err("Private pack staging missing; stage and rehash mods first".into());
            }
            emit_pack_progress(&app, &key, "VERIFYING", "pack.rehash_official_mods", None);
            let api_key = configured_api_key().map_err(|e| e.to_string())?;
            let resolved = resolve_curseforge_files(&metadata, &OfficialApiClient, &api_key)
                .map_err(|e| e.to_string())?;
            let receipt = tl_pack::staged_receipt::audit_curseforge_staging(
                &stage, &metadata, &resolved
            )?;
            if !receipt.complete_mod_inventory
                || !receipt.blocked_optional.is_empty()
                || !receipt.blocked_required.is_empty()
                || !receipt.failures.is_empty()
                || receipt.verified.len() != metadata.files.len()
            {
                return Err("Staged CurseForge mod inventory is incomplete; cannot execute Forge".into());
            }
            if cancel.load(Ordering::SeqCst) {
                return Err("Forge install cancelled before host processor execution".into());
            }
            emit_pack_progress(&app, &key, "VERIFYING", "forge.pinned_mojang_maven", None);
            let prepared = tl_pack::forge_official::prepare_official_windows_1165(
                &metadata.archive_sha256,
                &metadata.game_version,
                loader_version,
                &key,
                &base,
                java,
                host_execution_consent,
                &cancel,
            )?;
            if cancel.load(Ordering::SeqCst) {
                return Err("Forge install cancelled before private instance publication".into());
            }
            emit_pack_progress(&app, &key, "VERIFYING", "forge.processor_output_verified", None);
            let installed = game_ops::finalize_approved_forge_instance(
                display_name, profile_uuid, archive, &stage, &resolved,
                prepared.approved(), &cancel,
            )?;
            // Persist exactly the reviewed Java executable for subsequent Play.
            let instance_id = installed.id.to_string();
            let settings = java_settings::read(&instance_id)?;
            java_settings::save(
                &instance_id,
                settings.xms_mb,
                settings.xmx_mb,
                java_path,
                settings.revision,
            ).map_err(|e| format!(
                "Forge published and verified, but Java selection must be fixed in Settings: {e}"
            ))?;
            emit_pack_progress(&app, &key, "INSTALLED", "forge.ready_after_independent_audit", None);
            Ok(installed)
        }
    }).await;
    if let Ok(mut jobs) = pack_jobs().lock() {
        jobs.remove(&operation_id);
    }
    outcome.map_err(|e| format!("Forge installer worker panicked or was interrupted: {e}"))?
}


/// Import ALREADY installed CurseForge profile (no API, no CDN requests).
/// Reconciles original ZIP manifest project/file IDs with minecraftinstance.json;
/// copies only matching local JARs/ZIP overrides into a new private stage.
/// Local JAR hashes are observed only, NOT upstream provider attestations.
/// The source CurseForge profile, including saves, is never changed.
#[tauri::command]
async fn pack_import_installed_curseforge(
    app: AppHandle,
    archive_path: String,
    installed_profile_path: String,
    operation_id: String,
    profile_id: String,
    java_path: String,
    host_execution_consent: bool,
) -> Result<GameInstance, String> {
    if !host_execution_consent {
        return Err("Explicit opt-in required to execute official Forge Java on Windows".into());
    }
    if operation_id.len() < 8 || operation_id.len() > 64
        || !operation_id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid local-import operation ID".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut jobs = pack_jobs().lock().map_err(|_| "pack registry unavailable")?;
        if jobs.contains_key(&operation_id) {
            return Err("Local import operation is already active".into());
        }
        jobs.insert(operation_id.clone(), cancel.clone());
    }
    let id = operation_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<GameInstance, String> {
        #[cfg(not(windows))]
        {
            let _ = (app, archive_path, installed_profile_path, profile_id, java_path, cancel);
            Err("Local Forge profile installer currently supports Windows only".into())
        }
        #[cfg(windows)]
        {
            let archive = Path::new(&archive_path);
            let profile = Path::new(&installed_profile_path);
            let java = Path::new(&java_path);
            if !archive.is_absolute() || !archive.is_file()
                || !profile.is_absolute() || !profile.is_dir()
                || !java.is_absolute() || !java.is_file()
                || !java.extension().and_then(|x| x.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
            {
                return Err("ZIP, installed CurseForge profile folder, and Java .exe must exist as absolute paths".into());
            }
            let offline_profile = uuid::Uuid::parse_str(&profile_id)
                .map_err(|_| "Invalid offline profile UUID")?;
            let metadata = inspect_archive(archive).map_err(|e| e.to_string())?;
            if metadata.source != tl_pack::PackSource::CurseforgeZip
                || metadata.loader.kind != "forge"
            {
                return Err("Import requires a CurseForge ZIP with Forge loader metadata".into());
            }
            let forge_version = metadata.loader.version.as_deref()
                .ok_or("Original manifest lacks Forge version")?;
            if metadata.game_version != "1.16.5" || forge_version != "36.2.29" {
                return Err(format!(
                    "Installed mod import supports this manifest, but official Forge processor execution for {} / {} has not been audited yet",
                    metadata.game_version, forge_version
                ));
            }
            let mut probe = std::process::Command::new(java);
            probe.arg("-version");
            use std::os::windows::process::CommandExt;
            probe.creation_flags(0x0800_0000);
            let output = probe.output().map_err(|e| format!("Java probe failed: {e}"))?;
            if !output.status.success() {
                return Err("Selected Java executable failed -version".into());
            }
            let version_text = format!("{}{}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            );
            if tl_game::parse_java_major(&version_text).map_err(|e| e.to_string())? != 8 {
                return Err("Minecraft 1.16.5 Forge adapter requires Java 8".into());
            }
            let base = default_data_dir().map_err(|e| e.to_string())?;
            let instance_root = isolated_data_dir(&base.join("instances"))
                .map_err(|e| e.to_string())?;
            let stage = instance_root.join(format!("{id}.staging"));
            emit_pack_progress(&app, &id, "VERIFYING", "local_import.manifest_ids", None);
            let mods = tl_pack::installed_import::stage_installed_curseforge(
                archive, profile, &stage, &cancel,
            )?;
            if cancel.load(Ordering::SeqCst) {
                return Err("Cancelled before executing official Forge".into());
            }
            emit_pack_progress(&app, &id, "VERIFYING", "local_import.copied_mods_hashed", None);
            // Only the official Forge bootstrap downloads its canonical Mojang
            // and Maven inputs; NO CurseForge API or mod CDN request occurs.
            let approved = tl_pack::forge_official::prepare_official_windows_1165(
                &metadata.archive_sha256,
                &metadata.game_version,
                forge_version,
                &id,
                &base,
                java,
                host_execution_consent,
                &cancel,
            )?;
            if cancel.load(Ordering::SeqCst) {
                return Err("Cancelled before Forge publication".into());
            }
            emit_pack_progress(&app, &id, "VERIFYING", "local_import.publishing_verified_forge", None);
            let installed = game_ops::finalize_approved_forge_local_instance(
                metadata.name.clone(), offline_profile, archive, &stage, &mods,
                approved.approved(), &cancel,
            )?;
            // After publishing, persist the Java 8 binary for the Play button.
            let settings = java_settings::read(&installed.id.to_string())?;
            java_settings::save(
                &installed.id.to_string(), settings.xms_mb,
                settings.xmx_mb, java_path, settings.revision,
            ).map_err(|e| format!(
                "Local profile published; configure Java 8 in Settings before Play: {e}"
            ))?;
            emit_pack_progress(&app, &id, "INSTALLED", "local_import.ready_local_hashes_only", None);
            Ok(installed)
        }
    }).await;
    if let Ok(mut jobs) = pack_jobs().lock() {
        jobs.remove(&operation_id);
    }
    result.map_err(|e| format!("Local CurseForge import task failed: {e}"))?
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
            pack_api_key_configured,
            pack_resolve,
            pack_stage_curseforge,
            pack_import_restricted,
            pack_staged_receipt,
            pack_install_official_forge,
            pack_import_installed_curseforge,
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
