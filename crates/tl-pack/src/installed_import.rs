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
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

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
    #[serde(default)]
    file_name: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalProfile {
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
            return Err("Installed profile/staging path contains a symlink".into());
        }
    }
    Ok(())
}

fn checked_names(
    local: &LocalProfile,
    manifest: &PackMetadata,
) -> Result<Vec<(u64, u64, bool, String)>, String> {
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
    let mut map = BTreeMap::new();
    for addon in &local.installed_addons {
        let installed = addon
            .installed_file
            .as_ref()
            .ok_or("CurseForge local metadata lacks installedFile; cannot match manifest IDs")?;
        let name = addon
            .file_name_on_disk
            .as_deref()
            .or(installed.file_name.as_deref())
            .ok_or("CurseForge local metadata lacks an installed mod filename")?;
        if addon.addon_id == 0
            || installed.id == 0
            || !safe_mod_file_name(name)
            || map
                .insert(addon.addon_id, (installed.id, name.to_owned()))
                .is_some()
        {
            return Err("Malformed/duplicated CurseForge profile mod metadata".into());
        }
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
) -> Result<(PackMetadata, Vec<(u64, u64, bool, String)>), String> {
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
    let metadata_path = installed_profile.join("minecraftinstance.json");
    no_symlink_ancestors(&metadata_path)?;
    let info = fs::symlink_metadata(&metadata_path).map_err(|_| {
        "CurseForge minecraftinstance.json missing; cannot match mod IDs without API"
    })?;
    if !info.file_type().is_file() || info.len() > MAX_PROFILE_META_BYTES {
        return Err("CurseForge profile metadata missing/oversized".into());
    }
    let raw = fs::read(&metadata_path).map_err(|e| e.to_string())?;
    let local: LocalProfile = serde_json::from_slice(&raw)
        .map_err(|e| format!("CurseForge profile metadata invalid: {e}"))?;
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
    let (pack, names) = selection(archive, installed_profile, staging)?;
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
    crate::stage_overrides(archive, &pack, staging).map_err(|e| e.to_string())?;
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
    fn unsafe_filename_refused() {
        let bad: LocalProfile = serde_json::from_str(r#"{"installedAddons":[{"addonID":12,"installedFile":{"id":34,"fileName":"../evil.jar"}}]}"#).unwrap();
        assert!(checked_names(&bad, &pack()).is_err());
    }
}
