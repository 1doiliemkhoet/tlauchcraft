//! Import an ALREADY installed CurseForge app profile without API or network.
//! Only links manifest IDs with the local CurseForge minecraftinstance.json.
//! Local SHA-1/SHA-256 snapshots prove copy integrity, NOT upstream authenticity.
//! Never reads or copies saves; never alters the source profile.
use crate::{
    curseforge::safe_mod_file_name, staged_receipt::VerifiedModReceipt, PackMetadata, PackSource,
};
use serde::Deserialize;
use sha1::{Digest as _, Sha1};
use sha2::Sha256;
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const MAX_MOD_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_PROFILE_META_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalAddon {
    #[serde(rename = "addonID", alias = "addonId")]
    addon_id: u64,
    #[serde(default)]
    file_name_on_disk: Option<String>,
    installed_file: Option<LocalInstalledFile>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalInstalledFile {
    id: u64,
    // CurseForge's minecraftinstance.json stores the *actual on-disk* JAR
    // name inside installedFile.fileNameOnDisk. fileName is not guaranteed
    // to be present and must only be a compatibility fallback.
    #[serde(default, alias = "FileNameOnDisk")]
    file_name_on_disk: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalProfile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    game_version: Option<String>,
    #[serde(default)]
    installed_addons: Vec<LocalAddon>,
    #[serde(default)]
    base_mod_loader: Option<LocalLoader>,
}
#[derive(Debug, Clone, Deserialize)]
struct LocalLoader {
    name: String,
}

fn no_symlink_ancestors(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Import path must be absolute".into());
    }
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!(
                "Unsafe link/reparse point inside imported profile or private staging at: {}",
                ancestor.display()
            ));
        }
    }
    Ok(())
}

type MatchedMod = (u64, u64, bool, String);

fn local_disk_name(addon: &LocalAddon) -> Result<&str, String> {
    let installed = addon.installed_file.as_ref().ok_or_else(|| {
        format!(
            "CurseForge addon {} has no installedFile metadata",
            addon.addon_id
        )
    })?;
    installed
        .file_name_on_disk
        .as_deref()
        .filter(|name| !name.is_empty())
        .or(addon
            .file_name_on_disk
            .as_deref()
            .filter(|name| !name.is_empty()))
        .or(installed
            .file_name
            .as_deref()
            .filter(|name| !name.is_empty()))
        .ok_or_else(|| {
            format!(
                "CurseForge addon {} has no installed filename",
                addon.addon_id
            )
        })
}

fn read_profile_metadata(root: &Path) -> Result<LocalProfile, String> {
    let metadata_path = root.join("minecraftinstance.json");
    no_symlink_ancestors(&metadata_path)?;
    let info = fs::symlink_metadata(&metadata_path).map_err(|_| {
        "CurseForge minecraftinstance.json missing; choose the installed instance folder"
    })?;
    if !info.file_type().is_file() || info.len() == 0 || info.len() > MAX_PROFILE_META_BYTES {
        return Err("CurseForge profile metadata missing/oversized".into());
    }
    let raw = fs::read(&metadata_path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&raw).map_err(|e| format!("CurseForge profile metadata invalid: {e}"))
}

