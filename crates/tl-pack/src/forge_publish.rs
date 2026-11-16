//! Trusted Forge 1.16.5 publication boundary.
//! Only execution proof sealed by tl-loader's actual guarded runner can reach
//! the in-process verifier and permit issuer.
//! This module NEVER executes processors, downloads mods or trusts frontend
//! claims of readiness. A caller must provide actual approved execution inputs.
use crate::{
    curseforge::CurseForgeResolveReport,
    publisher::{publish_verified, VerifiedPublishPermit},
    staged_receipt::{audit_curseforge_staging, VerifiedModReceipt},
    PackMetadata, PackSource,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest as _, Sha1};
use sha2::Sha256;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use tl_loader::{
    forge_playable::{
        verify_forge_runtime_for_pack, PackForgeSelection, VerifiedForgeJava, VerifiedForgeLibrary,
    },
    runtime::{VerifiedProcessorExecution, VerifiedProcessorJar},
};

pub const FORGE_INSTALL_MARKER: &str = ".tlauchcraft-forge-install.json";
const MAX_RECORD_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledForgeLibrary {
    pub relative_path: String,
    pub official_sha1: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedOverride {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Durable *file-integrity* evidence, not a claim that game UI/world/LAN ran.
/// The SQLite READY state is set separately after a successful atomic publish.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeInstallRecord {
    pub schema: String,
    pub minecraft_version: String,
    pub forge_version: String,
    /// Java version from the trusted loader compatibility adapter.
    pub required_java_major: u32,
    pub archive_sha256: String,
    pub version: Value,
    pub natives_relative_path: String,
    pub installed_mods: Vec<VerifiedModReceipt>,
    pub installed_libraries: Vec<InstalledForgeLibrary>,
    pub verified_overrides: Vec<VerifiedOverride>,
    pub minecraft_verified_files: usize,
}

pub struct ApprovedForgePublishInputs<'a> {
    pub selection: &'a PackForgeSelection,
    pub profile_json: &'a str,
    pub derived: &'a Value,
    pub mojang_parent: &'a Value,
    pub processor_execution: &'a VerifiedProcessorExecution,
    pub java: &'a VerifiedForgeJava,
    pub maven: &'a [VerifiedProcessorJar],
    pub installer_or_generated: &'a [VerifiedForgeLibrary],
}

fn reject_symlinks(path: &Path) -> Result<(), String> {
    for part in path.ancestors() {
        if fs::symlink_metadata(part).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Forge publication path contains a symlink".into());
        }
    }
    Ok(())
}

fn verify_tree_symlinks(dir: &Path) -> Result<(), String> {
    reject_symlinks(dir)?;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Forge staged instance contains a symlink".into());
        }
        if kind.is_dir() {
            verify_tree_symlinks(&entry.path())?;
        } else if !kind.is_file() {
            return Err("Forge staged instance contains a special file".into());
        }
    }
    Ok(())
}

fn file_hashes(path: &Path) -> Result<(u64, String, String), String> {
    reject_symlinks(path)?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() || meta.len() > crate::MAX_SINGLE_FILE {
        return Err("missing or oversized Forge file".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut total = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).ok_or("file size overflow")?;
        if total > meta.len() {
            return Err("file grew during validation".into());
        }
        sha1.update(&buf[..n]);
        sha256.update(&buf[..n]);
    }
    if total != meta.len() {
        return Err("file changed during validation".into());
    }
    Ok((
        total,
        format!("{:x}", sha1.finalize()),
        format!("{:x}", sha256.finalize()),
    ))
}

fn check_file(
    root: &Path,
    relative: &str,
    size: u64,
    sha1: Option<&str>,
    sha256: Option<&str>,
) -> Result<(), String> {
    let rel = crate::safe_path(relative).map_err(|e| e.to_string())?;
    let (actual_size, actual_sha1, actual_sha256) = file_hashes(&root.join(rel))?;
    if actual_size != size
        || sha1.is_some_and(|expected| !actual_sha1.eq_ignore_ascii_case(expected))
        || sha256.is_some_and(|expected| !actual_sha256.eq_ignore_ascii_case(expected))
    {
        return Err(format!("published Forge artifact changed: {relative}"));
    }
    Ok(())
}

fn verify_natives(root: &Path, relative: &str) -> Result<(), String> {
    let path = root.join(crate::safe_path(relative).map_err(|e| e.to_string())?);
    reject_symlinks(&path)?;
    let mut dlls = 0usize;
    for entry in fs::read_dir(&path).map_err(|_| "verified Windows natives missing")? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("native directory contains a symlink".into());
        }
        if kind.is_file()
            && entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("dll"))
        {
            dlls += 1;
        }
    }
    if dlls == 0 {
        return Err("verified Windows native DLLs missing".into());
    }
    Ok(())
}

