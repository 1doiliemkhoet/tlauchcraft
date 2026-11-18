//! Forge P0: strict, version-selected runtime receipt for Chat 4.
//! A prepared processor graph is NEVER a successful execution receipt.
//! Does not run Java, install game files, mutate existing worlds or publish instances.
use crate::{
    forge_1165::merge_forge_1165_version,
    forge_provenance::{audit_official_forge_metadata, ArtifactRequirement, SourceRequirement},
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
                .ok_or_else(|| {
                    LoaderError::InvalidMetadata(format!("normalized Forge pack missing {key}"))
                })
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
    /// Official immutable Maven-relative destination under Chat 4's new instance.
    pub relative_path: String,
    pub path: PathBuf,
    /// Integrity SHA-1: upstream pinned for Maven/installer outputs; for
    /// SpecialSource SRG only, observed from the approved processor run.
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
    /// Unmodified merged Mojang+Forge metadata for independent auditing.
    pub inherited_version: Value,
    /// Java 8-compatible launch view; Chat 4 supplies this to tl-game.
    pub java8_launch_version: Value,
    pub execution_verified: bool,
    pub instance_ready: bool,
    pub game_launched: bool,
}

/// Preserve official metadata for provenance but strip only Java 9+ module
/// options from the Java 8 launch view. Forge's main class and game arguments
/// remain unchanged. This does not launch a JVM or modify a player instance.
pub fn forge_java8_launch_version(merged: &Value) -> Result<Value, LoaderError> {
    if merged["mainClass"] != "cpw.mods.modlauncher.Launcher"
        || merged["inheritsFrom"] != "1.16.5"
        || merged["id"]
            .as_str()
            .is_none_or(|id| !id.starts_with("1.16.5-forge-"))
        || merged
            .get("javaVersion")
            .is_some_and(|value| value["majorVersion"].as_u64() != Some(8))
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    let mut launch = merged.clone();
    // Mojang's old 1.16.5 metadata can omit javaVersion entirely.
    // This is a scoped Forge 1.16.5 Java 8 mapping, NOT an arbitrary default.
    launch["javaVersion"] = serde_json::json!({
        "component": "jre-legacy",
        "majorVersion": 8
    });
    let entries = launch["arguments"]["jvm"]
        .as_array_mut()
        .ok_or_else(|| LoaderError::InvalidMetadata("inherited JVM arguments missing".into()))?;
    let mut filtered = Vec::new();
    let mut skip_value = false;
    for item in entries.iter() {
        if skip_value {
            skip_value = false;
            continue;
        }
        let Some(text) = item.as_str() else {
            // Preserve Mojang rule objects so tl-game can evaluate OS rules.
            filtered.push(item.clone());
            continue;
        };
        if matches!(
            text,
            "--add-exports" | "--add-opens" | "--add-reads" | "--add-modules"
        ) {
            skip_value = true;
            continue;
        }
        if [
            "--add-exports=",
            "--add-opens=",
            "--add-reads=",
            "--add-modules=",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        {
            continue;
        }
        filtered.push(item.clone());
    }
    if skip_value {
        return Err(LoaderError::InvalidMetadata(
            "trailing Java module option".into(),
        ));
    }
    *entries = filtered;
    Ok(launch)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn checked_file(path: &Path, requirement: &ArtifactRequirement) -> Result<(), LoaderError> {
    let metadata = fs::symlink_metadata(path).map_err(|e| {
        LoaderError::InvalidMetadata(format!(
            "missing Forge library {}: {e}",
            requirement.coordinate
        ))
    })?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != requirement.size_bytes
    {
        return Err(LoaderError::InvalidMetadata(format!(
            "Forge library size/type mismatch: {}",
            requirement.coordinate
        )));
    }
    crate::runtime::verify_sha1(path, &requirement.sha1)
}

fn path_is_within(base: &Path, path: &Path) -> bool {
    path != base && path.starts_with(base) && path.is_absolute()
}

/// The patched Minecraft+Forge client is an output of the official final
/// binarypatcher processor. It is deliberately absent from version.json's
/// declared Maven libraries, but FML requires it at this Maven coordinate.
/// Accept only the output whose exact SHA-1 was pinned by install_profile.json.
fn verified_patched_client(
    selected: &PackForgeSelection,
    profile: &Value,
    request: &ProcessorRunRequest,
    report: &ProcessorRunReport,
) -> Result<VerifiedForgeLibrary, LoaderError> {
    let pinned = profile["data"]["PATCHED_SHA"]["client"]
        .as_str()
        .ok_or_else(|| LoaderError::InvalidMetadata("official PATCHED_SHA missing".into()))?
        .trim_matches('\'');
    let final_processor = request
        .processors
        .last()
        .ok_or(LoaderError::NeedsInstallerExecution)?;
    if final_processor.executable.coordinate != "net.minecraftforge:binarypatcher:1.0.12"
        || final_processor.outputs.len() != 1
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let output = &final_processor.outputs[0];
    if output.relative_path != "generated/forge-client.jar"
        || !output.sha1.eq_ignore_ascii_case(pinned)
    {
        return Err(LoaderError::InvalidMetadata(
            "patched Forge client output differs from official processor contract".into(),
        ));
    }
    let source = request.stage_root.join(&output.relative_path);
    if !path_is_within(&request.stage_root, &source)
        || !report
            .verified_outputs
            .iter()
            .any(|verified| verified == &source)
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let metadata = fs::symlink_metadata(&source)
        .map_err(|e| LoaderError::InvalidMetadata(format!("patched Forge client missing: {e}")))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(LoaderError::InvalidMetadata(
            "patched Forge client is empty, a symlink or not a regular file".into(),
        ));
    }
    crate::runtime::verify_sha1(&source, pinned)?;
    let forge_minecraft_version =
        format!("{}-{}", selected.minecraft_version, selected.forge_version);
    let relative_path = format!(
        "net/minecraftforge/forge/{forge_minecraft_version}/forge-{forge_minecraft_version}-client.jar"
    );
    Ok(VerifiedForgeLibrary {
        coordinate: format!("net.minecraftforge:forge:{forge_minecraft_version}:client"),
        relative_path,
        path: source,
        official_sha1: pinned.to_owned(),
        size_bytes: metadata.len(),
        origin: ForgeLibraryOrigin::VerifiedInstallerOrProcessor,
    })
}

/// Forge FML validates a second Minecraft resource at startup: the
/// jarsplitter-generated client EXTRA JAR. The official installer pins its
/// digest in MC_EXTRA_SHA.client but never lists it in version.json Maven libs.
/// Supply only output verified from the *second* authentic jarsplitter step.
fn verified_minecraft_extra(
    selected: &PackForgeSelection,
    profile: &Value,
    request: &ProcessorRunRequest,
    report: &ProcessorRunReport,
) -> Result<VerifiedForgeLibrary, LoaderError> {
    let pinned = profile["data"]["MC_EXTRA_SHA"]["client"]
        .as_str()
        .ok_or_else(|| LoaderError::InvalidMetadata("official MC_EXTRA_SHA missing".into()))?
        .trim_matches('\'');
    let mcp_version = profile["data"]["MCP_VERSION"]["client"]
        .as_str()
        .ok_or_else(|| LoaderError::InvalidMetadata("official MCP_VERSION missing".into()))?
        .trim_matches('\'');
    if mcp_version.is_empty()
        || mcp_version.len() > 64
        || !mcp_version.bytes().all(|c| c.is_ascii_digit() || c == b'.')
    {
        return Err(LoaderError::InvalidMetadata(
            "unsafe official MCP version".into(),
        ));
    }
    let splitter = request
        .processors
        .get(1)
        .ok_or(LoaderError::NeedsInstallerExecution)?;
    if splitter.executable.coordinate != "net.minecraftforge:jarsplitter:1.1.4"
        || splitter.outputs.len() != 2
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let output = splitter
        .outputs
        .iter()
        .find(|item| item.relative_path == "generated/client-extra.jar")
        .ok_or(LoaderError::NeedsInstallerExecution)?;
    if !output.sha1.eq_ignore_ascii_case(pinned) {
        return Err(LoaderError::InvalidMetadata(
            "jarsplitter extra JAR digest differs from official installer".into(),
        ));
    }
    let source = request.stage_root.join(&output.relative_path);
    if !path_is_within(&request.stage_root, &source)
        || !report
            .verified_outputs
            .iter()
            .any(|verified| verified == &source)
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let metadata = fs::symlink_metadata(&source).map_err(|e| {
        LoaderError::InvalidMetadata(format!("Minecraft client extra missing: {e}"))
    })?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(LoaderError::InvalidMetadata(
            "Minecraft client extra JAR is empty, a symlink or not a regular file".into(),
        ));
    }
    crate::runtime::verify_sha1(&source, pinned)?;
    let version = format!("{}-{mcp_version}", selected.minecraft_version);
    let relative_path = format!("net/minecraft/client/{version}/client-{version}-extra.jar");
    Ok(VerifiedForgeLibrary {
        coordinate: format!("net.minecraft:client:{version}:extra"),
        relative_path,
        path: source,
        official_sha1: pinned.to_owned(),
        size_bytes: metadata.len(),
        origin: ForgeLibraryOrigin::VerifiedInstallerOrProcessor,
    })
}

