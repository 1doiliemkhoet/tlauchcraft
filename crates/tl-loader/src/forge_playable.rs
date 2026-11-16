//! Forge 1.16.5 completion contract for Chat 4's transactional installer.
//! Unlike preflight, this requires *actual verified processor execution* and
//! every URL-less Forge JAR's exact upstream-declared bytes. No Java is spawned,
//! no game is launched and no Minecraft world/profile is touched here.
use crate::{
    forge_1165::{forge_1165_client_blueprint, merge_forge_1165_version},
    forge_provenance::{audit_official_forge_metadata, ArtifactRequirement, SourceRequirement},
    loader_runtime::{verification_from_execution, RuntimeState},
    runtime::{ProcessorRunReport, ProcessorRunRequest, VerifiedProcessorJar},
    LoaderError, LoaderKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

/// Fields originate in Chat4's normalized, verified pack *manifest*, not a
/// hardcoded Forge test fixture or an untrusted display name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackForgeSelection {
    pub minecraft_version: String,
    pub loader_kind: String,
    pub forge_version: String,
    /// Identifies the verified source archive. Prevents mixing receipts from packs.
    pub pack_archive_sha256: String,
}

impl PackForgeSelection {
    pub fn validate(&self) -> Result<(), LoaderError> {
        if self.minecraft_version != "1.16.5"
            || !self.loader_kind.eq_ignore_ascii_case("forge")
            || self.pack_archive_sha256.len() != 64
            || !self
                .pack_archive_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(LoaderError::CompatibilityConflict);
        }
        crate::loader_runtime::forge_adapter_for(&self.minecraft_version, &self.forge_version)?;
        Ok(())
    }

    pub fn version_id(&self) -> Result<String, LoaderError> {
        self.validate()?;
        Ok(format!(
            "{}-forge-{}",
            self.minecraft_version, self.forge_version
        ))
    }
}

/// Source-verified artifacts include BOTH Forge's no-URL/installer-generated
/// jars, reconciled against their recorded official SHA-1 and byte sizes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeInstalledArtifact {
    pub coordinate: String,
    pub path: PathBuf,
    pub sha1: String,
    pub size_bytes: u64,
}

/// Version-scoped, read-only, verified handoff for Chat4/Chat2.
/// IMPORTANT: Not a published instance, and not proof of Minecraft launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgePlayableHandoff {
    pub schema_version: u32,
    pub pack_archive_sha256: String,
    pub minecraft_version: String,
    pub forge_version: String,
    pub version_id: String,
    pub java_major: u32,
    pub derived_main_class: String,
    pub resolved_version: Value,
    pub verified_processor_count: usize,
    pub generated_artifacts: Vec<ForgeInstalledArtifact>,
    pub official_libraries: Vec<ForgeInstalledArtifact>,
    pub stage_root: PathBuf,
    pub can_stage_new_instance: bool,
    pub instance_ready: bool,
    pub java_executed_by_this_function: bool,
}

/// Verify a file freshly on disk. Never follow symlinks in any ancestor.
fn check_artifact(path: &Path, digest: &str, length: u64) -> Result<(), LoaderError> {
    if !path.is_absolute()
        || !path.is_file()
        || digest.len() != 40
        || !digest.bytes().all(|b| b.is_ascii_hexdigit())
        || length == 0
    {
        return Err(LoaderError::InvalidMetadata(
            "generated Forge artifact missing or invalid".into(),
        ));
    }
    if path.ancestors().any(|component| component.is_symlink()) {
        return Err(LoaderError::InvalidMetadata(
            "symlink in Forge artifact path".into(),
        ));
    }
    let actual = fs::metadata(path).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    if actual.len() != length {
        return Err(LoaderError::InvalidMetadata(format!(
            "Forge artifact size mismatch: {}",
            path.display()
        )));
    }
    let mut source = File::open(path).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let mut buf = [0u8; 65536];
    let mut sha = Sha1::new();
    loop {
        let n = source
            .read(&mut buf)
            .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
    }
    if format!("{:x}", sha.finalize()) != digest.to_ascii_lowercase() {
        return Err(LoaderError::InvalidMetadata(format!(
            "Forge artifact SHA-1 mismatch: {}",
            path.display()
        )));
    }
    Ok(())
}

