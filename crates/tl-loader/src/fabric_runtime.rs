//! Fabric runtime library inventory verification and launch metadata assembly.
//! Unlike Forge, Fabric has no mandatory Java patcher processor; however,
//! official profile JSON alone does NOT authenticate downloaded Maven JARs.
//! The caller must supply independently resolved upstream SHA-1 and size,
//! then this module rehashes local files before constructing a launch view.
//! No instance publication, Java execution, or READY transition occurs here.
use crate::{fabric::Fabric, LoaderError, LoaderInstaller, LoaderKind, LoaderSpec};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::Path,
};

const MAX_FABRIC_JAR: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabricLibraryProof {
    pub relative_path: String,
    pub upstream_sha1: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct VerifiedFabricLaunchView {
    pub minecraft_version: String,
    pub loader_version: String,
    pub required_java_major: u32,
    pub merged_version: Value,
    pub verified_libraries: Vec<FabricLibraryProof>,
    /// The downstream publisher still needs Mojang assets/natives, staged
    /// mod inventory, durable receipt anchoring, and a real Play bridge.
    pub instance_ready: bool,
}

fn reject_symlink_components(path: &Path) -> Result<(), LoaderError> {
    if !path.is_absolute() {
        return Err(LoaderError::InvalidInput);
    }
    for part in path.ancestors() {
        if fs::symlink_metadata(part).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(LoaderError::InvalidMetadata(
                "Fabric runtime cache path contains a symlink".into(),
            ));
        }
    }
    Ok(())
}

fn rehash_jar(root: &Path, proof: &FabricLibraryProof) -> Result<(), LoaderError> {
    let expected = &proof.upstream_sha1;
    if expected.len() != 40
        || !expected.bytes().all(|x| x.is_ascii_hexdigit())
        || proof.size_bytes == 0
        || proof.size_bytes > MAX_FABRIC_JAR
    {
        return Err(LoaderError::InvalidMetadata(
            "Fabric library lacks a bounded upstream digest and size".into(),
        ));
    }
    let path = root.join("libraries").join(&proof.relative_path);
    reject_symlink_components(&path)?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    if !metadata.file_type().is_file() || metadata.len() != proof.size_bytes {
        return Err(LoaderError::InvalidMetadata(
            "Fabric library size/type differs from independent proof".into(),
        ));
    }
    let mut source = File::open(&path)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let mut digest = Sha1::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = source
            .read(&mut buffer)
            .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    let actual = format!("{:x}", digest.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(LoaderError::InvalidMetadata(
            "Fabric library rehash differs from upstream proof".into(),
        ));
    }
    Ok(())
}

