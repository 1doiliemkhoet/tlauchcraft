//! Forge 1.16.5 installation graph preparation. No JAR is run by this module.
//! The verified official installer provides the profile. All external files must be
//! pinned by the caller and preflight verified before assembling a process request.
use crate::{
    forge_1165::{forge_1165_client_blueprint, ForgeClientStep},
    runtime::{ApprovedProcessor, ExpectedOutput, ProcessorRunRequest, VerifiedProcessorJar},
    LoaderError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrepareState {
    Verified,
    NeedsAction,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareReport {
    pub state: PrepareState,
    /// Non-executed request. Verified means *prepared*, never installed.
    pub request: Option<ProcessorRunRequest>,
    pub unresolved: Vec<String>,
    pub error: Option<String>,
}

/// A bound value must be a literal argument, a verified read-only external path,
/// or a path generated exclusively under a fresh stage root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoundValue {
    Literal(String),
    VerifiedExternal(VerifiedProcessorJar),
    Staged(String),
}

/// Caller supplies preverified Maven artifact bindings, one per exact coordinate.
/// For Maven with explicit extensions (e.g. @zip), preserve the full string key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeBindings {
    pub minecraft: String,
    pub forge_version: String,
    pub java: PathBuf,
    pub stage_root: PathBuf,
    pub timeout_seconds: u64,
    /// Main tool/classpath/dependencies obtained from the official verified installer
    /// and downloaded only from approved canonical Maven origins.
    pub artifacts: BTreeMap<String, VerifiedProcessorJar>,
    /// Explicit preverified data values keyed by their install_profile data name.
    pub data: BTreeMap<String, BoundValue>,
}

/// Forge install_profile has data values per side (client/server).
/// Resolve only client fields; never fall back to server content.
#[derive(Deserialize)]
struct DataProfile {
    #[serde(default)]
    data: BTreeMap<String, ClientData>,
    #[serde(default)]
    libraries: Vec<LibraryRecord>,
}
#[derive(Deserialize)]
struct LibraryRecord {
    name: String,
    downloads: Option<DownloadRecord>,
}
#[derive(Deserialize)]
struct DownloadRecord {
    artifact: Option<LibraryArtifact>,
}
#[derive(Deserialize)]
struct LibraryArtifact {
    url: String,
    sha1: String,
}
/// The profile's own upstream-pinned SHA-1 and URL are authoritative. A caller
/// may not substitute a matching local SHA of *different* tool bytes.
fn validate_profile_artifact(
    coordinate: &str,
    artifact: &VerifiedProcessorJar,
    profile: &DataProfile,
) -> Result<(), PrepareFailure> {
    if profile.libraries.is_empty() {
        return Ok(());
    } // legacy representative fixture
    let record = profile
        .libraries
        .iter()
        .find(|item| item.name == coordinate)
        .ok_or_else(|| {
            LoaderError::InvalidMetadata(format!(
                "processor artifact absent from official profile: {coordinate}"
            ))
        })?;
    let entry = record
        .downloads
        .as_ref()
        .and_then(|d| d.artifact.as_ref())
        .ok_or_else(|| {
            LoaderError::InvalidMetadata("official artifact download descriptor missing".into())
        })?;
    if entry.url.is_empty() {
        return Err(LoaderError::InvalidMetadata(
            "installer-generated library has no remote URL; requires verified installer evidence"
                .into(),
        )
        .into());
    }
    if artifact.source != entry.url || !artifact.sha1.eq_ignore_ascii_case(&entry.sha1) {
        return Err(LoaderError::InvalidMetadata(format!(
            "artifact provenance/sha1 mismatch: {coordinate}"
        ))
        .into());
    }
    Ok(())
}
#[derive(Deserialize)]
struct ClientData {
    client: Option<String>,
}

/// Prepare without executing arbitrary metadata-defined Java main classes.
/// Missing trusted inputs = NeedsAction. Malformed metadata = Failed.
pub fn prepare_forge_1165(
    profile_json: &str,
    version_json: &Value,
    bindings: &ForgeBindings,
) -> PrepareReport {
    match prepare_inner(profile_json, version_json, bindings) {
        Ok(processors) => PrepareReport {
            state: PrepareState::Verified,
            request: Some(ProcessorRunRequest {
                minecraft_version: bindings.minecraft.clone(),
                java: bindings.java.clone(),
                stage_root: bindings.stage_root.clone(),
                processors,
                timeout_seconds: bindings.timeout_seconds,
            }),
            unresolved: vec![],
            error: None,
        },
        Err(PrepareFailure::Missing(items)) => PrepareReport {
            state: PrepareState::NeedsAction,
            request: None,
            unresolved: items,
            error: None,
        },
        Err(PrepareFailure::Invalid(error)) => PrepareReport {
            state: PrepareState::Failed,
            request: None,
            unresolved: vec![],
            error: Some(error.to_string()),
        },
    }
}