/// Recheck overrides against the original ZIP bytes, not a JSON/boolean
/// supplied by the UI. Overrides are staged at the game-directory root.
fn verify_original_overrides(
    root: &Path,
    archive: &Path,
    manifest: &PackMetadata,
) -> Result<Vec<VerifiedOverride>, String> {
    let input = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(input).map_err(|e| e.to_string())?;
    let metadata: Value = {
        let mut entry = zip.by_name("manifest.json").map_err(|e| e.to_string())?;
        if entry.size() > crate::MAX_MANIFEST {
            return Err("oversized CurseForge manifest".into());
        }
        serde_json::from_reader(&mut entry).map_err(|e| e.to_string())?
    };
    let prefix = metadata["overrides"].as_str().unwrap_or("overrides");
    let prefix = crate::safe_path(prefix).map_err(|e| e.to_string())?;
    let prefix = prefix.to_string_lossy();
    let mut verified = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for relative in &manifest.overrides {
        let destination = crate::safe_path(relative).map_err(|e| e.to_string())?;
        if !seen.insert(relative.to_ascii_lowercase()) {
            return Err("duplicate override in pack manifest".into());
        }
        let source_path = format!("{prefix}/{relative}");
        // The normalized ZIP inspector strips a trailing slash on directory
        // entries; ZIP lookup must restore it before comparing directories.
        let zip_name = if zip.file_names().any(|name| name == source_path) {
            source_path
        } else {
            format!("{source_path}/")
        };
        let mut entry = zip
            .by_name(&zip_name)
            .map_err(|_| "missing original override")?;
        let target = root.join(&destination);
        reject_symlinks(&target)?;
        if entry.is_dir() {
            if !target.is_dir() {
                return Err("override directory missing".into());
            }
            continue;
        }
        if entry.size() > crate::MAX_SINGLE_FILE {
            return Err("invalid original override file size".into());
        }
        let mut source_hash = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = entry.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            source_hash.update(&buf[..n]);
        }
        let expected = format!("{:x}", source_hash.finalize());
        check_file(root, relative, entry.size(), None, Some(&expected))?;
        verified.push(VerifiedOverride {
            relative_path: relative.clone(),
            sha256: expected,
            bytes: entry.size(),
        });
    }
    Ok(verified)
}

/// This method does NOT execute a third-party Forge processor. Callers must
/// separately obtain Control's approval and isolated execution evidence.
/// Only real verified processor outputs and complete disk bytes can mint permit.
pub fn publish_forge_from_execution(
    stage: &Path,
    destination: &Path,
    source_archive: &Path,
    pack: &PackMetadata,
    resolved: &CurseForgeResolveReport,
    evidence: ApprovedForgePublishInputs<'_>,
) -> Result<ForgeInstallRecord, String> {
    if pack.source != PackSource::CurseforgeZip || !stage.is_absolute() {
        return Err("Forge requires an original CurseForge archive and private stage".into());
    }
    let current = crate::inspect_archive(source_archive).map_err(|e| e.to_string())?;
    if current.archive_sha256 != pack.archive_sha256
        || current.game_version != pack.game_version
        || current.loader != pack.loader
        || current.files != pack.files
        || current.overrides != pack.overrides
    {
        return Err("source ZIP differs from reviewed Forge manifest".into());
    }
    if pack.game_version != evidence.selection.minecraft_version
        || pack.loader.kind != "forge"
        || pack.loader.version.as_deref() != Some(evidence.selection.forge_version.as_str())
        || pack.archive_sha256 != evidence.selection.pack_archive_sha256
    {
        return Err("Forge runtime version differs from original modpack".into());
    }
    verify_tree_symlinks(stage)?;
    let mods = audit_curseforge_staging(stage, pack, resolved)?;
    if !mods.complete_mod_inventory
        || !mods.blocked_optional.is_empty()
        || !mods.failures.is_empty()
        || mods.verified.len() != pack.files.len()
    {
        return Err("required/optional mod inventory is incomplete or unverified".into());
    }
    let runtime = verify_forge_runtime_for_pack(
        evidence.selection,
        evidence.profile_json,
        evidence.derived,
        evidence.mojang_parent,
        evidence.processor_execution.request(),
        evidence.processor_execution.report(),
        evidence.java,
        evidence.maven,
        evidence.installer_or_generated,
    )
    .map_err(|e| format!("Forge processor verification failed: {e}"))?;
    if !runtime.execution_verified
        || runtime.required_java_major == 0
        || runtime.instance_ready
        || runtime.game_launched
        || runtime.processor_count != 4
    {
        return Err("Forge processors not accepted by trusted runtime".into());
    }
    let mut libraries = Vec::new();
    for library in runtime.libraries {
        let relative = format!("libraries/{}", library.relative_path);
        check_file(
            stage,
            &relative,
            library.size_bytes,
            Some(&library.official_sha1),
            None,
        )?;
        libraries.push(InstalledForgeLibrary {
            relative_path: relative,
            official_sha1: library.official_sha1,
            size_bytes: library.size_bytes,
        });
    }
    if libraries.is_empty() {
        return Err("Forge library inventory empty".into());
    }
    let overrides = verify_original_overrides(stage, source_archive, pack)?;
    verify_natives(stage, "natives")?;
    let version = runtime.java8_launch_version;
    if version["id"] != runtime.derived_version_id
        || version["mainClass"] != "cpw.mods.modlauncher.Launcher"
        || version["inheritsFrom"] != pack.game_version
        || version["javaVersion"]["majorVersion"] != runtime.required_java_major
    {
        return Err("inherited Forge launch metadata invalid".into());
    }
    let checked = tl_game::vanilla_evidence::validate_install_files(&version, stage)
        .map_err(|e| format!("Mojang/Forge client, natives, assets invalid: {e}"))?;
    fs::create_dir_all(stage.join("logs")).map_err(|e| e.to_string())?;
    let record = ForgeInstallRecord {
        schema: "tl-forge-published/v1".into(),
        minecraft_version: pack.game_version.clone(),
        forge_version: evidence.selection.forge_version.clone(),
        required_java_major: runtime.required_java_major,
        archive_sha256: pack.archive_sha256.clone(),
        version,
        natives_relative_path: "natives".into(),
        installed_mods: mods.verified,
        installed_libraries: libraries,
        verified_overrides: overrides,
        minecraft_verified_files: checked,
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("published Forge record too large".into());
    }
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join(FORGE_INSTALL_MARKER))
        .map_err(|e| format!("Forge marker cannot be written create-new: {e}"))?;
    out.write_all(&bytes)
        .and_then(|_| out.sync_all())
        .map_err(|e| e.to_string())?;
    publish_verified(
        stage,
        destination,
        VerifiedPublishPermit::after_verified_forge_publish_audit(),
    )?;
    Ok(record)
}