/// jarsplitter produces both SLIM and EXTRA resources, each with an upstream
/// SHA-1 pinned in install_profile.json. Neither appears as a Maven dependency
/// in the standard version JSON, yet they are useful immutable runtime outputs.
fn verified_minecraft_slim(
    selected: &PackForgeSelection,
    profile: &Value,
    request: &ProcessorRunRequest,
    report: &ProcessorRunReport,
) -> Result<VerifiedForgeLibrary, LoaderError> {
    let pinned = profile["data"]["MC_SLIM_SHA"]["client"]
        .as_str()
        .ok_or_else(|| LoaderError::InvalidMetadata("official MC_SLIM_SHA missing".into()))?
        .trim_matches('\'');
    let mcp_version = official_mcp_version(profile)?;
    let splitter = request
        .processors
        .get(1)
        .ok_or(LoaderError::NeedsInstallerExecution)?;
    if splitter.executable.coordinate != "net.minecraftforge:jarsplitter:1.1.4"
        || splitter.outputs.len() != 2
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let output = splitter
        .outputs
        .iter()
        .find(|output| output.relative_path == "generated/client-slim.jar")
        .ok_or(LoaderError::NeedsInstallerExecution)?;
    if !output.sha1.eq_ignore_ascii_case(pinned) {
        return Err(LoaderError::InvalidMetadata(
            "jarsplitter slim SHA-1 differs from official installer".into(),
        ));
    }
    let source = request.stage_root.join(&output.relative_path);
    if !path_is_within(&request.stage_root, &source)
        || !report
            .verified_outputs
            .iter()
            .any(|verified| verified == &source)
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let metadata = fs::symlink_metadata(&source)
        .map_err(|e| LoaderError::InvalidMetadata(format!("Minecraft client slim missing: {e}")))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(LoaderError::InvalidMetadata(
            "Minecraft client slim JAR is not a nonempty regular file".into(),
        ));
    }
    crate::runtime::verify_sha1(&source, pinned)?;
    let version = format!("{}-{mcp_version}", selected.minecraft_version);
    Ok(VerifiedForgeLibrary {
        coordinate: format!("net.minecraft:client:{version}:slim"),
        relative_path: format!("net/minecraft/client/{version}/client-{version}-slim.jar"),
        path: source,
        official_sha1: pinned.to_owned(),
        size_bytes: metadata.len(),
        origin: ForgeLibraryOrigin::VerifiedInstallerOrProcessor,
    })
}