enum PrepareFailure {
    Missing(Vec<String>),
    Invalid(LoaderError),
}

impl From<LoaderError> for PrepareFailure {
    fn from(value: LoaderError) -> Self {
        Self::Invalid(value)
    }
}

fn prepare_inner(
    profile_json: &str,
    version_json: &Value,
    bindings: &ForgeBindings,
) -> Result<Vec<ApprovedProcessor>, PrepareFailure> {
    if bindings.minecraft != "1.16.5"
        || bindings.timeout_seconds == 0
        || bindings.timeout_seconds > 900
        || bindings.stage_root.exists()
        || !bindings.stage_root.is_absolute()
    {
        return Err(LoaderError::InvalidInput.into());
    }
    let blueprint =
        forge_1165_client_blueprint(profile_json, version_json, &bindings.forge_version)?;
    let profile: DataProfile = serde_json::from_str(profile_json)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let mut producers = BTreeMap::<String, usize>::new();
    for (index, step) in blueprint.steps.iter().enumerate() {
        for token in step_output_tokens(step)? {
            if producers.insert(token.clone(), index).is_some() {
                return Err(LoaderError::InvalidMetadata(format!(
                    "duplicate declared producer: {token}"
                ))
                .into());
            }
        }
    }
    let mut all_missing = BTreeSet::new();
    let mut results = Vec::new();
    let mut produced = BTreeSet::<String>::new();
    for (index, step) in blueprint.steps.iter().enumerate() {
        let mut missing = BTreeSet::new();
        let prepared = prepare_step(
            step,
            index,
            &producers,
            &profile,
            bindings,
            &produced,
            &mut missing,
        )?;
        if !missing.is_empty() {
            all_missing.extend(missing);
            continue;
        }
        if let Some((processor, outputs)) = prepared {
            for output in outputs {
                if !produced.insert(output) {
                    return Err(
                        LoaderError::InvalidMetadata("duplicate processor output".into()).into(),
                    );
                }
            }
            results.push(processor);
        }
    }
    if !all_missing.is_empty() {
        return Err(PrepareFailure::Missing(all_missing.into_iter().collect()));
    }
    if results.len() != blueprint.steps.len() {
        return Err(LoaderError::InvalidMetadata("incomplete processor graph".into()).into());
    }
    Ok(results)
}

fn step_output_tokens(step: &ForgeClientStep) -> Result<Vec<String>, PrepareFailure> {
    let mut result = Vec::new();
    for key in step.outputs.keys() {
        let token = key
            .strip_prefix('{')
            .and_then(|x| x.strip_suffix('}'))
            .ok_or_else(|| LoaderError::InvalidMetadata("declared output token invalid".into()))?;
        result.push(token.to_string());
    }
    if step.outputs.is_empty() {
        let flag = match step.jar.as_str() {
            "net.minecraftforge:installertools:1.2.6" => "--output",
            "net.md-5:SpecialSource:1.8.5" => "--out-jar",
            _ => {
                return Err(
                    LoaderError::InvalidMetadata("unapproved output-less processor".into()).into(),
                )
            }
        };
        let i = step.args.iter().position(|v| v == flag).ok_or_else(|| {
            LoaderError::InvalidMetadata("intermediate output flag absent".into())
        })?;
        let token = step
            .args
            .get(i + 1)
            .and_then(|x| x.strip_prefix('{').and_then(|x| x.strip_suffix('}')))
            .ok_or_else(|| {
                LoaderError::InvalidMetadata("intermediate output token malformed".into())
            })?;
        result.push(token.into());
    }
    Ok(result)
}

