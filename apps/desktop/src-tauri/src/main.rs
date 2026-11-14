#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use std::collections::BTreeMap;
use tl_core::{CoreError, GameInstance, OfflineProfile, CONTRACT_VERSION};
use tl_storage::{default_data_dir, InstanceStore, ProfileStore, SqliteStore};
use tl_pack::{inspect_archive, PackMetadata};
use tl_pack::curseforge::{configured_api_key, resolve_curseforge_files, CurseForgeResolveReport, OfficialApiClient};
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            desktop_capabilities,
            profile_list,
            profile_create,
            instance_list,
            pack_inspect,
            pack_resolve
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tlauchcraft desktop");
}