/// The SpecialSource processor's --out-jar is a generated INTERMEDIATE.
/// Forge's official profile does NOT provide an expected MC_SRG_SHA; the
/// runner records an observed SHA-1 after executing this approved processor.
/// Bind that evidence to its exact output identity, then re-hash the same
/// immutable bytes at publication and Play. Do not claim this SHA-1 is upstream.
fn verified_minecraft_srg(
    selected: &PackForgeSelection,
    profile: &Value,
    request: &ProcessorRunRequest,
    report: &ProcessorRunReport,
) -> Result<VerifiedForgeLibrary, LoaderError> {
    let mcp_version = official_mcp_version(profile)?;
    let processor = request
        .processors
        .get(2)
        .ok_or(LoaderError::NeedsInstallerExecution)?;
    if processor.executable.coordinate != "net.md-5:SpecialSource:1.8.5"
        || processor.outputs.len() != 0
        || processor.intermediate_outputs.len() != 1
        || processor.intermediate_outputs[0] != "generated/client-srg.jar"
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let matches = report
        .intermediate_evidence
        .iter()
        .filter(|item| item.producer_index == 2 && item.relative_path == "generated/client-srg.jar")
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let evidence = matches[0];
    if evidence.size_bytes == 0
        || evidence.observed_sha1.len() != 40
        || !evidence
            .observed_sha1
            .bytes()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Err(LoaderError::InvalidMetadata(
            "SpecialSource observed SRG identity/digest invalid".into(),
        ));
    }
    let source = request.stage_root.join(&evidence.relative_path);
    if !path_is_within(&request.stage_root, &source) {
        return Err(LoaderError::InvalidInput);
    }
    let metadata = fs::symlink_metadata(&source)
        .map_err(|e| LoaderError::InvalidMetadata(format!("Minecraft client SRG missing: {e}")))?;
    if !metadata.file_type().is_file() || metadata.len() != evidence.size_bytes {
        return Err(LoaderError::InvalidMetadata(
            "Minecraft client SRG size or file type differs from processor evidence".into(),
        ));
    }
    crate::runtime::verify_sha1(&source, &evidence.observed_sha1)?;
    let version = format!("{}-{mcp_version}", selected.minecraft_version);
    Ok(VerifiedForgeLibrary {
        coordinate: format!("net.minecraft:client:{version}:srg"),
        relative_path: format!("net/minecraft/client/{version}/client-{version}-srg.jar"),
        path: source,
        // Stored as a digest for independent rehash, NOT an official pinned SHA.
        official_sha1: evidence.observed_sha1.clone(),
        size_bytes: metadata.len(),
        origin: ForgeLibraryOrigin::VerifiedInstallerOrProcessor,
    })
}

