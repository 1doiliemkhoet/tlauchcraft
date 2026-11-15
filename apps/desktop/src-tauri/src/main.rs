#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use std::collections::BTreeMap;
use tl_core::{CoreError, GameInstance, OfflineProfile, CONTRACT_VERSION};
use tl_storage::{default_data_dir, InstanceStore, ProfileStore, SqliteStore};
use tl_pack::{inspect_archive, PackMetadata};
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
