#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct Capabilities {
    schema_version: u8,
    provider: &'static str,
    features: BTreeMap<&'static str, bool>,
}

#[tauri::command]
fn desktop_capabilities() -> Capabilities {
    // Safe default: no core commands wired until Control approves concrete facade.
    let features = ["profiles", "instances", "versions", "modpacks", "skins", "lan",
        "game", "settings", "downloads"]
        .into_iter()
        .map(|feature| (feature, false))
        .collect();
    Capabilities { schema_version: 1, provider: "shell-only (awaiting Chat 1 facade)", features }
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![desktop_capabilities])
        .run(tauri::generate_context!())
        .expect("failed to run tlauchcraft desktop");
}