fn prepare_step(
    step: &ForgeClientStep,
    step_index: usize,
    producers: &BTreeMap<String, usize>,
    profile: &DataProfile,
    bindings: &ForgeBindings,
    produced: &BTreeSet<String>,
    missing: &mut BTreeSet<String>,
) -> Result<Option<(ApprovedProcessor, Vec<String>)>, PrepareFailure> {
    // Reject unapproved processor classes before reporting missing dependencies.
    let main = approved_main(&step.jar)?;
    let jar = resolve_artifact(&step.jar, profile, bindings, missing)?;
    let mut classpath = Vec::new();
    for dep in &step.classpath {
        if let Some(x) = resolve_artifact(dep, profile, bindings, missing)? {
            classpath.push(x);
        }
    }
    // Collect known output bindings before expanding arguments. Two authentic
    // Forge 36.2.29 tools generate intermediates without a declared SHA-1 map.
    // The output is inspected after the producing process, never assumed verified.
    let mut outputs = Vec::new();
    let mut output_paths = Vec::new();
    for (raw_path, raw_hash) in &step.outputs {
        let path = match resolve_output_path(raw_path, profile, bindings, missing)? {
            Some(x) => x,
            None => continue,
        };
        let digest = match resolve_hash(raw_hash, profile, bindings, missing)? {
            Some(x) => x,
            None => continue,
        };
        output_paths.push(path.clone());
        outputs.push(ExpectedOutput {
            relative_path: path,
            sha1: digest,
        });
    }
    let mut intermediate_outputs = Vec::new();
    if step.outputs.is_empty() {
        let output_flag = match step.jar.as_str() {
            "net.minecraftforge:installertools:1.2.6" => "--output",
            "net.md-5:SpecialSource:1.8.5" => "--out-jar",
            _ => {
                return Err(LoaderError::InvalidMetadata(
                    "processor has no declared or approved inferred outputs".into(),
                )
                .into())
            }
        };
        let pos = step
            .args
            .iter()
            .position(|arg| arg == output_flag)
            .ok_or_else(|| {
                LoaderError::InvalidMetadata("intermediate output flag missing".into())
            })?;
        let raw_output = step.args.get(pos + 1).ok_or_else(|| {
            LoaderError::InvalidMetadata("intermediate output token missing".into())
        })?;
        let path = match resolve_output_path(raw_output, profile, bindings, missing)? {
            Some(p) => p,
            None => return Ok(None),
        };
        intermediate_outputs.push(path.clone());
        output_paths.push(path);
    }
    let mut allowable = produced.clone();
    allowable.extend(output_paths.iter().cloned());
    // An earlier declared producer blocked by missing inputs remains a NeedsAction,
    // not a false graph-cycle error. Later producers or undeclared sources fail.
    for (token, producer) in producers {
        if *producer < step_index {
            if let Some(BoundValue::Staged(path)) = bindings.data.get(token) {
                allowable.insert(path.clone());
            }
        }
    }
    // Input flags may only use already-produced staged outputs. A current-step
    // output cannot retroactively satisfy an input dependency.
    for (i, arg) in step.args.iter().enumerate() {
        if matches!(
            arg.as_str(),
            "--input" | "--in-jar" | "--srg-in" | "--srg" | "--clean" | "--apply"
        ) {
            if let Some(raw_input) = step.args.get(i + 1) {
                if let Some(token) = raw_input
                    .strip_prefix('{')
                    .and_then(|v| v.strip_suffix('}'))
                {
                    if let Some(BoundValue::Staged(relative)) = bindings.data.get(token) {
                        safe_stage_relative(relative)?;
                        if !produced.contains(relative) {
                            match producers.get(token) {
                                Some(index) if *index < step_index => {
                                    missing.insert(format!("upstream-output:{token}"));
                                }
                                _ => {
                                    return Err(LoaderError::InvalidMetadata(format!(
                                        "processor consumes staged input before producer: {token}"
                                    ))
                                    .into());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut args = Vec::new();
    for token in &step.args {
        if let Some(x) = resolve_arg(token, profile, bindings, &allowable, missing)? {
            args.push(x);
        }
    }
    if !missing.is_empty() {
        return Ok(None);
    }
    let jar = jar.ok_or_else(|| {
        LoaderError::InvalidMetadata("processor executable missing after preflight".into())
    })?;
    if outputs.is_empty() && intermediate_outputs.is_empty() {
        return Err(
            LoaderError::InvalidMetadata("processor output evidence missing".into()).into(),
        );
    }
    Ok(Some((
        ApprovedProcessor {
            executable: jar,
            classpath,
            main_class: main.to_owned(),
            arguments: args,
            outputs,
            intermediate_outputs,
        },
        output_paths,
    )))
}

fn approved_main(coordinate: &str) -> Result<&'static str, PrepareFailure> {
    let main = if coordinate.starts_with("net.minecraftforge:installertools:") {
        "net.minecraftforge.installertools.ConsoleTool"
    } else if coordinate.starts_with("net.minecraftforge:jarsplitter:") {
        "net.minecraftforge.jarsplitter.ConsoleTool"
    } else if coordinate.starts_with("net.minecraftforge:ForgeAutoRenamingTool:") {
        "net.minecraftforge.fart.Main"
    } else if coordinate.starts_with("net.minecraftforge:binarypatcher:") {
        "net.minecraftforge.binarypatcher.ConsoleTool"
    } else if coordinate == "net.md-5:SpecialSource:1.8.5" {
        "net.md_5.specialsource.SpecialSource"
    } else {
        return Err(
            LoaderError::InvalidMetadata("unapproved Forge processor coordinate".into()).into(),
        );
    };
    Ok(main)
}

fn resolve_artifact(
    coordinate: &str,
    profile: &DataProfile,
    bindings: &ForgeBindings,
    missing: &mut BTreeSet<String>,
) -> Result<Option<VerifiedProcessorJar>, PrepareFailure> {
    crate::maven::artifact_path(coordinate)?;
    if let Some(artifact) = bindings.artifacts.get(coordinate) {
        if artifact.coordinate != coordinate {
            return Err(LoaderError::InvalidMetadata("artifact coordinate mismatch".into()).into());
        }
        validate_profile_artifact(coordinate, artifact, profile)?;
        crate::runtime::validate_verified_artifact(artifact)?;
        Ok(Some(artifact.clone()))
    } else {
        missing.insert(format!("artifact:{coordinate}"));
        Ok(None)
    }
}

fn resolve_arg(
    raw: &str,
    profile: &DataProfile,
    bindings: &ForgeBindings,
    produced: &BTreeSet<String>,
    missing: &mut BTreeSet<String>,
) -> Result<Option<String>, PrepareFailure> {
    // Do not expand shell-like arguments or freeform nested expressions.
    if raw.is_empty() || raw.len() > 8192 || raw.chars().any(char::is_control) {
        return Err(LoaderError::InvalidInput.into());
    }
    let mut output = String::new();
    let mut chars = raw.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        if ch == '{' {
            let mut token = String::new();
            let mut closed = false;
            for (_, value) in chars.by_ref() {
                if value == '}' {
                    closed = true;
                    break;
                }
                token.push(value);
            }
            if !closed
                || token.is_empty()
                || !token
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            {
                return Err(LoaderError::InvalidMetadata("malformed data token".into()).into());
            }
            let result = resolve_data(&token, profile, bindings, produced, missing)?;
            if let Some(x) = result {
                output.push_str(&x);
            }
        } else if ch == '[' {
            let mut token = String::new();
            let mut closed = false;
            for (_, value) in chars.by_ref() {
                if value == ']' {
                    closed = true;
                    break;
                }
                token.push(value);
            }
            if !closed {
                return Err(
                    LoaderError::InvalidMetadata("unterminated Maven reference".into()).into(),
                );
            }
            let key = format!("[{token}]");
            if let Some(x) = bindings.data.get(&key) {
                if let BoundValue::VerifiedExternal(artifact) = x {
                    validate_profile_artifact(&token, artifact, profile)?;
                }
                output.push_str(&bound_value(&key, x, bindings, produced)?);
            } else if let Some(artifact) = bindings.artifacts.get(&token) {
                if artifact.coordinate != token {
                    return Err(LoaderError::InvalidInput.into());
                }
                validate_profile_artifact(&token, artifact, profile)?;
                crate::runtime::validate_verified_artifact(artifact)?;
                output.push_str(artifact.path.to_str().ok_or(LoaderError::InvalidInput)?);
            } else {
                missing.insert(format!("maven:{key}"));
            }
        } else if matches!(ch, '}' | ']') {
            return Err(LoaderError::InvalidMetadata("stray token delimiter".into()).into());
        } else {
            output.push(ch);
        }
    }
    if output.contains('{') || output.contains('}') || output.contains('[') || output.contains(']')
    {
        return Err(LoaderError::InvalidMetadata("nested/unresolved token".into()).into());
    }
    if output.is_empty() && !missing.is_empty() {
        Ok(None)
    } else {
        Ok(Some(output))
    }
}

fn resolve_data(
    key: &str,
    profile: &DataProfile,
    bindings: &ForgeBindings,
    produced: &BTreeSet<String>,
    missing: &mut BTreeSet<String>,
) -> Result<Option<String>, PrepareFailure> {
    // Built-ins must be explicitly bound; never resolve implicit paths to user instances.
    if let Some(item) = bindings.data.get(key) {
        return bound_value(key, item, bindings, produced).map(Some);
    }
    let Some(record) = profile.data.get(key) else {
        missing.insert(format!("data:{key}"));
        return Ok(None);
    };
    let Some(value) = &record.client else {
        missing.insert(format!("client-data:{key}"));
        return Ok(None);
    };
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        return Ok(Some(value[1..value.len() - 1].to_string()));
    }
    if value.starts_with('[') && value.ends_with(']') {
        let result = resolve_arg(value, profile, bindings, produced, missing)?;
        return Ok(result);
    }
    // Forge BINPATCH is embedded inside the verified installer; require a
    // separate extraction/provenance gate, never interpret it as a local path.
    if value.starts_with('/') {
        missing.insert(format!("verified-installer-entry:{key}:{value}"));
        return Ok(None);
    }
    // Unknown archive paths are not interpreted as arbitrary local files.
    missing.insert(format!("binding:{key}"));
    Ok(None)
}

fn bound_value(
    key: &str,
    value: &BoundValue,
    bindings: &ForgeBindings,
    produced: &BTreeSet<String>,
) -> Result<String, PrepareFailure> {
    match value {
        BoundValue::Literal(x) => {
            // Literals are scalar metadata, never an unverified jar/zip path.
            if key.starts_with('[')
                || x.contains('/')
                || x.contains('\\')
                || x.ends_with(".jar")
                || x.ends_with(".zip")
                || x.is_empty()
                || x.len() > 8192
                || x.chars().any(char::is_control)
                || x.chars().any(|ch| matches!(ch, '{' | '}' | '[' | ']'))
            {
                return Err(LoaderError::InvalidMetadata(format!("invalid literal {key}")).into());
            }
            Ok(x.clone())
        }
        BoundValue::VerifiedExternal(x) => {
            // Every referenced input is checked before any process can spawn.
            // A metadata source string never substitutes for a locally checked hash.
            if !x.path.is_absolute() {
                return Err(LoaderError::InvalidInput.into());
            }
            crate::runtime::validate_verified_artifact(x)?;
            Ok(x.path.to_string_lossy().into_owned())
        }
        BoundValue::Staged(rel) => {
            safe_stage_relative(rel)?;
            if !produced.contains(rel) {
                return Err(LoaderError::InvalidMetadata(
                    "input depends on unavailable output".into(),
                )
                .into());
            }
            Ok(bindings.stage_root.join(rel).to_string_lossy().into_owned())
        }
    }
}

fn resolve_output_path(
    raw: &str,
    profile: &DataProfile,
    bindings: &ForgeBindings,
    missing: &mut BTreeSet<String>,
) -> Result<Option<String>, PrepareFailure> {
    let token = raw.strip_prefix('{').and_then(|x| x.strip_suffix('}'));
    let Some(name) = token else {
        return Err(LoaderError::InvalidMetadata("output must be a data token".into()).into());
    };
    // Staged path must be explicitly bound; this avoids arbitrary manifest paths.
    if !profile.data.contains_key(name) && !bindings.data.contains_key(name) {
        missing.insert(format!("output:{name}"));
        return Ok(None);
    }
    match bindings.data.get(name) {
        Some(BoundValue::Staged(path)) => {
            safe_stage_relative(path)?;
            Ok(Some(path.clone()))
        }
        Some(_) => {
            Err(LoaderError::InvalidMetadata("output is not private staging path".into()).into())
        }
        None => {
            missing.insert(format!("staged-output:{name}"));
            Ok(None)
        }
    }
}

fn resolve_hash(
    raw: &str,
    profile: &DataProfile,
    bindings: &ForgeBindings,
    missing: &mut BTreeSet<String>,
) -> Result<Option<String>, PrepareFailure> {
    let value = if let Some(name) = raw.strip_prefix('{').and_then(|x| x.strip_suffix('}')) {
        resolve_data(name, profile, bindings, &BTreeSet::new(), missing)?
    } else {
        Some(raw.trim_matches('\'').to_string())
    };
    if let Some(ref hash) = value {
        if hash.len() != 40 || !hash.bytes().all(|x| x.is_ascii_hexdigit()) {
            return Err(LoaderError::InvalidMetadata("invalid output hash".into()).into());
        }
    }
    Ok(value)
}

fn safe_stage_relative(path: &str) -> Result<(), PrepareFailure> {
    if path.is_empty() || path.len() > 512 || path.contains('\\') || path.contains(':') {
        return Err(LoaderError::InvalidInput.into());
    }
    let parts = Path::new(path);
    if parts.is_absolute()
        || parts
            .components()
            .any(|x| !matches!(x, Component::Normal(_)))
    {
        return Err(LoaderError::InvalidInput.into());
    }
    Ok(())
}
