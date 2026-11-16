//! Stable installed-pack fingerprint for offline LAN compatibility handoff.
//! Must be computed from the *verified on-disk* inventory, not ZIP names or
//! frontend-supplied completion flags. Not a launcher READY authorization.
use crate::{
    artifact_audit::{audit_staged_artifacts, ExpectedArtifact},
    normalized::NormalizedPack,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFingerprint {
    pub version: u32,
    pub sha256: String,
    pub minecraft_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub verified_files: usize,
    pub ready: bool,
}

/// All listed files must be rehashed and verified. A missing optional mod is
/// a distinct installation, so cannot produce the same compatibility digest.
pub fn fingerprint_staged_pack(
    staging: &Path,
    pack: &NormalizedPack,
    inventory: &[ExpectedArtifact],
) -> Result<PackFingerprint, String> {
    if pack.files.len() != inventory.len() {
        return Err("pack and installed inventory counts differ".into());
    }
    let mut planned = std::collections::HashMap::new();
    for file in &pack.files {
        if planned
            .insert(file.target.to_ascii_lowercase(), file)
            .is_some()
        {
            return Err("duplicate normalized path".into());
        }
    }
    let mut seen = HashSet::new();
    for artifact in inventory {
        let key = artifact.relative_path.to_ascii_lowercase();
        let file = planned.get(&key).ok_or("undeclared installed mod")?;
        if !seen.insert(key) || file.required != artifact.required {
            return Err("duplicate path or required flag mismatch".into());
        }
        if file.sha1.as_ref().is_some_and(|value| {
            artifact.upstream_sha1.as_ref().is_none_or(|actual| !value.eq_ignore_ascii_case(actual))
        }) || file.sha512.as_ref().is_some_and(|value| {
            artifact.upstream_sha512.as_ref().is_none_or(|actual| !value.eq_ignore_ascii_case(actual))
        }) {
            return Err("trusted upstream digest differs from plan".into());
        }
    }
    let audit = audit_staged_artifacts(staging, inventory);
    if !audit.complete || audit.blocked.len() != 0
        || audit.required_verified + audit.optional_verified != inventory.len()
    {
        return Err("all installed mods must be independently verified".into());
    }
    let mut sorted: Vec<_> = inventory.iter().collect();
    sorted.sort_by_key(|item| item.relative_path.to_ascii_lowercase());
    let mut hash = Sha256::new();
    let parts: Vec<&str> = vec![
        "tlauchcraft-installed-pack-v1",
        &pack.archive_sha256,
        &pack.minecraft_version,
        &pack.loader_version,
    ];
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    let loader = format!("{:?}", pack.loader).to_ascii_lowercase();
    hash.update((loader.len() as u64).to_be_bytes());
    hash.update(loader.as_bytes());
    hash.update((sorted.len() as u64).to_be_bytes());
    let verified_files = sorted.len();
    for file in sorted {
        let path = file.relative_path.to_ascii_lowercase();
        let checksum = file.sha256.to_ascii_lowercase();
        for part in [&path, &checksum] {
            hash.update((part.len() as u64).to_be_bytes());
            hash.update(part.as_bytes());
        }
        hash.update(file.bytes.to_be_bytes());
        hash.update([u8::from(file.required)]);
    }
    Ok(PackFingerprint {
        version: 1,
        sha256: format!("{:x}", hash.finalize()),
        minecraft_version: pack.minecraft_version.clone(),
        loader_kind: loader,
        loader_version: pack.loader_version.clone(),
        verified_files,
        ready: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        normalized::{InstallSupport, LoaderKind, NormalizedFile, ProviderKind},
        PackSource,
    };
    use sha1::Sha1;
    fn prepare() -> (tempfile::TempDir, NormalizedPack, Vec<ExpectedArtifact>) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("mods")).unwrap();
        std::fs::write(tmp.path().join("mods/a.jar"), b"verified bytes").unwrap();
        let sha1 = format!("{:x}", Sha1::digest(b"verified bytes"));
        let pack = NormalizedPack {
            name: "Test pack".into(),
            source: PackSource::CurseforgeZip,
            archive_sha256: "a".repeat(64),
            minecraft_version: "1.16.5".into(),
            loader: LoaderKind::Forge,
            loader_version: "36.2.29".into(),
            files: vec![NormalizedFile {
                target: "mods/a.jar".into(), provider: ProviderKind::CurseForge,
                required: true, project_id: Some(100), file_id: Some(200),
                sha1: Some(sha1.clone()), sha512: None, download_urls: vec![],
            }],
            support: InstallSupport::StageOnly,
            reasons: vec![],
        };
        let installed = vec![ExpectedArtifact {
            relative_path: "mods/a.jar".into(),
            required: true,
            bytes: 14,
            sha256: format!("{:x}", Sha256::digest(b"verified bytes")),
            upstream_sha1: Some(sha1), upstream_sha512: None,
        }];
        (tmp, pack, installed)
    }
    #[test]
    fn deterministic_fingerprint_never_authorizes_play() {
        let (tmp, pack, files) = prepare();
        let root = tmp.path().canonicalize().unwrap();
        let a = fingerprint_staged_pack(&root, &pack, &files).unwrap();
        let b = fingerprint_staged_pack(&root, &pack, &files).unwrap();
        assert_eq!(a.sha256, b.sha256);
        assert_eq!(a.verified_files, 1);
        assert!(!a.ready);
    }
    #[test]
    fn tampered_mod_never_generates_compatible_fingerprint() {
        let (tmp, pack, files) = prepare();
        let root = tmp.path().canonicalize().unwrap();
        std::fs::write(root.join("mods/a.jar"), b"tampered data!").unwrap();
        assert!(fingerprint_staged_pack(&root, &pack, &files).is_err());
    }
    #[test]
    fn different_loader_version_produces_different_fingerprint() {
        let (tmp, mut pack, files) = prepare();
        let root = tmp.path().canonicalize().unwrap();
        let a = fingerprint_staged_pack(&root, &pack, &files).unwrap();
        pack.loader_version = "36.2.34".into();
        let b = fingerprint_staged_pack(&root, &pack, &files).unwrap();
        assert_ne!(a.sha256, b.sha256);
    }
}
