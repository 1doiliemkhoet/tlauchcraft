#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use std::collections::BTreeMap;
use tl_core::{CoreError, GameInstance, OfflineProfile, CONTRACT_VERSION};
use tl_storage::{default_data_dir, InstanceStore, ProfileStore, SqliteStore};
use tl_pack::{inspect_archive, PackMetadata};
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            desktop_capabilities,
            profile_list,
            profile_create,
            instance_list,
            pack_inspect
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tlauchcraft desktop");
}