/// The caller must fetch `/v2/versions/loader/<MC>/<loader>/profile/json`
/// from meta.fabricmc.net, then resolve each library SHA-1 over authenticated
/// HTTPS from the approved official Maven repository. Neither a checksum
/// copied from the local JAR nor an arbitrary caller hash proves provenance.
/// This method cannot itself assert that provenance; publication stays blocked.
pub fn verify_local_fabric_runtime(
    minecraft: &str,
    loader_version: &str,
    parent: &Value,
    fabric_profile_json: &str,
    root: &Path,
    upstream_proofs: &[FabricLibraryProof],
) -> Result<VerifiedFabricLaunchView, LoaderError> {
    crate::validate_component(minecraft)?;
    crate::validate_component(loader_version)?;
    reject_symlink_components(root)?;
    if parent["id"] != minecraft || parent.get("inheritsFrom").is_some() {
        return Err(LoaderError::CompatibilityConflict);
    }
    let required_java = tl_game::required_java(parent)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let spec = LoaderSpec {
        kind: LoaderKind::Fabric,
        version: Some(loader_version.to_owned()),
    };
    let plan = Fabric.plan(minecraft, &spec, fabric_profile_json, required_java)?;
    if !plan.ready_for_metadata_merge || plan.libraries.is_empty() {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    if upstream_proofs.len() != plan.libraries.len() {
        return Err(LoaderError::InvalidMetadata(
            "Fabric Maven inventory missing/extra libraries".into(),
        ));
    }
    let mut lookup = BTreeMap::<&str, &FabricLibraryProof>::new();
    for proof in upstream_proofs {
        if crate::maven::artifact_path_from_relative(&proof.relative_path).is_err() {
            return Err(LoaderError::InvalidInput);
        }
        if lookup.insert(proof.relative_path.as_str(), proof).is_some() {
            return Err(LoaderError::InvalidMetadata(
                "duplicate Fabric library receipt".into(),
            ));
        }
    }
    let mut verified = Vec::new();
    let mut seen = BTreeSet::new();
    let mut profile: Value = serde_json::from_str(fabric_profile_json)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let libraries = profile["libraries"]
        .as_array_mut()
        .ok_or_else(|| LoaderError::InvalidMetadata("Fabric profile lacks libraries".into()))?;
    for (library, artifact) in libraries.iter_mut().zip(&plan.libraries) {
        let proof = lookup.get(artifact.relative_path.as_str()).ok_or_else(|| {
            LoaderError::InvalidMetadata(format!(
                "missing independent SHA-1 for {}",
                artifact.relative_path
            ))
        })?;
        if !seen.insert(artifact.relative_path.as_str()) {
            return Err(LoaderError::InvalidMetadata(
                "duplicate Fabric Maven coordinate".into(),
            ));
        }
        rehash_jar(root, proof)?;
        library["downloads"] = json!({
            "artifact": {
                "path": artifact.relative_path,
                "sha1": proof.upstream_sha1,
                "size": proof.size_bytes,
                "url": artifact.url,
            }
        });
        verified.push((**proof).clone());
    }
    let merged = tl_game::inherit(parent, &profile)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    if merged["mainClass"] != plan.main_class.as_deref().unwrap_or("")
        || merged["id"] != plan.version_id
        || merged["inheritsFrom"] != minecraft
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    Ok(VerifiedFabricLaunchView {
        minecraft_version: minecraft.to_owned(),
        loader_version: loader_version.to_owned(),
        required_java_major: required_java,
        merged_version: merged,
        verified_libraries: verified,
        instance_ready: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};
    fn temp_root() -> std::path::PathBuf {
        let id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("tl-fabric-proof-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    }
    const MC: &str = "1.20.1";
    const LOADER: &str = "0.16.10";
    fn parent() -> Value {
        json!({
            "id":MC,
            "javaVersion":{"majorVersion":17},
            "libraries":[],
            "mainClass":"net.minecraft.client.main.Main",
            "arguments":{"game":[],"jvm":[]},
        })
    }
    fn profile() -> &'static str {
        r#"{"id":"fabric-loader-0.16.10-1.20.1","inheritsFrom":"1.20.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.16.10","url":"https://maven.fabricmc.net/"}]}"#
    }
    #[test]
    fn hashes_local_library_but_does_not_claim_published_instance() {
        let root = temp_root();
        let relative = "net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar";
        let file = root.join("libraries").join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"test fixture only").unwrap();
        let sha = format!("{:x}", Sha1::digest(b"test fixture only"));
        let proofs = vec![FabricLibraryProof {
            relative_path: relative.into(),
            upstream_sha1: sha,
            size_bytes: 17,
        }];
        let out = verify_local_fabric_runtime(MC, LOADER, &parent(), profile(), &root, &proofs).unwrap();
        assert_eq!(out.required_java_major, 17);
        assert_eq!(out.verified_libraries.len(), 1);
        assert_eq!(out.merged_version["mainClass"], "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert!(!out.instance_ready);
        fs::write(&file, b"another fixture!").unwrap();
        assert!(verify_local_fabric_runtime(MC, LOADER, &parent(), profile(), &root, &proofs).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn rejects_missing_proof_wrong_version_and_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        assert!(verify_local_fabric_runtime(MC, LOADER, &parent(), profile(), &root, &[]).is_err());
        assert!(verify_local_fabric_runtime("1.19.2", LOADER, &parent(), profile(), &root, &[]).is_err());
        assert!(crate::maven::artifact_path_from_relative("../../evil.jar").is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