fn checked_names(local: &LocalProfile, manifest: &PackMetadata) -> Result<Vec<MatchedMod>, String> {
    if manifest.source != PackSource::CurseforgeZip
        || manifest.loader.kind != "forge"
        || manifest.loader.version.as_deref().is_none_or(str::is_empty)
        || manifest.files.is_empty()
        || manifest.files.len() > 4096
    {
        return Err("A CurseForge Forge ZIP with mod IDs is required for local import".into());
    }
    // CurseForge's own profile metadata is not signed. Use it only for file ID
    // reconciliation and bind actual SHA-1/SHA-256 independently on disk.
    if let Some(loader) = &local.base_mod_loader {
        let version = manifest.loader.version.as_deref().unwrap_or_default();
        if !loader.name.to_ascii_lowercase().contains("forge") || !loader.name.ends_with(version) {
            return Err("Installed CurseForge profile selects a different Forge loader".into());
        }
    }
    if let Some(game_version) = local.game_version.as_deref() {
        if game_version != manifest.game_version {
            return Err(format!(
                "CurseForge profile Minecraft {game_version} does not match selected pack {}",
                manifest.game_version
            ));
        }
    }
    let wanted: HashSet<u64> = manifest.files.iter().filter_map(|f| f.project_id).collect();
    let mut map = BTreeMap::new();
    for (index, addon) in local.installed_addons.iter().enumerate() {
        let id = addon.addon_id;
        if id == 0 {
            return Err(format!(
                "CurseForge installedAddons[{index}] has an invalid project ID (0)"
            ));
        }
        let name = local_disk_name(addon)?;
        // CurseForge installedAddons may include the *modpack project itself*
        // (e.g. 590644: Rustic Waters II 1.16-1.16.zip). It is not a mod JAR;
        // exclude this record only if it is not referenced by manifest.files.
        // Never silently skip an unknown extra JAR.
        if name.to_ascii_lowercase().ends_with(".zip") && !wanted.contains(&id) {
            continue;
        }
        let installed = addon
            .installed_file
            .as_ref()
            .ok_or_else(|| format!("CurseForge addon {id} has no installedFile metadata"))?;
        if installed.id == 0 {
            return Err(format!("CurseForge addon {id} has an invalid file ID (0)"));
        }
        if !safe_mod_file_name(name) {
            return Err(format!(
                "CurseForge addon {id} file {name:?} is not a safe JAR filename"
            ));
        }
        if let Some((previous_file_id, previous_name)) = map.get(&id) {
            return Err(format!(
                "CurseForge installedAddons contains duplicate project ID {id}: file {previous_file_id} ({previous_name}) and file {} ({name})",
                installed.id
            ));
        }
        map.insert(id, (installed.id, name.to_owned()));
    }
    let mut names = HashSet::new();
    let mut matched = Vec::new();
    for file in &manifest.files {
        let id = file.project_id.ok_or("manifest project ID absent")?;
        let file_id = file.file_id.ok_or("manifest file ID absent")?;
        let (installed_id, name) = map
            .remove(&id)
            .ok_or_else(|| format!("missing local CurseForge addon ID {id}"))?;
        if installed_id != file_id {
            return Err(format!(
                "CurseForge local addon {id} uses file {installed_id}, ZIP requires {file_id}"
            ));
        }
        if !names.insert(name.to_ascii_lowercase()) {
            return Err("duplicate local mod filename on Windows".into());
        }
        matched.push((id, file_id, file.required, name));
    }
    if !map.is_empty() {
        return Err("Installed profile has additional addons not declared by ZIP; import exact profile only".into());
    }
    Ok(matched)
}

/// A folder-only import needs no user-provided ZIP or provider API key.
/// We create a temporary local *snapshot* manifest/archive only to reuse
/// the proven Forge staging/publish audit, never to imply upstream trust.
/// It includes only a strict allowlist of pack assets, NOT saves, worlds,
/// logs, user identity, caches or the actual mods (which are rehashed
/// separately by stage_installed_curseforge).
pub fn snapshot_installed_folder(
    installed_profile: &Path,
    private_snapshot: &Path,
    cancelled: &AtomicBool,
) -> Result<PackMetadata, String> {
    snapshot_installed_folder_with_progress(
        installed_profile,
        private_snapshot,
        cancelled,
        |_, _, _| {},
    )
}

