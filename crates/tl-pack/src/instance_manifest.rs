//! Prepare a durable *non-ready* game-instance manifest inside private
//! staging. Actual READY and SQLite instance publication require the merged,
//! trusted tl-loader runtime and a separate atomic commit transaction.
use crate::{
    artifact_audit::{audit_staged_artifacts, ExpectedArtifact},
    normalized::NormalizedPack,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

pub const INSTALL_MANIFEST_NAME: &str = ".tlauchcraft-pack-install.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchPreferences {
    /// A proposed Java path, not evidence of executable compatibility.
    pub java_path: String,
    pub xms_mb: u32,
    pub xmx_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedInstanceManifest {
    pub schema_version: u32,
    pub state: String,
    pub pack: NormalizedPack,
    pub expected_files: Vec<ExpectedArtifact>,
    pub required_verified: usize,
    pub optional_verified: usize,
    pub launch: LaunchPreferences,
    /// Persisted only to make READY=false explicit across crashes.
    pub ready: bool,
}

/// Re-hash staged payloads immediately before writing a create-new marker.
/// A marker is *not* an executable loader receipt and cannot mint publication
/// permission. Existing manifest/world/profile files are never overwritten.
pub fn persist_staged_manifest(
    stage: &Path,
    pack: &NormalizedPack,
    expected: &[ExpectedArtifact],
    launch: LaunchPreferences,
) -> Result<StagedInstanceManifest, String> {
    if !stage.is_absolute() || pack.files.is_empty() || expected.is_empty() {
        return Err("private stage and nonempty complete manifest required".into());
    }
    if launch.xms_mb < 256
        || launch.xmx_mb < launch.xms_mb
        || launch.xmx_mb > 262_144
        || launch.java_path.trim().is_empty()
    {
        return Err("invalid proposed Java/RAM preferences".into());
    }
    if pack.archive_sha256.len() != 64
        || !pack.archive_sha256.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("invalid archive identity".into());
    }
    // Exact inventory identity is required. Count-only comparisons can be
    // bypassed by duplicating one good mod to hide a different missing mod.
    let mut planned = std::collections::HashMap::new();
    for file in &pack.files {
        let key = file.target.to_ascii_lowercase();
        if planned.insert(key, file).is_some() {
            return Err("duplicate normalized mod path".into());
        }
    }
    let mut observed = std::collections::HashSet::new();
    for artifact in expected {
        let key = artifact.relative_path.to_ascii_lowercase();
        if !observed.insert(key.clone()) {
            return Err("duplicate expected artifact path".into());
        }
        let file = planned
            .get(&key)
            .ok_or("unexpected staged artifact identity")?;
        if file.required != artifact.required
            || file.sha1.as_ref().is_some_and(|h| {
                artifact
                    .upstream_sha1
                    .as_ref()
                    .is_none_or(|v| !v.eq_ignore_ascii_case(h))
            })
            || file.sha512.as_ref().is_some_and(|h| {
                artifact
                    .upstream_sha512
                    .as_ref()
                    .is_none_or(|v| !v.eq_ignore_ascii_case(h))
            })
        {
            return Err("staged artifact does not match normalized plan".into());
        }
    }
    if observed.len() != planned.len() {
        return Err("expected inventory omits manifest entries".into());
    }
    let expected_required = expected.iter().filter(|f| f.required).count();
    let target = stage.join(INSTALL_MANIFEST_NAME);
    if fs::symlink_metadata(&target).is_ok() {
        return Err("staged manifest already exists; never overwrite".into());
    }
    let checked = audit_staged_artifacts(stage, expected);
    if !checked.complete || checked.required_verified != expected_required {
        return Err("incomplete or unverified required staged artifacts".into());
    }
    let manifest = StagedInstanceManifest {
        schema_version: 1,
        state: "AWAITING_LOADER_VERIFICATION".into(),
        pack: pack.clone(),
        expected_files: expected.to_vec(),
        required_verified: checked.required_verified,
        optional_verified: checked.optional_verified,
        launch,
        ready: false,
    };
    let data = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| "cannot serialize staged instance manifest")?;
    if data.len() > 4 * 1024 * 1024 {
        return Err("staged manifest exceeds size bound".into());
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)
        .map_err(|_| "cannot create new staged manifest")?;
    file.write_all(&data)
        .and_then(|_| file.sync_all())
        .map_err(|_| "staged manifest write or sync failed")?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        normalized::{InstallSupport, LoaderKind, NormalizedFile, ProviderKind},
        PackSource,
    };
    use sha1::Sha1;
    use sha2::{Digest, Sha256};
    fn pack() -> NormalizedPack {
        NormalizedPack {
            name: "Generic pack".into(),
            source: PackSource::ModrinthMrpack,
            archive_sha256: "a".repeat(64),
            minecraft_version: "1.20.1".into(),
            loader: LoaderKind::Fabric,
            loader_version: "0.16.0".into(),
            files: vec![NormalizedFile {
                target: "mods/a.jar".into(),
                provider: ProviderKind::Modrinth,
                required: true,
                project_id: None,
                file_id: None,
                sha1: Some(format!("{:x}", Sha1::digest(b"fixture"))),
                sha512: None,
                download_urls: Vec::new(),
            }],
            support: InstallSupport::InspectOnly,
            reasons: vec!["Fabric runtime unavailable".into()],
        }
    }
    fn expected() -> ExpectedArtifact {
        ExpectedArtifact {
            relative_path: "mods/a.jar".into(),
            required: true,
            bytes: 7,
            sha256: format!("{:x}", Sha256::digest(b"fixture")),
            upstream_sha1: Some(format!("{:x}", Sha1::digest(b"fixture"))),
            upstream_sha512: None,
        }
    }
    fn launch() -> LaunchPreferences {
        LaunchPreferences {
            java_path: "C:\\Java\\bin\\java.exe".into(),
            xms_mb: 512,
            xmx_mb: 4096,
        }
    }
    #[test]
    fn create_new_manifest_is_durable_nonready_and_idempotently_refuses_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let stage = tmp.path().canonicalize().unwrap();
        fs::create_dir(stage.join("mods")).unwrap();
        fs::write(stage.join("mods/a.jar"), b"fixture").unwrap();
        let result = persist_staged_manifest(&stage, &pack(), &[expected()], launch()).unwrap();
        assert_eq!(result.state, "AWAITING_LOADER_VERIFICATION");
        assert_eq!(result.required_verified, 1);
        assert!(!result.ready);
        assert!(persist_staged_manifest(&stage, &pack(), &[expected()], launch()).is_err());
        let saved = fs::read_to_string(stage.join(INSTALL_MANIFEST_NAME)).unwrap();
        assert!(saved.contains("AWAITING_LOADER_VERIFICATION"));
        assert!(saved.contains("\"ready\": false"));
    }
    #[test]
    fn mismatch_blocks_manifest_and_preserves_existing_world() {
        let tmp = tempfile::tempdir().unwrap();
        let stage = tmp.path().canonicalize().unwrap();
        fs::create_dir(stage.join("mods")).unwrap();
        fs::create_dir(stage.join("saves")).unwrap();
        fs::write(stage.join("mods/a.jar"), b"tampered").unwrap();
        fs::write(stage.join("saves/level.dat"), b"world").unwrap();
        assert!(persist_staged_manifest(&stage, &pack(), &[expected()], launch()).is_err());
        assert!(!stage.join(INSTALL_MANIFEST_NAME).exists());
        assert_eq!(fs::read(stage.join("saves/level.dat")).unwrap(), b"world");
    }
    #[test]
    fn same_count_wrong_identity_or_missing_required_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let stage = tmp.path().canonicalize().unwrap();
        fs::create_dir(stage.join("mods")).unwrap();
        fs::write(stage.join("mods/a.jar"), b"fixture").unwrap();
        let mut wrong = expected();
        wrong.relative_path = "mods/other.jar".into();
        assert!(persist_staged_manifest(&stage, &pack(), &[wrong], launch()).is_err());
        assert!(!stage.join(INSTALL_MANIFEST_NAME).exists());
    }
    #[test]
    fn upstream_hash_or_required_flag_must_match_normalized_plan() {
        let tmp = tempfile::tempdir().unwrap();
        let stage = tmp.path().canonicalize().unwrap();
        fs::create_dir(stage.join("mods")).unwrap();
        fs::write(stage.join("mods/a.jar"), b"fixture").unwrap();
        let mut wrong = expected();
        wrong.upstream_sha1 = Some("0".repeat(40));
        assert!(persist_staged_manifest(&stage, &pack(), &[wrong], launch()).is_err());
        let mut wrong = expected();
        wrong.required = false;
        assert!(persist_staged_manifest(&stage, &pack(), &[wrong], launch()).is_err());
        assert!(!stage.join(INSTALL_MANIFEST_NAME).exists());
    }
    #[test]
    fn unsupported_ram_or_missing_inventory_never_writes_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let stage = tmp.path().canonicalize().unwrap();
        assert!(persist_staged_manifest(&stage, &pack(), &[], launch()).is_err());
        let mut settings = launch();
        settings.xms_mb = 8192;
        settings.xmx_mb = 1024;
        assert!(persist_staged_manifest(&stage, &pack(), &[expected()], settings).is_err());
        assert!(!stage.join(INSTALL_MANIFEST_NAME).exists());
    }
}
