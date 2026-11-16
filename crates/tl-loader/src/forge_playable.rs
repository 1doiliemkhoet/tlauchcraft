//! Forge P0: strict, version-selected runtime receipt for Chat 4.
//! A prepared processor graph is NEVER a successful execution receipt.
//! Does not run Java, install game files, mutate existing worlds or publish instances.
use crate::{
    forge_1165::merge_forge_1165_version,
    forge_provenance::{
        audit_official_forge_metadata, ArtifactRequirement, SourceRequirement,
    },
    loader_runtime::{verification_from_execution, RuntimeState},
    runtime::{ProcessorRunReport, ProcessorRunRequest, VerifiedProcessorJar},
    LoaderError, LoaderKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Pass this exact contract from Chat 4's normalized modpack manifest.
/// No Forge version is chosen by fixture, default, or previous profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackForgeSelection {
    pub minecraft_version: String,
    pub forge_version: String,
    pub pack_archive_sha256: String,
}

impl PackForgeSelection {
    /// Exact field names match Chat 4's NormalizedPack serialization.
    /// This avoids a circular tl-loader -> tl-pack dependency.
    pub fn from_normalized_pack_json(pack: &Value) -> Result<Self, LoaderError> {
        if pack["loader"] != "forge" {
            return Err(LoaderError::CompatibilityConflict);
        }
        let read = |key: &str| -> Result<String, LoaderError> {
            pack[key]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| LoaderError::InvalidMetadata(format!(
                    "normalized Forge pack missing {key}"
                )))
        };
        let selection = Self {
            minecraft_version: read("minecraft_version")?,
            forge_version: read("loader_version")?,
            pack_archive_sha256: read("archive_sha256")?,
        };
        crate::validate_component(&selection.minecraft_version)?;
        crate::validate_component(&selection.forge_version)?;
        if !valid_sha256(&selection.pack_archive_sha256) {
            return Err(LoaderError::InvalidInput);
        }
        Ok(selection)
    }
}