/// Disk read is not enough to issue a publish permit. Used only at Play time,
/// after the corresponding persisted SQLite instance is already READY.
pub fn reverify_published_forge(root: &Path) -> Result<ForgeInstallRecord, String> {
    if !root.is_absolute() {
        return Err("published Forge path must be absolute".into());
    }
    verify_tree_symlinks(root)?;
    let marker = root.join(FORGE_INSTALL_MARKER);
    if fs::metadata(&marker)
        .map_err(|_| "Forge install record missing")?
        .len()
        > MAX_RECORD_BYTES
    {
        return Err("Forge install record oversized".into());
    }
    let record: ForgeInstallRecord =
        serde_json::from_slice(&fs::read(&marker).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if record.schema != "tl-forge-published/v1"
        || record.minecraft_version.is_empty()
        || record.forge_version.is_empty()
        || !(8..=25).contains(&record.required_java_major)
        || record.archive_sha256.len() != 64
        || !record.archive_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || record.version["id"]
            != format!(
                "{}-forge-{}",
                record.minecraft_version, record.forge_version
            )
        || record.version["mainClass"] != "cpw.mods.modlauncher.Launcher"
        || record.version["inheritsFrom"] != record.minecraft_version
        || record.version["javaVersion"]["majorVersion"] != record.required_java_major
        || record.installed_libraries.is_empty()
        || record.installed_mods.is_empty()
    {
        return Err("Forge installed marker is not valid for its recorded loader selection".into());
    }
    for library in &record.installed_libraries {
        check_file(
            root,
            &library.relative_path,
            library.size_bytes,
            Some(&library.official_sha1),
            None,
        )?;
    }
    for file in &record.installed_mods {
        check_file(
            root,
            &file.relative_path,
            file.bytes,
            Some(&file.upstream_sha1),
            Some(&file.observed_sha256),
        )?;
    }
    for file in &record.verified_overrides {
        check_file(
            root,
            &file.relative_path,
            file.bytes,
            None,
            Some(&file.sha256),
        )?;
    }
    verify_natives(root, &record.natives_relative_path)?;
    let count = tl_game::vanilla_evidence::validate_install_files(&record.version, root)
        .map_err(|e| e.to_string())?;
    if count != record.minecraft_verified_files {
        return Err("Forge Minecraft file inventory changed".into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_publish_marker_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        assert!(reverify_published_forge(temp.path()).is_err());
    }

    #[test]
    fn forged_ready_json_cannot_issue_publish_permit() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(FORGE_INSTALL_MARKER),
            r#"{"schema":"tl-forge-published/v1","state":"READY"}"#,
        )
        .unwrap();
        assert!(reverify_published_forge(temp.path()).is_err());
    }

    #[test]
    fn published_file_hash_changes_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("mod.jar"), b"original").unwrap();
        let expected = format!("{:x}", Sha256::digest(b"original"));
        assert!(check_file(temp.path(), "mod.jar", 8, None, Some(&expected)).is_ok());
        fs::write(temp.path().join("mod.jar"), b"modified").unwrap();
        assert!(check_file(temp.path(), "mod.jar", 8, None, Some(&expected)).is_err());
        assert!(check_file(temp.path(), "../escape", 8, None, Some(&expected)).is_err());
    }
}