/// Report only genuine completed file/directory counts, never estimated bytes or percentages.
/// The callback is invoked on the worker thread, and must not mutate the source profile.
pub fn snapshot_installed_folder_with_progress<F: FnMut(&'static str, usize, usize)>(
    installed_profile: &Path,
    private_snapshot: &Path,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Result<PackMetadata, String> {
    let root = fs::canonicalize(installed_profile)
        .map_err(|e| format!("Could not resolve installed CurseForge profile folder: {e}"))?;
    no_symlink_ancestors(&root)?;
    if !root.is_dir() || !private_snapshot.is_absolute() || private_snapshot.exists() {
        return Err(
            "Installed profile must be a directory and private snapshot must be new".into(),
        );
    }
    no_symlink_ancestors(private_snapshot)?;
    if private_snapshot.starts_with(&root) || root.starts_with(private_snapshot) {
        return Err("Cannot create a private snapshot inside the CurseForge source profile".into());
    }
    let local = read_profile_metadata(&root)?;
    let game = local
        .game_version
        .as_deref()
        .filter(|v| {
            !v.is_empty()
                && v.len() <= 40
                && v.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'_')
        })
        .ok_or("Installed CurseForge profile has invalid/missing gameVersion")?;
    let loader = local
        .base_mod_loader
        .as_ref()
        .ok_or("Installed CurseForge profile lacks baseModLoader.name")?;
    let (loader_kind, loader_version) = loader
        .name
        .split_once('-')
        .ok_or("Invalid baseModLoader.name: expected loader-version")?;
    if loader_kind != "forge"
        || loader_version.is_empty()
        || loader_version.len() > 40
        || !loader_version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
    {
        return Err(format!(
            "Installed profile loader {:?} has no supported Forge version adapter",
            loader.name
        ));
    }
    let name = local
        .name
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| root.file_name().and_then(|v| v.to_str()))
        .ok_or("Installed profile name not found")?;
    if name.len() > 160 || name.chars().any(char::is_control) {
        return Err("Installed profile name is too long/unsafe".into());
    }
    let mod_dir = root.join("mods");
    no_symlink_ancestors(&mod_dir)?;
    if !fs::symlink_metadata(&mod_dir).is_ok_and(|m| m.file_type().is_dir()) {
        return Err("Installed profile mods/ folder missing".into());
    }
    let mut files = Vec::new();
    let mut seen_projects = HashSet::new();
    let mut seen_names = HashSet::new();
    let total_addons = local.installed_addons.len();
    progress("local_import.hashing_mods", 0, total_addons);
    for (index, addon) in local.installed_addons.iter().enumerate() {
        let filename = local_disk_name(addon)?;
        if filename.to_ascii_lowercase().ends_with(".zip") {
            // Pack self-entry is metadata about its original download.
            // The archive itself is neither downloaded nor installed as a JAR.
            continue;
        }
        if addon.addon_id == 0 || !safe_mod_file_name(filename) {
            return Err(format!(
                "CurseForge addon {} contains an unsupported/unsafe mod filename {:?}",
                addon.addon_id, filename
            ));
        }
        let installed = addon
            .installed_file
            .as_ref()
            .ok_or("Installed addon missing file metadata")?;
        if installed.id == 0
            || !seen_projects.insert(addon.addon_id)
            || !seen_names.insert(filename.to_ascii_lowercase())
        {
            return Err(format!(
                "Duplicate/missing installed CurseForge addon/file ID or filename: {} ({filename})",
                addon.addon_id
            ));
        }
        let source = mod_dir.join(filename);
        observed_hashes(&source)?;
        if index % 8 == 0 {
            progress("local_import.hashing_mods", index + 1, total_addons);
        }
        files.push(serde_json::json!({
            "projectID": addon.addon_id,
            "fileID": installed.id,
            "required": true,
        }));
    }
    progress("local_import.hashing_mods", total_addons, total_addons);
    if files.is_empty() || files.len() > 4096 {
        return Err("Installed profile has no supported JAR addons or exceeds limit".into());
    }
    // Preflight the actual local JAR directory, including any manually placed
    // untracked mod. No silent omission and no reliance on CurseForge API.
    for entry in fs::read_dir(&mod_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
            return Err("Installed mods directory contains a link/special entry".into());
        }
        if kind.is_file() {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "Invalid installed mod Unicode filename")?;
            if name.to_ascii_lowercase().ends_with(".jar")
                && !seen_names.contains(&name.to_ascii_lowercase())
            {
                return Err(format!("Untracked JAR in installed profile: {name}"));
            }
        }
    }
    let manifest = serde_json::json!({
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": name,
        "minecraft": {
            "version": game,
            "modLoaders": [{"id": loader.name, "primary": true}]
        },
        "files": files,
        "overrides": "overrides"
    });
    // Only known pack content is copied. The source's saved worlds, user
    // account information, screenshots and runtime logs are always excluded.
    const PACK_ASSET_DIRS: &[&str] = &[
        "config",
        "defaultconfigs",
        "scripts",
        "kubejs",
        "resourcepacks",
        "shaderpacks",
        "patchouli_books",
        "openloader",
        "global_data_packs",
        "global_packs",
        "thingpacks",        // Data-driven custom blocks/items consumed by Data & Thing.
        "journeymap/config", // Pack defaults only; exclude player maps/waypoints.
        "datapacks",
        "assets",
        "resources",
        "crafttweaker",
        "customnpcs",
        "flans",
        "modpack",
        "packmenu",
        "fancymenu",
    ];
    let mut assets = Vec::<(String, PathBuf, u64)>::new();
    let mut seen_paths = HashSet::new();
    let mut total = 0u64;
    progress("local_import.scanning_assets", 0, PACK_ASSET_DIRS.len());
    for (index, folder) in PACK_ASSET_DIRS.iter().enumerate() {
        let dir = root.join(folder);
        if !dir.exists() {
            progress(
                "local_import.scanning_assets",
                index + 1,
                PACK_ASSET_DIRS.len(),
            );
            continue;
        }
        no_symlink_ancestors(&dir)?;
        if !fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_dir()) {
            return Err(format!(
                "Installed pack asset {folder} is not a regular directory"
            ));
        }
        collect_pack_assets(
            &root,
            &dir,
            cancelled,
            &mut assets,
            &mut seen_paths,
            &mut total,
        )?;
        progress(
            "local_import.scanning_assets",
            index + 1,
            PACK_ASSET_DIRS.len(),
        );
    }
    assets.sort_by(|a, b| a.0.cmp(&b.0));
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(private_snapshot)
        .map_err(|e| format!("Private source snapshot failed: {e}"))?;
    let result = (|| -> Result<(), String> {
        let mut writer = ZipWriter::new(output);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer
            .start_file("manifest.json", opts)
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&serde_json::to_vec(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let total_assets = assets.len();
        progress("local_import.copying_assets", 0, total_assets);
        for (index, (relative, source, original_size)) in assets.into_iter().enumerate() {
            if cancelled.load(Ordering::SeqCst) {
                return Err("Cancelled while creating private pack snapshot".into());
            }
            writer
                .start_file(format!("overrides/{relative}"), opts)
                .map_err(|e| e.to_string())?;
            let copied = std::io::copy(
                &mut File::open(&source).map_err(|e| e.to_string())?,
                &mut writer,
            )
            .map_err(|e| e.to_string())?;
            if copied != original_size
                || fs::symlink_metadata(&source)
                    .map_err(|e| e.to_string())?
                    .len()
                    != copied
            {
                return Err(format!("Pack asset changed while copying: {relative}"));
            }
            if index % 8 == 0 {
                progress("local_import.copying_assets", index + 1, total_assets);
            }
        }
        progress("local_import.copying_assets", total_assets, total_assets);
        let output = writer.finish().map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(private_snapshot);
        return Err(error);
    }
    let report = crate::inspect_archive(private_snapshot).map_err(|e| {
        let _ = fs::remove_file(private_snapshot);
        format!("Generated local-only manifest failed validation: {e}")
    })?;
    // Use the same ZIP manifest binding as the old import path, now derived
    // independently from the user's already-installed CurseForge profile.
    if let Err(error) = checked_names(&local, &report) {
        let _ = fs::remove_file(private_snapshot);
        return Err(error);
    }
    Ok(report)
}

/// Collect a whitelist of pack content; preserve relative paths and refuse
/// every linked/special input. Deterministic sorted ZIP evidence is created
/// from these files, with a hard limit before staging.
fn collect_pack_assets(
    root: &Path,
    current: &Path,
    cancelled: &AtomicBool,
    assets: &mut Vec<(String, PathBuf, u64)>,
    seen: &mut HashSet<String>,
    total: &mut u64,
) -> Result<(), String> {
    no_symlink_ancestors(current)?;
    for entry in fs::read_dir(current).map_err(|e| e.to_string())? {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Cancelled while scanning local pack assets".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        no_symlink_ancestors(&path)?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            collect_pack_assets(root, &path, cancelled, assets, seen, total)?;
        } else if kind.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Non-Unicode pack asset path")?
                .replace('\\', "/");
            crate::safe_path(&rel).map_err(|e| e.to_string())?;
            if !seen.insert(rel.to_ascii_lowercase()) {
                return Err(format!("Case-duplicate installed pack asset: {rel}"));
            }
            let size = entry.metadata().map_err(|e| e.to_string())?.len();
            if size > crate::MAX_SINGLE_FILE {
                return Err(format!("Pack asset too large: {rel}"));
            }
            *total = total
                .checked_add(size)
                .ok_or("Pack snapshot size overflow")?;
            if *total > crate::MAX_UNCOMPRESSED || assets.len() >= crate::MAX_FILES - 2 {
                return Err("Installed pack snapshot exceeds archive safety limits".into());
            }
            assets.push((rel, path, size));
        } else {
            return Err("Local pack assets contain a link or special file".into());
        }
    }
    Ok(())
}

