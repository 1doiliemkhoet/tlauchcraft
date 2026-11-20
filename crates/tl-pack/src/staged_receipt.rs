//! Read-only inventory evidence for a private CurseForge stage.
//! All claims are based on official provider IDs, upstream SHA-1 and actual
//! disk bytes. This is NOT a Forge execution or Minecraft READY receipt.
use crate::{curseforge::CurseForgeResolveReport, PackMetadata, PackSource};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::Read,
    path::Path,
};

const MAX_MOD_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedModReceipt {
    pub project_id: u64,
    pub file_id: u64,
    pub required: bool,
    pub relative_path: String,
    pub bytes: u64,
    pub upstream_sha1: String,
    pub observed_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedPackReceipt {
    pub source: PackSource,
    pub minecraft_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub archive_sha256: String,
    pub required_total: usize,
    pub optional_total: usize,
    pub verified: Vec<VerifiedModReceipt>,
    pub blocked_required: Vec<String>,
    pub blocked_optional: Vec<String>,
    pub failures: Vec<String>,
    pub complete_mod_inventory: bool,
    pub forge_runtime_verified: bool,
    pub instance_ready: bool,
}

fn reject_symlinks(path: &Path) -> Result<(), String> {
    for item in path.ancestors() {
        if fs::symlink_metadata(item).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("symlinked staged pack file or ancestor".into());
        }
    }
    Ok(())
}

fn verify_file(root: &Path, filename: &str, sha1: &str, size: u64) -> Result<String, String> {
    if !crate::curseforge::safe_mod_file_name(filename) {
        return Err("unsafe official mod filename".into());
    }
    if size == 0
        || size > MAX_MOD_BYTES
        || sha1.len() != 40
        || !sha1.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("official SHA-1 or size unavailable".into());
    }
    let path = root.join("mods").join(filename);
    reject_symlinks(&path)?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| "file missing from private staging")?;
    if !metadata.file_type().is_file() || metadata.len() != size {
        return Err("staged file type or byte count mismatch".into());
    }
    let mut input = File::open(path).map_err(|_| "cannot read staged mod")?;
    let mut sha1_state = Sha1::new();
    let mut sha256_state = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = input
            .read(&mut buffer)
            .map_err(|_| "staged mod read failed")?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .ok_or("staged mod size overflow")?;
        if total > size {
            return Err("staged mod grew during verification".into());
        }
        sha1_state.update(&buffer[..n]);
        sha256_state.update(&buffer[..n]);
    }
    if total != size || format!("{:x}", sha1_state.finalize()) != sha1.to_ascii_lowercase() {
        return Err("staged mod SHA-1 mismatch".into());
    }
    Ok(format!("{:x}", sha256_state.finalize()))
}