fn official_mcp_version(profile: &Value) -> Result<&str, LoaderError> {
    let mcp_version = profile["data"]["MCP_VERSION"]["client"]
        .as_str()
        .ok_or_else(|| LoaderError::InvalidMetadata("official MCP_VERSION missing".into()))?
        .trim_matches('\'');
    if mcp_version.is_empty()
        || mcp_version.len() > 64
        || !mcp_version.bytes().all(|c| c.is_ascii_digit() || c == b'.')
    {
        return Err(LoaderError::InvalidMetadata(
            "unsafe official MCP version".into(),
        ));
    }
    Ok(mcp_version)
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
    crate::loader_runtime::forge_adapter_for(&selected.minecraft_version, &selected.forge_version)?;
    let audit = audit_official_forge_metadata(profile_json, derived)?;
    if audit.minecraft_version != selected.minecraft_version || audit.derived_version != expected {
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
// Each argument is a separately verified, security-sensitive source. Keeping
// the nine explicit inputs prevents a single untrusted JSON bundle from being
// misrepresented as a trusted execution permit. This function never runs Java.
#[allow(clippy::too_many_arguments)]
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
    if fs::read(&marker).ok().as_deref() != Some(&b"tl-loader-forge-v1"[..]) {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    if execution.log_paths.len() != 8
        || execution
            .log_paths
            .iter()
            .any(|path| !path.starts_with(&request.stage_root) || !path.is_file())
    {
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
            || !processor
                .classpath
                .iter()
                .zip(&step.classpath)
                .all(|(actual, expected)| actual.coordinate == *expected)
            || processor.arguments.len() != step.args.len()
            || !processor
                .arguments
                .iter()
                .zip(&step.args)
                .all(|(actual, expected)| {
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
            if !processor
                .outputs
                .iter()
                .any(|output| output.sha1.eq_ignore_ascii_case(expected_sha1))
            {
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
            return Err(LoaderError::InvalidMetadata(
                "duplicate Maven evidence".into(),
            ));
        }
    }
    let mut generated = BTreeMap::new();
    for item in installer_or_generated {
        if generated.insert(item.coordinate.as_str(), item).is_some() {
            return Err(LoaderError::InvalidMetadata(
                "duplicate generated library evidence".into(),
            ));
        }
    }
    for requirement in audit
        .profile_libraries
        .iter()
        .chain(audit.derived_libraries.iter())
    {
        let descriptor = (
            requirement.sha1.clone(),
            requirement.size_bytes,
            requirement.relative_path.clone(),
            requirement.source.clone(),
        );
        if let Some(previous) = seen.get(&requirement.coordinate) {
            if previous != &descriptor {
                return Err(LoaderError::InvalidMetadata(format!(
                    "conflicting Forge inventory record: {}",
                    requirement.coordinate
                )));
            }
            continue;
        }
        seen.insert(requirement.coordinate.clone(), descriptor);
        let (path, origin) = match &requirement.source {
            SourceRequirement::CanonicalMaven { url } => {
                let evidence = maven.get(requirement.coordinate.as_str()).ok_or_else(|| {
                    LoaderError::InvalidMetadata(format!(
                        "missing Maven library: {}",
                        requirement.coordinate
                    ))
                })?;
                if evidence.source != *url || !evidence.sha1.eq_ignore_ascii_case(&requirement.sha1)
                {
                    return Err(LoaderError::InvalidMetadata(format!(
                        "wrong Maven provenance: {}",
                        requirement.coordinate
                    )));
                }
                (&evidence.path, ForgeLibraryOrigin::OfficialMaven)
            }
            SourceRequirement::VerifiedInstallerOrGenerated => {
                let evidence = generated
                    .get(requirement.coordinate.as_str())
                    .ok_or_else(|| {
                        LoaderError::InvalidMetadata(format!(
                            "installer/generated Forge library still missing: {}",
                            requirement.coordinate
                        ))
                    })?;
                if !matches!(
                    evidence.origin,
                    ForgeLibraryOrigin::VerifiedInstallerOrProcessor
                ) || evidence.relative_path != requirement.relative_path
                    || evidence.size_bytes != requirement.size_bytes
                    || !evidence
                        .official_sha1
                        .eq_ignore_ascii_case(&requirement.sha1)
                    || !path_is_within(&request.stage_root, &evidence.path)
                {
                    return Err(LoaderError::InvalidMetadata(format!(
                        "unverified generated Forge provenance/stage: {}",
                        requirement.coordinate
                    )));
                }
                (
                    &evidence.path,
                    ForgeLibraryOrigin::VerifiedInstallerOrProcessor,
                )
            }
        };
        checked_file(path, requirement)?;
        libraries.push(VerifiedForgeLibrary {
            coordinate: requirement.coordinate.clone(),
            relative_path: requirement.relative_path.clone(),
            path: path.clone(),
            official_sha1: requirement.sha1.clone(),
            size_bytes: requirement.size_bytes,
            origin,
        });
    }
    // A successful binarypatcher output is needed by FML at launch but is not
    // listed in the installer/derived Maven library arrays. Include it in the
    // trusted inventory so tl-pack copies, publishes and later re-hashes it.
    let patched_client = verified_patched_client(selected, &official_profile, request, execution)?;
    if libraries
        .iter()
        .any(|library| library.relative_path == patched_client.relative_path)
    {
        return Err(LoaderError::InvalidMetadata(
            "patched Forge client conflicts with a declared Maven library".into(),
        ));
    }
    libraries.push(patched_client);
    let minecraft_extra =
        verified_minecraft_extra(selected, &official_profile, request, execution)?;
    if libraries
        .iter()
        .any(|library| library.relative_path == minecraft_extra.relative_path)
    {
        return Err(LoaderError::InvalidMetadata(
            "Minecraft client extra conflicts with declared Maven library".into(),
        ));
    }
    libraries.push(minecraft_extra);
    // Audit the full set of client generated JARs now, rather than allowing
    // FML to discover one missing resource per operator launch attempt.
    for generated in [
        verified_minecraft_slim(selected, &official_profile, request, execution)?,
        verified_minecraft_srg(selected, &official_profile, request, execution)?,
    ] {
        if libraries
            .iter()
            .any(|library| library.relative_path == generated.relative_path)
        {
            return Err(LoaderError::InvalidMetadata(
                "Minecraft generated resource collides with Maven inventory".into(),
            ));
        }
        libraries.push(generated);
    }
    let inherited = merge_forge_1165_version(mojang_parent, derived, &selected.forge_version)?;
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
        java8_launch_version: forge_java8_launch_version(&inherited)?,
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
        assert!(validate_forge_pack_selection(
            &selected("36.2.29"),
            PROFILE,
            &json!({"id":"fake"})
        )
        .is_err());
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
    fn real_1165_inheritance_exposes_forge_main_class_game_and_jvm_args() {
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        let parent = json!({
            "id": "1.16.5",
            "javaVersion": {"majorVersion": 8},
            "libraries": [{"name": "org.example:parent:1.0"}],
            "downloads": {"client": {
                "url": "https://launcher.mojang.com/v1/objects/37fd3c903861eeff3bc24b71eed48f828b5269c8/client.jar",
                "sha1": "37fd3c903861eeff3bc24b71eed48f828b5269c8",
                "size": 17547153
            }},
            "arguments": {
                "game": ["--username", "${auth_player_name}"],
                "jvm": ["-Dparent=present"]
            }
        });
        let merged = merge_forge_1165_version(&parent, &derived, "36.2.29").unwrap();
        let env = tl_game::Environment {
            os: "windows".into(),
            arch: "x86_64".into(),
            os_version: String::new(),
            features: std::collections::HashMap::new(),
        };
        assert_eq!(
            tl_game::main_class(&merged).unwrap(),
            "cpw.mods.modlauncher.Launcher"
        );
        assert_eq!(tl_game::required_java(&merged).unwrap(), 8);
        let game_args = tl_game::game_arguments(&merged, &env).unwrap();
        assert!(game_args
            .windows(2)
            .any(|v| { v[0] == "--launchTarget" && v[1] == "fmlclient" }));
        assert!(game_args
            .windows(2)
            .any(|v| { v[0] == "--fml.forgeVersion" && v[1] == "36.2.29" }));
        let jvm = tl_game::jvm_arguments(&merged, &env).unwrap();
        assert!(jvm.iter().any(|x| x == "-Dparent=present"));
        assert!(jvm.iter().any(|x| x.starts_with("--add-exports=")));
        assert!(tl_game::libraries(&merged, &env)
            .iter()
            .any(|x| { x["name"] == "net.minecraftforge:forge:1.16.5-36.2.29" }));
        let launch = forge_java8_launch_version(&merged).unwrap();
        assert_eq!(
            tl_game::main_class(&launch).unwrap(),
            "cpw.mods.modlauncher.Launcher"
        );
        assert_eq!(tl_game::game_arguments(&launch, &env).unwrap(), game_args);
        let launch_jvm = tl_game::jvm_arguments(&launch, &env).unwrap();
        assert!(launch_jvm.iter().any(|x| x == "-Dparent=present"));
        assert!(!launch_jvm.iter().any(|x| x.starts_with("--add-exports")
            || x.starts_with("--add-opens")
            || x.starts_with("--add-modules")));
        assert!(forge_java8_launch_version(&json!({
            "mainClass":"net.minecraft.Main", "javaVersion":{"majorVersion":8},
            "arguments":{"jvm":[]}
        }))
        .is_err());
        let mut legacy = merged.clone();
        legacy.as_object_mut().unwrap().remove("javaVersion");
        assert!(tl_game::required_java(&legacy).is_err());
        let mapped = forge_java8_launch_version(&legacy).unwrap();
        assert_eq!(tl_game::required_java(&mapped).unwrap(), 8);
        let mut malformed = merged.clone();
        malformed["javaVersion"]["majorVersion"] = json!("8");
        assert!(forge_java8_launch_version(&malformed).is_err());
    }

    #[test]
    fn client_srg_and_slim_outputs_require_real_processor_evidence() {
        use crate::runtime::{
            ApprovedProcessor, ExpectedOutput, IntermediateEvidence, ProcessorOutcome,
        };
        use sha1::{Digest, Sha1};
        let root =
            std::env::temp_dir().join(format!("forge-generated-srg-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let stage = root.join("forge-execution").join("test-op");
        fs::create_dir_all(stage.join("generated")).unwrap();
        let srg = stage.join("generated/client-srg.jar");
        let slim = stage.join("generated/client-slim.jar");
        let srg_bytes = b"SpecialSource generated SRG intermediate";
        let slim_bytes = b"official jarsplitter SLIM output";
        fs::write(&srg, srg_bytes).unwrap();
        fs::write(&slim, slim_bytes).unwrap();
        let srg_sha = format!("{:x}", Sha1::digest(srg_bytes));
        let slim_sha = format!("{:x}", Sha1::digest(slim_bytes));
        let profile = json!({"data":{
            "MCP_VERSION":{"client":"'20210115.111550'"},
            "MC_SLIM_SHA":{"client":format!("'{slim_sha}'")}
        }});
        let processor = |coordinate: &str| ApprovedProcessor {
            executable: VerifiedProcessorJar {
                coordinate: coordinate.into(),
                path: root.join("processor.jar"),
                sha1: "a".repeat(40),
                source: String::new(),
            },
            classpath: vec![],
            main_class: "example.Processor".into(),
            arguments: vec![],
            outputs: vec![],
            intermediate_outputs: vec![],
        };
        let mut request = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: root.join("java.exe"),
            stage_root: stage,
            processors: vec![
                processor("net.minecraftforge:installertools:1.2.6"),
                processor("net.minecraftforge:jarsplitter:1.1.4"),
                processor("net.md-5:SpecialSource:1.8.5"),
            ],
            timeout_seconds: 180,
        };
        request.processors[1].outputs = vec![
            ExpectedOutput {
                relative_path: "generated/client-extra.jar".into(),
                sha1: "a".repeat(40),
            },
            ExpectedOutput {
                relative_path: "generated/client-slim.jar".into(),
                sha1: slim_sha.clone(),
            },
        ];
        request.processors[2].intermediate_outputs = vec!["generated/client-srg.jar".into()];
        let mut report = ProcessorRunReport {
            outcome: ProcessorOutcome::Verified,
            completed: 3,
            exit_codes: vec![Some(0); 3],
            log_paths: vec![],
            verified_outputs: vec![slim.clone()],
            intermediate_evidence: vec![IntermediateEvidence {
                producer_index: 2,
                relative_path: "generated/client-srg.jar".into(),
                observed_sha1: srg_sha.clone(),
                size_bytes: srg_bytes.len() as u64,
            }],
            error: None,
        };
        let selected = selected("36.2.29");
        let srg_result = verified_minecraft_srg(&selected, &profile, &request, &report).unwrap();
        assert_eq!(
            srg_result.relative_path,
            "net/minecraft/client/1.16.5-20210115.111550/client-1.16.5-20210115.111550-srg.jar"
        );
        assert_eq!(srg_result.official_sha1, srg_sha);
        let slim_result = verified_minecraft_slim(&selected, &profile, &request, &report).unwrap();
        assert_eq!(
            slim_result.relative_path,
            "net/minecraft/client/1.16.5-20210115.111550/client-1.16.5-20210115.111550-slim.jar"
        );
        assert_eq!(slim_result.official_sha1, slim_sha);

        fs::write(&srg, b"tampered").unwrap();
        assert!(verified_minecraft_srg(&selected, &profile, &request, &report).is_err());
        fs::write(&srg, srg_bytes).unwrap();
        report.intermediate_evidence.clear();
        assert!(verified_minecraft_srg(&selected, &profile, &request, &report).is_err());
        report.intermediate_evidence.push(IntermediateEvidence {
            producer_index: 1,
            relative_path: "generated/client-srg.jar".into(),
            observed_sha1: srg_sha.clone(),
            size_bytes: srg_bytes.len() as u64,
        });
        assert!(verified_minecraft_srg(&selected, &profile, &request, &report).is_err());
        report.intermediate_evidence[0].producer_index = 2;
        report.intermediate_evidence[0].observed_sha1 = "f".repeat(40);
        assert!(verified_minecraft_srg(&selected, &profile, &request, &report).is_err());
        report.intermediate_evidence[0].observed_sha1 = srg_sha;
        let mut wrong = request.clone();
        wrong.processors[2].intermediate_outputs = vec!["generated/different.jar".into()];
        assert!(verified_minecraft_srg(&selected, &profile, &wrong, &report).is_err());
        report.verified_outputs.clear();
        assert!(verified_minecraft_slim(&selected, &profile, &request, &report).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn jarsplitter_extra_is_pinned_and_published_to_fml_maven_path() {
        use crate::runtime::{ApprovedProcessor, ExpectedOutput, ProcessorOutcome};
        use sha1::{Digest, Sha1};
        let root =
            std::env::temp_dir().join(format!("forge-client-extra-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let stage = root.join("forge-execution").join("test-op");
        let source = stage.join("generated/client-extra.jar");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        let contents = b"synthetic jarsplitter extra resource";
        fs::write(&source, contents).unwrap();
        let hash = format!("{:x}", Sha1::digest(contents));
        let profile = json!({"data":{
            "MCP_VERSION":{"client":"'20210115.111550'"},
            "MC_EXTRA_SHA":{"client":format!("'{hash}'")}
        }});
        let processor = |coordinate: &str, outputs: Vec<ExpectedOutput>| ApprovedProcessor {
            executable: VerifiedProcessorJar {
                coordinate: coordinate.into(),
                path: root.join("processor.jar"),
                sha1: "0".repeat(40),
                source: String::new(),
            },
            classpath: vec![],
            main_class: "net.minecraftforge.jarsplitter.ConsoleTool".into(),
            arguments: vec![],
            outputs,
            intermediate_outputs: vec![],
        };
        let request = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: root.join("java.exe"),
            stage_root: stage,
            processors: vec![
                processor("net.minecraftforge:installertools:1.2.6", vec![]),
                processor(
                    "net.minecraftforge:jarsplitter:1.1.4",
                    vec![
                        ExpectedOutput {
                            relative_path: "generated/client-extra.jar".into(),
                            sha1: hash.clone(),
                        },
                        ExpectedOutput {
                            relative_path: "generated/client-slim.jar".into(),
                            sha1: "a".repeat(40),
                        },
                    ],
                ),
            ],
            timeout_seconds: 180,
        };
        let report = ProcessorRunReport {
            outcome: ProcessorOutcome::Verified,
            completed: 2,
            exit_codes: vec![Some(0), Some(0)],
            log_paths: vec![],
            verified_outputs: vec![source.clone()],
            intermediate_evidence: vec![],
            error: None,
        };
        let verified =
            verified_minecraft_extra(&selected("36.2.29"), &profile, &request, &report).unwrap();
        assert_eq!(
            verified.relative_path,
            "net/minecraft/client/1.16.5-20210115.111550/client-1.16.5-20210115.111550-extra.jar"
        );
        assert_eq!(verified.path, source);
        assert_eq!(verified.official_sha1, hash);
        fs::write(&source, b"tampered").unwrap();
        assert!(
            verified_minecraft_extra(&selected("36.2.29"), &profile, &request, &report).is_err()
        );
        fs::write(&source, contents).unwrap();
        let unverified = ProcessorRunReport {
            verified_outputs: vec![],
            ..report.clone()
        };
        assert!(
            verified_minecraft_extra(&selected("36.2.29"), &profile, &request, &unverified)
                .is_err()
        );
        let mut wrong = request.clone();
        wrong.processors[1].outputs[0].sha1 = "0".repeat(40);
        assert!(verified_minecraft_extra(&selected("36.2.29"), &profile, &wrong, &report).is_err());
        let mut wrong_version = profile.clone();
        wrong_version["data"]["MCP_VERSION"]["client"] = json!("'../../escape'");
        assert!(
            verified_minecraft_extra(&selected("36.2.29"), &wrong_version, &request, &report)
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn final_forge_patch_is_a_checksum_verified_runtime_library() {
        use crate::runtime::{ApprovedProcessor, ExpectedOutput, ProcessorOutcome};
        use sha1::{Digest, Sha1};
        let root =
            std::env::temp_dir().join(format!("forge-patched-client-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let stage = root.join("forge-execution").join("test-op");
        let output = stage.join("generated/forge-client.jar");
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        let content = b"synthetic checksum-verified Forge client output";
        fs::write(&output, content).unwrap();
        let hash = format!("{:x}", Sha1::digest(content));
        let profile = json!({"data":{"PATCHED_SHA":{"client":format!("'{hash}'")}}});
        let request = ProcessorRunRequest {
            minecraft_version: "1.16.5".into(),
            java: root.join("java.exe"),
            stage_root: stage.clone(),
            processors: vec![ApprovedProcessor {
                executable: VerifiedProcessorJar {
                    coordinate: "net.minecraftforge:binarypatcher:1.0.12".into(),
                    path: root.join("binarypatcher.jar"),
                    sha1: "0".repeat(40),
                    source: String::new(),
                },
                classpath: vec![],
                main_class: "net.minecraftforge.binarypatcher.ConsoleTool".into(),
                arguments: vec![],
                outputs: vec![ExpectedOutput {
                    relative_path: "generated/forge-client.jar".into(),
                    sha1: hash.clone(),
                }],
                intermediate_outputs: vec![],
            }],
            timeout_seconds: 180,
        };
        let report = ProcessorRunReport {
            outcome: ProcessorOutcome::Verified,
            completed: 1,
            exit_codes: vec![Some(0)],
            log_paths: vec![],
            verified_outputs: vec![output.clone()],
            intermediate_evidence: vec![],
            error: None,
        };
        let accepted =
            verified_patched_client(&selected("36.2.29"), &profile, &request, &report).unwrap();
        assert_eq!(
            accepted.relative_path,
            "net/minecraftforge/forge/1.16.5-36.2.29/forge-1.16.5-36.2.29-client.jar"
        );
        assert_eq!(accepted.path, output);
        assert_eq!(accepted.official_sha1, hash);
        assert!(accepted.size_bytes > 0);

        fs::write(&accepted.path, b"tampered patch").unwrap();
        assert!(
            verified_patched_client(&selected("36.2.29"), &profile, &request, &report).is_err()
        );
        fs::write(&accepted.path, content).unwrap();
        let missing_proof = ProcessorRunReport {
            verified_outputs: vec![],
            ..report.clone()
        };
        assert!(
            verified_patched_client(&selected("36.2.29"), &profile, &request, &missing_proof)
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plan_only_must_never_produce_execution_receipt() {
        use crate::runtime::{ProcessorOutcome, ProcessorRunReport};
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        let root =
            std::env::temp_dir().join(format!("forge-receipt-plan-only-{}", std::process::id()));
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
            &selected("36.2.29"),
            PROFILE,
            &derived,
            &parent,
            &request,
            &report,
            &VerifiedForgeJava {
                executable: request.java.clone(),
                sha1: "0".repeat(40),
                major: 8,
            },
            &[],
            &[]
        )
        .is_err());
        assert!(!root.exists());
    }
}