/// Hash source bytes; do not accept link/reparse-point mod inputs.
fn observed_hashes(path: &Path) -> Result<(u64, String, String), String> {
    no_symlink_ancestors(path)?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() || meta.len() == 0 || meta.len() > MAX_MOD_BYTES {
        return Err(format!(
            "Invalid local mod file size/type: {}",
            path.display()
        ));
    }
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut seen = 0u64;
    let mut buf = [0u8; 65536];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        seen = seen
            .checked_add(n as u64)
            .ok_or("local mod byte overflow")?;
        if seen > meta.len() {
            return Err("local mod changed during hashing".into());
        }
        sha1.update(&buf[..n]);
        sha256.update(&buf[..n]);
    }
    if seen != meta.len() {
        return Err("local mod changed during hashing".into());
    }
    Ok((
        seen,
        format!("{:x}", sha1.finalize()),
        format!("{:x}", sha256.finalize()),
    ))
}

/// Fail-closed selection validation. May be used for UI preflight before any
/// Forge processor runs; no CurseForge API key is needed.
fn selection(
    archive: &Path,
    installed_profile: &Path,
    staging: &Path,
) -> Result<(PackMetadata, Vec<MatchedMod>), String> {
    no_symlink_ancestors(archive)?;
    no_symlink_ancestors(installed_profile)?;
    no_symlink_ancestors(staging)?;
    if archive.starts_with(installed_profile)
        || installed_profile.starts_with(staging)
        || staging.starts_with(installed_profile)
        || archive == staging
        || !fs::symlink_metadata(installed_profile).is_ok_and(|m| m.file_type().is_dir())
        || fs::symlink_metadata(staging).is_ok()
        || staging
            .file_name()
            .and_then(|v| v.to_str())
            .is_none_or(|v| !v.ends_with(".staging"))
    {
        return Err("Invalid/occupied private stage or installed CurseForge profile folder".into());
    }
    let pack = crate::inspect_archive(archive).map_err(|e| e.to_string())?;
    let local = read_profile_metadata(installed_profile)?;
    let matching = checked_names(&local, &pack)?;
    let mod_dir = installed_profile.join("mods");
    no_symlink_ancestors(&mod_dir)?;
    if !fs::symlink_metadata(&mod_dir).is_ok_and(|m| m.file_type().is_dir()) {
        return Err("Installed CurseForge mods/ directory missing".into());
    }
    let expected: HashSet<String> = matching.iter().map(|x| x.3.to_ascii_lowercase()).collect();
    let mut seen = HashSet::new();
    for entry in fs::read_dir(mod_dir).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| "Invalid Unicode in installed mod filename")?;
        let kind = e.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
            return Err("Installed mod directory contains a link or special entry".into());
        }
        if kind.is_file()
            && name.to_ascii_lowercase().ends_with(".jar")
            && (!expected.contains(&name.to_ascii_lowercase())
                || !seen.insert(name.to_ascii_lowercase()))
        {
            return Err(format!("Unmatched/additional local mod JAR: {name}"));
        }
    }
    if seen != expected {
        return Err("Local profile is missing one or more ZIP-declared mod JARs".into());
    }
    Ok((pack, matching))
}