/// This re-verifies both CDN downloads and user-imported restricted mods.
/// Missing required files, unresolved relations and unknown extra JARs block
/// inventory completion. Even complete inventory NEVER certifies a loader.
pub fn audit_curseforge_staging(
    stage: &Path,
    manifest: &PackMetadata,
    resolved: &CurseForgeResolveReport,
) -> Result<StagedPackReceipt, String> {
    if manifest.source != PackSource::CurseforgeZip
        || manifest.loader.kind != "forge"
        || manifest.loader.version.as_deref().is_none_or(str::is_empty)
        || manifest.archive_sha256.len() != 64
        || !manifest
            .archive_sha256
            .bytes()
            .all(|c| c.is_ascii_hexdigit())
        || !stage.is_absolute()
        || !stage
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|s| s.ends_with(".staging"))
    {
        return Err("verified CurseForge manifest and private stage required".into());
    }
    reject_symlinks(stage)?;
    if !fs::symlink_metadata(stage).is_ok_and(|m| m.file_type().is_dir()) {
        return Err("private staging directory missing".into());
    }
    if manifest.files.len() != resolved.files.len() {
        return Err("official resolver inventory count mismatch".into());
    }
    let mut report = StagedPackReceipt {
        source: manifest.source,
        minecraft_version: manifest.game_version.clone(),
        loader_kind: manifest.loader.kind.clone(),
        loader_version: manifest.loader.version.clone().unwrap_or_default(),
        archive_sha256: manifest.archive_sha256.clone(),
        required_total: manifest.files.iter().filter(|f| f.required).count(),
        optional_total: manifest.files.iter().filter(|f| !f.required).count(),
        verified: Vec::new(),
        blocked_required: Vec::new(),
        blocked_optional: Vec::new(),
        failures: Vec::new(),
        complete_mod_inventory: false,
        forge_runtime_verified: false,
        instance_ready: false,
    };
    let mut planned_names = HashSet::new();
    for (declared, entry) in manifest.files.iter().zip(&resolved.files) {
        if declared.project_id != Some(entry.project_id)
            || declared.file_id != Some(entry.file_id)
            || declared.required != entry.required
        {
            return Err("provider identity or required flag differs from ZIP".into());
        }
        let identity = format!("{}:{}", entry.project_id, entry.file_id);
        let Some(name) = entry.file_name.as_deref() else {
            let reason = format!("{identity}: official filename unavailable");
            if entry.required {
                report.blocked_required.push(reason);
            } else {
                report.blocked_optional.push(reason);
            }
            continue;
        };
        if !planned_names.insert(name.to_ascii_lowercase()) {
            report
                .failures
                .push(format!("{identity}: duplicate Windows target filename"));
            continue;
        }
        let (Some(sha1), Some(size)) = (entry.sha1.as_deref(), entry.expected_size) else {
            let reason = format!(
                "{identity}: missing authoritative SHA-1/size; {}",
                entry.reason
            );
            if entry.required {
                report.blocked_required.push(reason);
            } else {
                report.blocked_optional.push(reason);
            }
            continue;
        };
        match verify_file(stage, name, sha1, size) {
            Ok(observed_sha256) => {
                report.verified.push(VerifiedModReceipt {
                    project_id: entry.project_id,
                    file_id: entry.file_id,
                    required: entry.required,
                    relative_path: format!("mods/{name}"),
                    bytes: size,
                    upstream_sha1: sha1.to_ascii_lowercase(),
                    observed_sha256,
                });
            }
            Err(error) => {
                let reason = format!("{identity}: {error}; provider={:?}", entry.availability);
                if error == "file missing from private staging" {
                    if entry.required {
                        report.blocked_required.push(reason);
                    } else {
                        report.blocked_optional.push(reason);
                    }
                } else {
                    report.failures.push(reason);
                }
            }
        }
    }
    for dep in &resolved.dependency_actions {
        let reason = format!(
            "dependency project {} required by {}/{}: {}",
            dep.project_id, dep.parent_project_id, dep.parent_file_id, dep.reason
        );
        if dep.required {
            report.blocked_required.push(reason);
        } else {
            report.blocked_optional.push(reason);
        }
    }
    let mods_dir = stage.join("mods");
    if fs::symlink_metadata(&mods_dir).is_ok_and(|m| m.file_type().is_dir()) {
        reject_symlinks(&mods_dir)?;
        for entry in fs::read_dir(&mods_dir).map_err(|_| "cannot list staged mods")? {
            let entry = entry.map_err(|_| "cannot inspect staged mod entry")?;
            let filename = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !planned_names.contains(&filename) {
                report
                    .failures
                    .push(format!("unknown or partial staged mod: {filename}"));
            }
        }
    }
    report.complete_mod_inventory = report.failures.is_empty()
        && report.blocked_required.is_empty()
        && report.verified.iter().filter(|f| f.required).count() == report.required_total;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        curseforge::{CurseForgeFile, FileAvailability},
        Loader, PlannedFile,
    };
    fn fixture(bytes: &[u8]) -> (PackMetadata, CurseForgeResolveReport) {
        let file = PlannedFile {
            path: "mods/official.jar".into(),
            provider: "curseforge".into(),
            project_id: Some(100),
            file_id: Some(200),
            urls: Vec::new(),
            sha1: None,
            sha512: None,
            required: true,
            side: "both".into(),
        };
        let pack = PackMetadata {
            name: "Independent forge pack".into(),
            source: PackSource::CurseforgeZip,
            game_version: "1.16.5".into(),
            loader: Loader {
                kind: "forge".into(),
                version: Some("36.2.29".into()),
            },
            archive_sha256: "a".repeat(64),
            archive_entries: 2,
            uncompressed_bytes: 500,
            files: vec![file],
            overrides: Vec::new(),
            warnings: Vec::new(),
        };
        let resolved = CurseForgeResolveReport {
            files: vec![CurseForgeFile {
                project_id: 100,
                file_id: 200,
                required: true,
                file_name: Some("official.jar".into()),
                download_url: None,
                sha1: Some(format!("{:x}", Sha1::digest(bytes))),
                expected_size: Some(bytes.len() as u64),
                availability: FileAvailability::NeedsManualDownload,
                reason: "manual import required".into(),
                dependencies: Vec::new(),
            }],
            resolved_count: 0,
            needs_action_count: 1,
            dependency_actions: Vec::new(),
            ready: false,
        };
        (pack, resolved)
    }
    #[test]
    fn verified_manually_imported_mod_can_complete_inventory_but_never_ready() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().canonicalize().unwrap().join("fixture.staging");
        fs::create_dir_all(stage.join("mods")).unwrap();
        fs::write(stage.join("mods/official.jar"), b"verified bytes").unwrap();
        let (pack, api) = fixture(b"verified bytes");
        let receipt = audit_curseforge_staging(&stage, &pack, &api).unwrap();
        assert!(receipt.complete_mod_inventory);
        assert_eq!(receipt.verified.len(), 1);
        assert!(!receipt.forge_runtime_verified);
        assert!(!receipt.instance_ready);
    }
    #[test]
    fn missing_or_tampered_required_mod_cannot_complete_inventory() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().canonicalize().unwrap().join("fixture.staging");
        fs::create_dir_all(stage.join("mods")).unwrap();
        let (pack, api) = fixture(b"trusted bytes");
        assert!(
            !audit_curseforge_staging(&stage, &pack, &api)
                .unwrap()
                .complete_mod_inventory
        );
        fs::write(stage.join("mods/official.jar"), b"modified bytes").unwrap();
        let r = audit_curseforge_staging(&stage, &pack, &api).unwrap();
        assert!(!r.complete_mod_inventory);
        assert!(!r.failures.is_empty());
    }
    #[test]
    fn undeclared_mod_and_required_relation_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().canonicalize().unwrap().join("fixture.staging");
        fs::create_dir_all(stage.join("mods")).unwrap();
        fs::write(stage.join("mods/official.jar"), b"good").unwrap();
        fs::write(stage.join("mods/unknown.jar"), b"unknown").unwrap();
        let (pack, api) = fixture(b"good");
        let receipt = audit_curseforge_staging(&stage, &pack, &api).unwrap();
        assert!(!receipt.complete_mod_inventory);
        assert!(receipt.failures.iter().any(|v| v.contains("unknown")));
    }
}