/// Java 8 executable identity reverified from bytes before a Chat 4 handoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedForgeJava {
    pub executable: PathBuf,
    pub sha1: String,
    pub major: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ForgeLibraryOrigin {
    /// Remote-sourceable library with canonical URL + official digest.
    OfficialMaven,
    /// URL-empty Forge Universal extracted from the pinned official installer,
    /// or the derived runtime generated inside the private stage.
    VerifiedInstallerOrProcessor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedForgeLibrary {
    pub coordinate: String,
    pub path: PathBuf,
    pub official_sha1: String,
    pub size_bytes: u64,
    pub origin: ForgeLibraryOrigin,
}

/// Receipt is a handoff to Chat 4, not proof the game launched.
/// Chat 4 must independently install libraries and verify the inherited launch
/// metadata in a NEW transactional instance, then perform an actual game launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeRuntimeReceipt {
    pub schema: String,
    pub pack_archive_sha256: String,
    pub minecraft_version: String,
    pub forge_version: String,
    pub derived_version_id: String,
    pub required_java_major: u32,
    pub verified_java_sha1: String,
    pub stage_root: PathBuf,
    pub processor_count: usize,
    pub libraries: Vec<VerifiedForgeLibrary>,
    pub inherited_version: Value,
    pub execution_verified: bool,
    pub instance_ready: bool,
    pub game_launched: bool,
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn checked_file(path: &Path, requirement: &ArtifactRequirement) -> Result<(), LoaderError> {
    let metadata = fs::symlink_metadata(path).map_err(|e| {
        LoaderError::InvalidMetadata(format!("missing Forge library {}: {e}", requirement.coordinate))
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != requirement.size_bytes {
        return Err(LoaderError::InvalidMetadata(format!(
            "Forge library size/type mismatch: {}", requirement.coordinate
        )));
    }
    crate::runtime::verify_sha1(path, &requirement.sha1)
}

fn path_is_within(base: &Path, path: &Path) -> bool {
    path != base && path.starts_with(base) && path.is_absolute()
}

/// Bind a user-selected pack to the official installer, rather than silently
/// continuing with a hardcoded fixture version.
pub fn validate_forge_pack_selection(
    selected: &PackForgeSelection,
    profile_json: &str,
    derived: &Value,
) -> Result<(), LoaderError> {
    crate::validate_component(&selected.minecraft_version)?;
    crate::validate_component(&selected.forge_version)?;
    if !valid_sha256(&selected.pack_archive_sha256) {
        return Err(LoaderError::InvalidInput);
    }
    let profile: Value = serde_json::from_str(profile_json)
        .map_err(|e| LoaderError::InvalidMetadata(format!("installer profile: {e}")))?;
    let expected = format!(
        "{}-forge-{}",
        selected.minecraft_version, selected.forge_version
    );
    if profile["minecraft"] != selected.minecraft_version
        || profile["version"] != expected
        || derived["id"] != expected
        || derived["inheritsFrom"] != selected.minecraft_version
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    // The audited runtime implementation currently supports exactly this
    // Minecraft family; other releases need a dedicated reviewed adapter.
    crate::loader_runtime::forge_adapter_for(
        &selected.minecraft_version,
        &selected.forge_version,
    )?;
    let audit = audit_official_forge_metadata(profile_json, derived)?;
    if audit.minecraft_version != selected.minecraft_version
        || audit.derived_version != expected
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    Ok(())
}

/// Produce a usable Chat 4 receipt only with real, complete processor evidence,
/// all official library digests (including URL-empty installer/derived libraries),
/// Java 8 selection, and exact modpack-selected version inheritance.
///
/// Maven inputs are sourced from the importer; installer-generated outputs must
/// be independently supplied and remain inside a fresh isolated stage or pinned
/// installer extraction root. This function does not create any runtime files.
pub fn verify_forge_runtime_for_pack(
    selected: &PackForgeSelection,
    profile_json: &str,
    derived: &Value,
    mojang_parent: &Value,
    request: &ProcessorRunRequest,
    execution: &ProcessorRunReport,
    java_evidence: &VerifiedForgeJava,
    official_maven: &[VerifiedProcessorJar],
    installer_or_generated: &[VerifiedForgeLibrary],
) -> Result<ForgeRuntimeReceipt, LoaderError> {
    validate_forge_pack_selection(selected, profile_json, derived)?;
    if request.minecraft_version != selected.minecraft_version
        || request.processors.len() != 4
        || !request.stage_root.is_absolute()
        || !request.stage_root.is_dir()
        || !request.java.is_absolute()
        || !request.java.is_file()
        || java_evidence.major != 8
        || java_evidence.executable != request.java
        || java_evidence.sha1.len() != 40
        || !java_evidence.sha1.bytes().all(|b| b.is_ascii_hexdigit())
        || mojang_parent["id"] != selected.minecraft_version
    {
        return Err(LoaderError::InvalidInput);
    }
    crate::runtime::verify_sha1(&request.java, &java_evidence.sha1)?;
    let marker = request.stage_root.join(".tl-loader-stage");
    if fs::read(&marker).ok().as_deref() != Some(b"tl-loader-forge-v1") {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    if execution.log_paths.len() != 8 || execution.log_paths.iter().any(|path| {
        !path.starts_with(&request.stage_root) || !path.is_file()
    }) {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    // Reconstruct the actual four-step official Forge client graph and compare
    // every executable, ordered classpath and literal argument before accepting
    // execution evidence supplied by the approved external runner.
    let blueprint = crate::forge_1165::forge_1165_client_blueprint(
        profile_json,
        derived,
        &selected.forge_version,
    )?;
    if blueprint.steps.len() != 4 {
        return Err(LoaderError::InvalidMetadata(
            "unsupported Forge processor graph length".into(),
        ));
    }
    let official_profile: Value = serde_json::from_str(profile_json)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    for (index, (processor, step)) in request.processors.iter().zip(&blueprint.steps).enumerate() {
        if processor.executable.coordinate != step.jar
            || processor.classpath.len() != step.classpath.len()
            || !processor.classpath.iter().zip(&step.classpath).all(|(actual, expected)| {
                actual.coordinate == *expected
            })
            || processor.arguments.len() != step.args.len()
            || !processor.arguments.iter().zip(&step.args).all(|(actual, expected)| {
                if expected.starts_with('{') || expected.starts_with('[') {
                    !actual.is_empty() && !actual.contains('{') && !actual.contains('[')
                } else {
                    actual == expected
                }
            })
        {
            return Err(LoaderError::InvalidMetadata(format!(
                "processor {index} differs from official executable/classpath/argument graph"
            )));
        }
        crate::runtime::validate_processor(processor)?;
        if processor.outputs.len() != step.outputs.len()
            || processor.intermediate_outputs.len() != usize::from(step.outputs.is_empty())
        {
            return Err(LoaderError::InvalidMetadata(format!(
                "processor {index} missing official outputs or intermediate dependencies"
            )));
        }
        for hash_token in step.outputs.values() {
            let key = hash_token
                .strip_prefix('{')
                .and_then(|x| x.strip_suffix('}'))
                .ok_or(LoaderError::InvalidInput)?;
            let expected_sha1 = official_profile["data"][key]["client"]
                .as_str()
                .ok_or(LoaderError::InvalidInput)?
                .trim_matches('\'');
            if !processor.outputs.iter().any(|output| {
                output.sha1.eq_ignore_ascii_case(expected_sha1)
            }) {
                return Err(LoaderError::InvalidMetadata(format!(
                    "processor {index} output hash differs from official client SHA-1"
                )));
            }
        }
    }
    let runtime = verification_from_execution(
        LoaderKind::Forge,
        &selected.minecraft_version,
        &selected.forge_version,
        request,
        execution,
    );
    if runtime.state != RuntimeState::ProcessorsVerified
        || runtime.completed_processors != 4
        || runtime.instance_ready
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let audit = audit_official_forge_metadata(profile_json, derived)?;
    let mut libraries = Vec::new();
    let mut seen = BTreeMap::<String, (String, u64, String, SourceRequirement)>::new();
    let mut maven = BTreeMap::new();
    for item in official_maven {
        if maven.insert(item.coordinate.as_str(), item).is_some() {
            return Err(LoaderError::InvalidMetadata("duplicate Maven evidence".into()));
        }
    }
    let mut generated = BTreeMap::new();
    for item in installer_or_generated {
        if generated.insert(item.coordinate.as_str(), item).is_some() {
            return Err(LoaderError::InvalidMetadata("duplicate generated library evidence".into()));
        }
    }
    for requirement in audit.profile_libraries.iter().chain(audit.derived_libraries.iter()) {
        let descriptor = (
            requirement.sha1.clone(),
            requirement.size_bytes,
            requirement.relative_path.clone(),
            requirement.source.clone(),
        );
        if let Some(previous) = seen.get(&requirement.coordinate) {
            if previous != &descriptor {
                return Err(LoaderError::InvalidMetadata(format!(
                    "conflicting Forge inventory record: {}", requirement.coordinate
                )));
            }
            continue;
        }
        seen.insert(requirement.coordinate.clone(), descriptor);
        let (path, origin) = match &requirement.source {
            SourceRequirement::CanonicalMaven { url } => {
                let evidence = maven.get(requirement.coordinate.as_str()).ok_or_else(|| {
                    LoaderError::InvalidMetadata(format!("missing Maven library: {}", requirement.coordinate))
                })?;
                if evidence.source != *url || !evidence.sha1.eq_ignore_ascii_case(&requirement.sha1) {
                    return Err(LoaderError::InvalidMetadata(format!(
                        "wrong Maven provenance: {}", requirement.coordinate
                    )));
                }
                (&evidence.path, ForgeLibraryOrigin::OfficialMaven)
            }
            SourceRequirement::VerifiedInstallerOrGenerated => {
                let evidence = generated.get(requirement.coordinate.as_str()).ok_or_else(|| {
                    LoaderError::InvalidMetadata(format!(
                        "installer/generated Forge library still missing: {}", requirement.coordinate
                    ))
                })?;
                if !matches!(evidence.origin, ForgeLibraryOrigin::VerifiedInstallerOrProcessor)
                    || evidence.size_bytes != requirement.size_bytes
                    || !evidence.official_sha1.eq_ignore_ascii_case(&requirement.sha1)
                    || !path_is_within(&request.stage_root, &evidence.path)
                {
                    return Err(LoaderError::InvalidMetadata(format!(
                        "unverified generated Forge provenance/stage: {}", requirement.coordinate
                    )));
                }
                (&evidence.path, ForgeLibraryOrigin::VerifiedInstallerOrProcessor)
            }
        };
        checked_file(path, requirement)?;
        libraries.push(VerifiedForgeLibrary {
            coordinate: requirement.coordinate.clone(),
            path: path.clone(),
            official_sha1: requirement.sha1.clone(),
            size_bytes: requirement.size_bytes,
            origin,
        });
    }
    let inherited = merge_forge_1165_version(
        mojang_parent, derived, &selected.forge_version,
    )?;
    if inherited["mainClass"] != "cpw.mods.modlauncher.Launcher" {
        return Err(LoaderError::CompatibilityConflict);
    }
    Ok(ForgeRuntimeReceipt {
        schema: "tl-forge-runtime-receipt/v1".into(),
        pack_archive_sha256: selected.pack_archive_sha256.clone(),
        minecraft_version: selected.minecraft_version.clone(),
        forge_version: selected.forge_version.clone(),
        derived_version_id: audit.derived_version,
        required_java_major: 8,
        verified_java_sha1: java_evidence.sha1.to_ascii_lowercase(),
        stage_root: request.stage_root.clone(),
        processor_count: 4,
        libraries,
        inherited_version: inherited,
        execution_verified: true,
        instance_ready: false,
        game_launched: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const PROFILE: &str = include_str!("../tests/fixtures/forge_36229_official_profile.json");
    const DERIVED: &str = include_str!("../tests/fixtures/forge_36229_official_version.json");
    fn selected(version: &str) -> PackForgeSelection {
        PackForgeSelection {
            minecraft_version: "1.16.5".into(),
            forge_version: version.into(),
            pack_archive_sha256: "a".repeat(64),
        }
    }
    #[test]
    fn manifest_selected_version_must_match_real_official_installer() {
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        validate_forge_pack_selection(&selected("36.2.29"), PROFILE, &derived).unwrap();
        assert!(matches!(
            validate_forge_pack_selection(&selected("36.2.34"), PROFILE, &derived),
            Err(LoaderError::CompatibilityConflict)
        ));
        assert!(validate_forge_pack_selection(&selected("36.2.29"), PROFILE, &json!({"id":"fake"})).is_err());
        let mut bad = selected("36.2.29");
        bad.pack_archive_sha256 = "x".repeat(64);
        assert!(validate_forge_pack_selection(&bad, PROFILE, &derived).is_err());
    }
    #[test]
    fn chat4_normalized_manifest_drives_exact_version_not_fixture_default() {
        let pack = json!({
            "loader": "forge",
            "minecraft_version": "1.16.5",
            "loader_version": "36.2.34",
            "archive_sha256": "a".repeat(64)
        });
        let selection = PackForgeSelection::from_normalized_pack_json(&pack).unwrap();
        assert_eq!(selection.forge_version, "36.2.34");
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        assert!(matches!(
            validate_forge_pack_selection(&selection, PROFILE, &derived),
            Err(LoaderError::CompatibilityConflict)
        ));
        let mut invalid = pack.clone();
        invalid["loader"] = json!("fabric");
        assert!(PackForgeSelection::from_normalized_pack_json(&invalid).is_err());
        invalid["loader"] = json!("forge");
        invalid["archive_sha256"] = json!("invalid");
        assert!(PackForgeSelection::from_normalized_pack_json(&invalid).is_err());
    }

    #[test]
    fn plan_only_must_never_produce_execution_receipt() {
        use crate::runtime::{ProcessorOutcome, ProcessorRunReport};
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        let root = std::env::temp_dir().join(format!("forge-receipt-plan-only-{}", std::process::id()));
        let request = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: root.join("java.exe"),
            stage_root: root.join("stage"),
            processors: vec![],
            timeout_seconds: 90,
        };
        let report = ProcessorRunReport {
            outcome: ProcessorOutcome::Verified,
            completed: 0,
            exit_codes: vec![],
            log_paths: vec![],
            verified_outputs: vec![],
            intermediate_evidence: vec![],
            error: None,
        };
        let parent = json!({"id":"1.16.5"});
        assert!(verify_forge_runtime_for_pack(
            &selected("36.2.29"), PROFILE, &derived, &parent, &request, &report,
            &VerifiedForgeJava { executable: request.java.clone(), sha1: "0".repeat(40), major: 8 },
            &[], &[]
        ).is_err());
        assert!(!root.exists());
    }
}