fn safe_maven_path(relative: &str) -> Result<PathBuf, LoaderError> {
    let p = Path::new(relative);
    if relative.is_empty()
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(LoaderError::InvalidMetadata(
            "unsafe official Maven artifact path".into(),
        ));
    }
    Ok(p.to_owned())
}

/// Converts a *real* completed processor job into a Chat4 install receipt.
/// Rejects preflight-only verdicts, fabricated execution counts, wrong modpack
/// Forge version, any corrupted canonical Maven library, URL-less Forge JAR
/// without exact official bytes, unexpected or duplicated artifacts, and
/// conflicting official metadata. Does not publish an instance.
pub fn build_forge_playable_handoff(
    selected: &PackForgeSelection,
    installer_profile: &str,
    derived: &Value,
    mojang_parent: &Value,
    processor_request: &ProcessorRunRequest,
    execution: &ProcessorRunReport,
    maven_roots: &[PathBuf],
) -> Result<ForgePlayableHandoff, LoaderError> {
    let expected_id = selected.version_id()?;
    let blueprint =
        forge_1165_client_blueprint(installer_profile, derived, &selected.forge_version)?;
    if blueprint.version_id != expected_id
        || processor_request.minecraft_version != selected.minecraft_version
        || processor_request.processors.len() != blueprint.steps.len()
        || blueprint.steps.len() != 4
        || !processor_request.stage_root.is_absolute()
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    let checked = verification_from_execution(
        LoaderKind::Forge,
        &selected.minecraft_version,
        &selected.forge_version,
        processor_request,
        execution,
    );
    if checked.state != RuntimeState::ProcessorsVerified
        || checked.completed_processors != 4
        || checked.instance_ready
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    // Explicitly bind every step to the manifest-specified *official* blueprint,
    // not just four arbitrary Java commands reporting exit 0.
    for (step, canonical) in processor_request
        .processors
        .iter()
        .zip(blueprint.steps.iter())
    {
        if step.executable.coordinate != canonical.jar
            || step
                .classpath
                .iter()
                .map(|lib| lib.coordinate.as_str())
                .ne(canonical.classpath.iter().map(String::as_str))
        {
            return Err(LoaderError::InvalidMetadata(
                "processor graph differs from verified installer".into(),
            ));
        }
    }
    let audit = audit_official_forge_metadata(installer_profile, derived)?;
    if audit.derived_version != expected_id || audit.minecraft_version != selected.minecraft_version
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    if maven_roots.is_empty()
        || maven_roots.len() > 16
        || maven_roots
            .iter()
            .any(|p| !p.is_absolute() || !p.is_dir() || p.ancestors().any(|a| a.is_symlink()))
    {
        return Err(LoaderError::InvalidInput);
    }
    let mut all = BTreeMap::<String, ArtifactRequirement>::new();
    for entry in audit
        .profile_libraries
        .iter()
        .chain(&audit.derived_libraries)
    {
        if let Some(prior) = all.insert(entry.coordinate.clone(), entry.clone()) {
            if prior.sha1 != entry.sha1
                || prior.size_bytes != entry.size_bytes
                || prior.relative_path != entry.relative_path
            {
                return Err(LoaderError::InvalidMetadata(
                    "conflicting official Forge Maven artifacts".into(),
                ));
            }
        }
    }
    let mut generated = Vec::new();
    let mut official = Vec::new();
    let mut claimed_files = BTreeSet::new();
    let mut generated_count = 0;
    for requirement in all.values() {
        let relative = safe_maven_path(&requirement.relative_path)?;
        let mut found = None;
        for root in maven_roots {
            let target = root.join(&relative);
            if !target.exists() {
                continue;
            }
            if found.is_some() {
                return Err(LoaderError::InvalidMetadata(
                    "duplicate Maven path in multiple roots".into(),
                ));
            }
            check_artifact(&target, &requirement.sha1, requirement.size_bytes)?;
            found = Some(target);
        }
        let target = found.ok_or_else(|| {
            LoaderError::InvalidMetadata(format!(
                "missing verified Forge runtime/library: {}",
                requirement.coordinate
            ))
        })?;
        if !claimed_files.insert(target.clone()) {
            return Err(LoaderError::InvalidMetadata(
                "same file claimed by multiple Forge artifacts".into(),
            ));
        }
        let item = ForgeInstalledArtifact {
            coordinate: requirement.coordinate.clone(),
            path: target,
            sha1: requirement.sha1.clone(),
            size_bytes: requirement.size_bytes,
        };
        match &requirement.source {
            SourceRequirement::CanonicalMaven { url } => {
                if !url.starts_with("https://") {
                    return Err(LoaderError::InvalidMetadata(
                        "unapproved Maven source URL".into(),
                    ));
                }
                official.push(item);
            }
            SourceRequirement::VerifiedInstallerOrGenerated => {
                generated_count += 1;
                generated.push(item);
            }
        }
    }
    if generated_count != 2 {
        return Err(LoaderError::InvalidMetadata(
            "expected both Forge derived and universal JARs from installer/execution".into(),
        ));
    }
    // Parent must itself be authentic verified Mojang 1.16.5 metadata. The
    // caller (Chat 4) must separately verify parent assets/natives and stage.
    if mojang_parent["id"].as_str() != Some(selected.minecraft_version.as_str())
        || mojang_parent["downloads"]["client"]["sha1"]
            .as_str()
            .is_none()
        || mojang_parent["assetIndex"]["sha1"].as_str().is_none()
    {
        return Err(LoaderError::InvalidMetadata(
            "missing trusted Mojang parent client/asset metadata".into(),
        ));
    }
    let merged = merge_forge_1165_version(mojang_parent, derived, &selected.forge_version)?;
    if merged["mainClass"] != "cpw.mods.modlauncher.Launcher"
        || merged["id"] != expected_id
        || merged["inheritsFrom"] != selected.minecraft_version
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    Ok(ForgePlayableHandoff {
        schema_version: 1,
        pack_archive_sha256: selected.pack_archive_sha256.clone(),
        minecraft_version: selected.minecraft_version.clone(),
        forge_version: selected.forge_version.clone(),
        version_id: expected_id,
        java_major: 8,
        derived_main_class: "cpw.mods.modlauncher.Launcher".into(),
        resolved_version: merged,
        verified_processor_count: 4,
        generated_artifacts: generated,
        official_libraries: official,
        stage_root: processor_request.stage_root.clone(),
        can_stage_new_instance: true,
        instance_ready: false,
        java_executed_by_this_function: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROFILE: &str = include_str!("../tests/fixtures/forge_36229_official_profile.json");
    const DERIVED: &str = include_str!("../tests/fixtures/forge_36229_official_version.json");
    fn selection(version: &str) -> PackForgeSelection {
        PackForgeSelection {
            minecraft_version: "1.16.5".into(),
            loader_kind: "forge".into(),
            forge_version: version.into(),
            pack_archive_sha256: "a".repeat(64),
        }
    }
    #[test]
    fn accepts_only_manifest_version_and_matching_upstream_metadata() {
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        assert_eq!(
            selection("36.2.29").version_id().unwrap(),
            "1.16.5-forge-36.2.29"
        );
        assert!(selection("36.2.34").version_id().is_ok());
        assert!(selection("36.2.34")
            .version_id()
            .unwrap()
            .ends_with("36.2.34"));
        assert!(forge_1165_client_blueprint(PROFILE, &derived, "36.2.34").is_err());
        assert!(selection("47.1.0").validate().is_err());
        assert!(selection("36.2.29").validate().is_ok());
    }
    #[test]
    fn plan_only_never_yields_publish_receipt() {
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        let request = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: PathBuf::from("C:/Java8/java.exe"),
            stage_root: PathBuf::from("C:/fresh-stage"),
            processors: vec![],
            timeout_seconds: 120,
        };
        let report = ProcessorRunReport {
            outcome: crate::runtime::ProcessorOutcome::Verified,
            completed: 0,
            exit_codes: vec![],
            log_paths: vec![],
            verified_outputs: vec![],
            intermediate_evidence: vec![],
            error: None,
        };
        assert!(build_forge_playable_handoff(
            &selection("36.2.29"),
            PROFILE,
            &derived,
            &serde_json::json!({"id":"1.16.5"}),
            &request,
            &report,
            &[]
        )
        .is_err());
    }
    #[test]
    fn staging_artifacts_require_strict_official_digest_and_size() {
        let root =
            std::env::temp_dir().join(format!("tl03-playable-artifact-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("fake.jar");
        std::fs::write(&file, b"bad").unwrap();
        assert!(check_artifact(&file, "0123456789012345678901234567890123456789", 3).is_err());
        assert!(check_artifact(&file, "0123456789012345678901234567890123456789", 4).is_err());
        assert!(safe_maven_path("../world/saves").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