/// Copy only manifest-identified JARs plus overrides from the original ZIP.
/// The source's saves/ and CurseForge configuration stay untouched.
/// The returned receipt is a LOCAL HASH SNAPSHOT; not authenticated upstream.
pub fn stage_installed_curseforge(
    archive: &Path,
    installed_profile: &Path,
    staging: &Path,
    cancelled: &AtomicBool,
) -> Result<Vec<VerifiedModReceipt>, String> {
    // CurseForge's profile folder may itself be reached through a legitimate
    // Windows directory junction (for example, relocated Instances storage).
    // Canonicalize only the two EXISTING read-only source roots once. All
    // nested metadata/JAR paths and the NEW writable stage still undergo
    // fail-closed link checks. Never canonicalize an inner mod file to bypass
    // a symlink pointing outside the profile.
    let archive = fs::canonicalize(archive)
        .map_err(|e| format!("Could not resolve original CurseForge ZIP: {e}"))?;
    let installed_profile = fs::canonicalize(installed_profile)
        .map_err(|e| format!("Could not resolve installed CurseForge profile folder: {e}"))?;
    let (pack, names) = selection(&archive, &installed_profile, staging)?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Local import cancelled".into());
    }
    let mut preview = Vec::new();
    let mut total = 0u64;
    for (project_id, file_id, required, name) in &names {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Local import cancelled".into());
        }
        let source = installed_profile.join("mods").join(name);
        let (bytes, local_sha1, observed_sha256) = observed_hashes(&source)?;
        total = total
            .checked_add(bytes)
            .ok_or("installed pack total size overflow")?;
        if total > MAX_TOTAL_BYTES {
            return Err("Installed mods exceed 8 GiB limit".into());
        }
        preview.push((
            *project_id,
            *file_id,
            *required,
            name.clone(),
            bytes,
            local_sha1,
            observed_sha256,
        ));
    }
    let parent = staging.parent().ok_or("invalid stage parent")?;
    no_symlink_ancestors(parent)?;
    crate::stage_overrides(&archive, &pack, staging).map_err(|e| e.to_string())?;
    let target_mods = staging.join("mods");
    if fs::symlink_metadata(&target_mods).is_ok() {
        no_symlink_ancestors(&target_mods)?;
    } else {
        fs::create_dir(&target_mods).map_err(|e| e.to_string())?;
    }
    let mut result = Vec::new();
    for (project_id, file_id, required, name, bytes, local_sha1, observed_sha256) in preview {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Local import cancelled; private stage preserved".into());
        }
        let target = target_mods.join(&name);
        no_symlink_ancestors(&target)?;
        let mut from =
            File::open(installed_profile.join("mods").join(&name)).map_err(|e| e.to_string())?;
        let mut to = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| format!("Local mod destination is already occupied: {name}: {e}"))?;
        let copied = std::io::copy(&mut from, &mut to).map_err(|e| e.to_string())?;
        to.flush().map_err(|e| e.to_string())?;
        to.sync_all().map_err(|e| e.to_string())?;
        drop(to);
        if copied != bytes
            || observed_hashes(&target)? != (bytes, local_sha1.clone(), observed_sha256.clone())
            || observed_hashes(&installed_profile.join("mods").join(&name))?
                != (bytes, local_sha1.clone(), observed_sha256.clone())
        {
            return Err(format!("Local mod changed during copy: {name}"));
        }
        result.push(VerifiedModReceipt {
            project_id,
            file_id,
            required,
            relative_path: format!("mods/{name}"),
            bytes,
            // For local import this is the OBSERVED hash, not an upstream provider hash.
            upstream_sha1: local_sha1,
            observed_sha256,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Loader, PlannedFile};
    fn pack() -> PackMetadata {
        PackMetadata {
            name: "Test".into(),
            source: PackSource::CurseforgeZip,
            game_version: "1.16.5".into(),
            loader: Loader {
                kind: "forge".into(),
                version: Some("36.2.29".into()),
            },
            archive_sha256: "a".repeat(64),
            archive_entries: 1,
            uncompressed_bytes: 1,
            files: vec![PlannedFile {
                path: "mods/curseforge-12-34.jar".into(),
                provider: "curseforge".into(),
                project_id: Some(12),
                file_id: Some(34),
                urls: vec![],
                sha1: None,
                sha512: None,
                required: true,
                side: "both".into(),
            }],
            overrides: vec![],
            warnings: vec![],
        }
    }
    #[test]
    fn exact_local_file_id_and_name_required() {
        let good: LocalProfile = serde_json::from_str(r#"{"baseModLoader":{"name":"forge-36.2.29"},"installedAddons":[{"addonID":12,"installedFile":{"id":34,"fileName":"example.jar"}}]}"#).unwrap();
        assert_eq!(checked_names(&good, &pack()).unwrap()[0].3, "example.jar");
        let wrong: LocalProfile = serde_json::from_str(r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":35,"fileName":"example.jar"}}]}"#).unwrap();
        assert!(checked_names(&wrong, &pack()).is_err());
        let extra: LocalProfile = serde_json::from_str(r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":34,"fileName":"example.jar"}},{"addonID":77,"installedFile":{"id":88,"fileName":"extra.jar"}}]}"#).unwrap();
        assert!(checked_names(&extra, &pack()).is_err());
    }
    #[test]
    fn curseforge_installed_file_name_on_disk_is_preferred() {
        // Real CurseForge metadata uses installedFile.fileNameOnDisk.
        // A display/remote fileName may be absent or different.
        let data: LocalProfile = serde_json::from_str(
            r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":34,"fileNameOnDisk":"actually-installed.jar","fileName":"remote-original.jar"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            checked_names(&data, &pack()).unwrap()[0].3,
            "actually-installed.jar"
        );
    }

    #[test]
    fn outer_disk_filename_and_legacy_filename_remain_compatible() {
        let outer: LocalProfile = serde_json::from_str(
            r#"{"installedAddons":[{"addonID":12,"fileNameOnDisk":"outer.jar","installedFile":{"id":34}}]}"#,
        )
        .unwrap();
        assert_eq!(checked_names(&outer, &pack()).unwrap()[0].3, "outer.jar");
        let legacy: LocalProfile = serde_json::from_str(
            r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":34,"fileName":"fallback.jar"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            checked_names(&legacy, &pack()).unwrap()[0].3,
            "fallback.jar"
        );
    }

    #[test]
    fn duplicate_project_or_unsafe_disk_name_remain_blocked_and_diagnostic() {
        let duplicate: LocalProfile = serde_json::from_str(
            r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":34,"fileNameOnDisk":"a.jar"}},{"addonID":12,"installedFile":{"id":34,"fileNameOnDisk":"a.jar"}}]}"#,
        )
        .unwrap();
        let err = checked_names(&duplicate, &pack()).unwrap_err();
        assert!(err.contains("duplicate project ID 12"), "{err}");

        let escaped: LocalProfile = serde_json::from_str(
            r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":34,"fileNameOnDisk":"../escape.jar"}}]}"#,
        )
        .unwrap();
        let err = checked_names(&escaped, &pack()).unwrap_err();
        assert!(err.contains("addon 12"), "{err}");
        assert!(err.contains("safe JAR filename"), "{err}");

        let missing: LocalProfile = serde_json::from_str(
            r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":34}}]}"#,
        )
        .unwrap();
        let err = checked_names(&missing, &pack()).unwrap_err();
        assert!(err.contains("addon 12"), "{err}");
        assert!(err.contains("no installed filename"), "{err}");
    }

    #[test]
    fn installed_modpack_zip_entry_is_not_a_mod_jar() {
        let data: LocalProfile = serde_json::from_str(
            r#"{"gameVersion":"1.16.5","baseModLoader":{"name":"forge-36.2.29"},"installedAddons":[{"addonID":590644,"installedFile":{"id":111,"fileNameOnDisk":"Rustic Waters II 1.16-1.16.zip"}},{"addonID":12,"installedFile":{"id":34,"fileNameOnDisk":"example.jar"}}]}"#,
        ).unwrap();
        let matched = checked_names(&data, &pack()).unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].0, 12);
        let mut pack_with_bad_id = pack();
        pack_with_bad_id.files[0].project_id = Some(590644);
        assert!(checked_names(&data, &pack_with_bad_id).is_err());
    }

    #[test]
    fn folder_snapshot_does_not_require_original_zip_or_copy_saves() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Rustic Waters II");
        fs::create_dir_all(root.join("mods")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("saves/world")).unwrap();
        fs::create_dir_all(root.join("thingpacks/sf5_things/things/sf5_things/block")).unwrap();
        fs::create_dir_all(root.join("global_packs/required_data/sf5_worldgen/data")).unwrap();
        fs::create_dir_all(root.join("journeymap/config/5.10")).unwrap();
        fs::create_dir_all(root.join("journeymap/data/sp/world")).unwrap();
        let thing = "thingpacks/sf5_things/things/sf5_things/block/block_of_white_dye.json";
        let worldgen = "global_packs/required_data/sf5_worldgen/data/fixture.json";
        let map_config = "journeymap/config/5.10/journeymap.core.config";
        let map_data = "journeymap/data/sp/world/private.txt";
        for (path, bytes) in [
            (thing, b"custom block".as_slice()),
            (worldgen, b"worldgen reference".as_slice()),
            (map_config, b"map defaults".as_slice()),
            (map_data, b"private map".as_slice()),
        ] {
            fs::write(root.join(path), bytes).unwrap();
        }
        fs::write(root.join("mods/example.jar"), b"demo jar bytes").unwrap();
        fs::write(root.join("config/pack.toml"), b"key=1").unwrap();
        fs::write(root.join("saves/world/level.dat"), b"private world").unwrap();
        fs::write(root.join("minecraftinstance.json"), r#"{"name":"Rustic Waters II","gameVersion":"1.16.5","baseModLoader":{"name":"forge-36.2.29"},"installedAddons":[{"addonID":590644,"installedFile":{"id":123,"fileNameOnDisk":"Rustic Waters II 1.16-1.16.zip"}},{"addonID":12,"installedFile":{"id":34,"fileNameOnDisk":"example.jar"}}]}"#).unwrap();
        let archive = temp.path().join("private-source.zip");
        let cancel = AtomicBool::new(false);
        let mut progress = Vec::new();
        let planned = snapshot_installed_folder_with_progress(
            &root,
            &archive,
            &cancel,
            |stage, completed, total| progress.push((stage.to_owned(), completed, total)),
        )
        .unwrap();
        assert!(progress.iter().any(|(stage, completed, total)| stage
            == "local_import.hashing_mods"
            && completed == total));
        assert!(progress.iter().any(|(stage, completed, total)| stage
            == "local_import.scanning_assets"
            && completed == total));
        assert!(progress.iter().any(|(stage, completed, total)| stage
            == "local_import.copying_assets"
            && completed == total));
        assert_eq!(planned.game_version, "1.16.5");
        assert_eq!(planned.loader.version.as_deref(), Some("36.2.29"));
        assert_eq!(planned.files.len(), 1);
        assert!(planned.overrides.iter().any(|x| x == "config/pack.toml"));
        for path in [thing, worldgen, map_config] {
            assert!(planned.overrides.iter().any(|x| x == path), "{path}");
        }
        assert!(!planned.overrides.iter().any(|x| x == map_data));
        let zip = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
        assert!(!zip.file_names().any(|s| s.contains("saves")
            || s.contains("mods/")
            || s.contains(".jar")
            || s.contains("journeymap/data/")));
        let stage = temp.path().join("local-run-12345.staging");
        let imported = stage_installed_curseforge(&archive, &root, &stage, &cancel).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(
            fs::read(stage.join("mods/example.jar")).unwrap(),
            b"demo jar bytes"
        );
        assert_eq!(fs::read(stage.join("config/pack.toml")).unwrap(), b"key=1");
        for path in [thing, worldgen, map_config] {
            assert_eq!(
                fs::read(stage.join(path)).unwrap(),
                fs::read(root.join(path)).unwrap()
            );
        }
        assert!(!stage.join(map_data).exists());
        assert_eq!(
            fs::read(root.join("saves/world/level.dat")).unwrap(),
            b"private world"
        );
    }

    #[test]
    fn folder_snapshot_rejects_untracked_local_mod() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Test");
        fs::create_dir_all(root.join("mods")).unwrap();
        fs::write(root.join("mods/unknown.jar"), b"unknown").unwrap();
        fs::write(root.join("minecraftinstance.json"), r#"{"gameVersion":"1.16.5","baseModLoader":{"name":"forge-36.2.29"},"installedAddons":[{"addonID":590644,"installedFile":{"id":123,"fileNameOnDisk":"Rustic Waters II 1.16-1.16.zip"}}]}"#).unwrap();
        assert!(snapshot_installed_folder(
            &root,
            &temp.path().join("private.zip"),
            &AtomicBool::new(false)
        )
        .is_err());
    }
    #[test]
    fn existing_roots_resolve_to_real_absolute_directory() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("installed");
        fs::create_dir(&real).unwrap();
        let canonical = fs::canonicalize(&real).unwrap();
        assert!(canonical.is_absolute());
        assert!(canonical.is_dir());
        // The normalized source is safe to check; writable stage remains
        // separate and cannot redirect outside the private instance root.
        no_symlink_ancestors(&canonical).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn installed_profile_windows_directory_link_resolves_to_real_folder() {
        use std::os::windows::fs::symlink_dir;
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real-installed");
        fs::create_dir(&real).unwrap();
        let linked = temp.path().join("selected-instances-link");
        if symlink_dir(&real, &linked).is_err() {
            // A Windows runner without Developer Mode/SeCreateSymbolicLinkPrivilege
            // cannot create this fixture; importer still relies on canonicalize.
            return;
        }
        assert!(no_symlink_ancestors(&linked).is_err());
        let resolved = fs::canonicalize(&linked).unwrap();
        assert_eq!(resolved, fs::canonicalize(&real).unwrap());
        no_symlink_ancestors(&resolved).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn linked_mod_file_under_real_profile_still_blocked() {
        use std::os::windows::fs::symlink_file;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("profile");
        fs::create_dir(&root).unwrap();
        let external = temp.path().join("other.jar");
        fs::write(&external, b"not-a-jar").unwrap();
        let mod_file = root.join("bad.jar");
        if symlink_file(&external, &mod_file).is_err() {
            return;
        }
        // Legitimate linked profile folder is allowed, but a linked
        // individual JAR is never trusted as part of the selected profile.
        assert!(observed_hashes(&mod_file).is_err());
    }
    #[test]
    fn unsafe_filename_refused() {
        let bad: LocalProfile = serde_json::from_str(r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":34,"fileName":"../evil.jar"}}]}"#).unwrap();
        assert!(checked_names(&bad, &pack()).is_err());
    }
}
