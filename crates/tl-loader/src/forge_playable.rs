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
    collections::{BTreeMap, BTreeSet},
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        || mojang_parent["id"] != selected.minecraft_version
    {
        return Err(LoaderError::InvalidInput);
    }
    let expected_order = [
        "net.minecraftforge:installertools:1.2.6",
        "net.minecraftforge:jarsplitter:1.1.4",
        "net.md-5:SpecialSource:1.8.5",
        "net.minecraftforge:binarypatcher:1.0.12",
    ];
    for (processor, expected) in request.processors.iter().zip(expected_order) {
        if processor.executable.coordinate != expected {
            return Err(LoaderError::InvalidMetadata("wrong Forge processor graph/order".into()));
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
    let mut seen = BTreeSet::new();
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
        if !seen.insert(requirement.coordinate.as_str()) {
            return Err(LoaderError::InvalidMetadata("duplicate official Forge coordinate".into()));
        }
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
            &selected("36.2.29"), PROFILE, &derived, &parent, &request, &report, &[], &[]
        ).is_err());
        assert!(!root.exists());
    }
}
