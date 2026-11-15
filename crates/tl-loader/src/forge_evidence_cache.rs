//! Read-only importer for the Control-reviewed Forge evidence cache.
//! Never downloads, executes Java, or touches a Minecraft instance.
use crate::{
    forge_graph::{
        prepare_forge_1165, BoundValue, ForgeBindings, InstallerEntryEvidence, PrepareReport,
        PrepareState,
    },
    forge_provenance::{audit_official_forge_metadata, ArtifactRequirement, SourceRequirement},
    runtime::VerifiedProcessorJar,
    LoaderError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedMojangClient {
    /// Local client.jar from Mojang's official, digest-addressed download metadata.
    pub path: PathBuf,
    pub url: String,
    pub sha1: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeCacheImport {
    /// Multiple user-selected evidence cache roots; each artifact is at its
    /// immutable Maven relative_path beneath exactly one root.
    pub cache_roots: Vec<PathBuf>,
    pub client: VerifiedMojangClient,
    pub binpatch: InstallerEntryEvidence,
    pub java8: PathBuf,
    pub stage_root: PathBuf,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheImportState {
    Verified,
    NeedsAction,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheImportReport {
    pub state: CacheImportState,
    pub verified_libraries: usize,
    pub generated_or_installer: usize,
    pub missing: Vec<String>,
    pub error: Option<String>,
    /// A successful import means only local preflight, NEVER instance ready.
    pub instance_ready: bool,
}

pub struct ImportedForgeCache {
    pub bindings: ForgeBindings,
    pub report: CacheImportReport,
}

/// Use together with `preflight_official_forge`. All profile/derived entries
/// are audited using the ORIGINAL trusted metadata URL + SHA-1 + exact size.
pub fn import_official_forge_cache(
    profile_json: &str,
    derived: &Value,
    options: &ForgeCacheImport,
) -> Result<ImportedForgeCache, LoaderError> {
    let audit = audit_official_forge_metadata(profile_json, derived)?;
    if options.cache_roots.is_empty()
        || options.cache_roots.len() > 16
        || !options.stage_root.is_absolute()
        || options.stage_root.exists()
        || !options.java8.is_absolute()
        || options.timeout_seconds == 0
        || options.timeout_seconds > 900
    {
        return Err(LoaderError::InvalidInput);
    }
    for root in &options.cache_roots {
        if !root.is_absolute() || !root.is_dir() {
            return Err(LoaderError::InvalidInput);
        }
        reject_symlinks(root)?;
    }
    reject_symlinks(&options.stage_root)?;
    let mut missing = BTreeSet::new();
    let mut artifacts = BTreeMap::new();
    let mut verified = 0;
    let mut generated = 0;
    let mut seen = BTreeMap::<String, (String, u64, String)>::new();
    for requirement in audit
        .profile_libraries
        .iter()
        .chain(&audit.derived_libraries)
    {
        match &requirement.source {
            SourceRequirement::VerifiedInstallerOrGenerated => {
                generated += 1;
                missing.insert(format!("generated-or-installer:{}", requirement.coordinate));
            }
            SourceRequirement::CanonicalMaven { url } => {
                // The same Maven coordinate must never have inconsistent upstream
                // checksum/size/origin across profile and derived metadata.
                if let Some(prior) = seen.get(&requirement.coordinate) {
                    if prior
                        != &(
                            requirement.sha1.clone(),
                            requirement.size_bytes,
                            url.clone(),
                        )
                    {
                        return Err(LoaderError::InvalidMetadata(format!(
                            "conflicting official library descriptors: {}",
                            requirement.coordinate
                        )));
                    }
                    continue;
                }
                seen.insert(
                    requirement.coordinate.clone(),
                    (
                        requirement.sha1.clone(),
                        requirement.size_bytes,
                        url.clone(),
                    ),
                );
                if let Some(file) = find_verified_file(requirement, url, &options.cache_roots)? {
                    artifacts.insert(requirement.coordinate.clone(), file);
                    verified += 1;
                } else {
                    missing.insert(format!("cache:{}", requirement.relative_path));
                }
            }
        }
    }
    // Source/size/digest are rechecked against official Mojang metadata supplied
    // by Control, and the URL is strictly digest-addressed.
    if !verified_mojang(&options.client)? {
        missing.insert("verified-mojang-client:1.16.5".into());
    }
    let mut data = BTreeMap::new();
    if !missing.contains("verified-mojang-client:1.16.5") {
        data.insert(
            "MINECRAFT_JAR".into(),
            BoundValue::VerifiedExternal(VerifiedProcessorJar {
                coordinate: "net.minecraft:client:1.16.5".into(),
                path: options.client.path.clone(),
                sha1: options.client.sha1.clone(),
                source: options.client.url.clone(),
            }),
        );
    }
    data.insert(
        "BINPATCH".into(),
        BoundValue::VerifiedInstallerEntry(options.binpatch.clone()),
    );
    for (token, relative) in [
        ("MAPPINGS", "generated/mappings.txt"),
        ("MC_SLIM", "generated/client-slim.jar"),
        ("MC_EXTRA", "generated/client-extra.jar"),
        ("MC_SRG", "generated/client-srg.jar"),
        ("PATCHED", "generated/client-patched.jar"),
    ] {
        data.insert(token.into(), BoundValue::Staged(relative.into()));
    }
    let profile: Value = serde_json::from_str(profile_json)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let forge_version = profile["version"]
        .as_str()
        .and_then(|v| v.strip_prefix("1.16.5-forge-"))
        .ok_or(LoaderError::CompatibilityConflict)?;
    let bindings = ForgeBindings {
        minecraft: audit.minecraft_version,
        forge_version: forge_version.into(),
        java: options.java8.clone(),
        stage_root: options.stage_root.clone(),
        timeout_seconds: options.timeout_seconds,
        artifacts,
        data,
    };
    // The two generated artifacts are expected missing until actual processors
    // run; they must not be conflated with a successful loader installation.
    let state = if missing.is_empty() {
        CacheImportState::Verified
    } else {
        CacheImportState::NeedsAction
    };
    Ok(ImportedForgeCache {
        bindings,
        report: CacheImportReport {
            state,
            verified_libraries: verified,
            generated_or_installer: generated,
            missing: missing.into_iter().collect(),
            error: None,
            instance_ready: false,
        },
    })
}

/// Never runs Java. Computes the verified preflight state from actual bytes and
/// BINPATCH provenance, preserving NeedsAction for two ungenerated Forge JARs.
pub fn preflight_official_forge(
    profile_json: &str,
    derived: &Value,
    options: &ForgeCacheImport,
) -> (CacheImportReport, PrepareReport) {
    match import_official_forge_cache(profile_json, derived, options) {
        Err(e) => (
            CacheImportReport {
                state: CacheImportState::Failed,
                verified_libraries: 0,
                generated_or_installer: 0,
                missing: vec![],
                error: Some(e.to_string()),
                instance_ready: false,
            },
            PrepareReport {
                state: PrepareState::Failed,
                request: None,
                unresolved: vec![],
                error: Some(e.to_string()),
            },
        ),
        Ok(result) => {
            let mut plan = prepare_forge_1165(profile_json, derived, &result.bindings);
            // Missing real cache material may make plan's resolved graph incomplete;
            // aggregate the audited inventory gaps into a conservative NeedsAction.
            if plan.state != PrepareState::Failed
                && result.report.state == CacheImportState::NeedsAction
            {
                plan.unresolved
                    .extend(result.report.missing.iter().cloned());
                plan.unresolved.sort();
                plan.unresolved.dedup();
                plan.state = PrepareState::NeedsAction;
                plan.request = None;
            }
            (result.report, plan)
        }
    }
}

fn find_verified_file(
    requirement: &ArtifactRequirement,
    url: &str,
    roots: &[PathBuf],
) -> Result<Option<VerifiedProcessorJar>, LoaderError> {
    let relative = Path::new(&requirement.relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|x| !matches!(x, Component::Normal(_)))
    {
        return Err(LoaderError::InvalidMetadata(
            "unsafe Maven relative path".into(),
        ));
    }
    let mut result = None;
    for root in roots {
        let path = root.join(relative);
        if !path.exists() {
            continue;
        }
        reject_symlinks(&path)?;
        if !path.is_file() {
            return Err(LoaderError::InvalidMetadata(
                "Maven artifact is not a regular file".into(),
            ));
        }
        verify_size_and_sha1(&path, requirement.size_bytes, &requirement.sha1)?;
        if result.is_some() {
            return Err(LoaderError::InvalidMetadata(
                "duplicate artifact across evidence roots".into(),
            ));
        }
        result = Some(VerifiedProcessorJar {
            coordinate: requirement.coordinate.clone(),
            path,
            sha1: requirement.sha1.clone(),
            source: url.into(),
        });
    }
    Ok(result)
}

fn verified_mojang(client: &VerifiedMojangClient) -> Result<bool, LoaderError> {
    // Control-verified Mojang 1.16.5 release metadata. A caller cannot
    // substitute a self-consistent but unrelated SHA-1 / digest-addressed URL.
    if !client.sha1.eq_ignore_ascii_case("37fd3c903861eeff3bc24b71eed48f828b5269c8")
        || client.size_bytes != 17_547_153
    { return Err(LoaderError::InvalidMetadata("unapproved Minecraft 1.16.5 client SHA-1/size".into())); }
    if client.sha1.len() != 40
        || !client.sha1.bytes().all(|x| x.is_ascii_hexdigit())
        || client.size_bytes == 0
    {
        return Err(LoaderError::InvalidInput);
    }
    let allowed = [
        "https://launcher.mojang.com",
        "https://piston-data.mojang.com",
    ];
    if !allowed.iter().any(|host| {
        client.url
            == format!(
                "{host}/v1/objects/{}/client.jar",
                client.sha1.to_ascii_lowercase()
            )
    }) {
        return Err(LoaderError::InvalidMetadata(
            "unapproved Mojang client URL".into(),
        ));
    }
    if !client.path.is_absolute() || !client.path.is_file() {
        return Ok(false);
    }
    reject_symlinks(&client.path)?;
    verify_size_and_sha1(&client.path, client.size_bytes, &client.sha1)?;
    Ok(true)
}
fn verify_size_and_sha1(path: &Path, size: u64, expected: &str) -> Result<(), LoaderError> {
    if std::fs::metadata(path)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?
        .len()
        != size
    {
        return Err(LoaderError::InvalidMetadata(format!(
            "official size mismatch: {}",
            path.display()
        )));
    }
    let mut file = File::open(path).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let mut hasher = Sha1::new();
    let mut bytes = [0_u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut bytes)
            .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
        if n == 0 {
            break;
        }
        hasher.update(&bytes[..n]);
    }
    if format!("{:x}", hasher.finalize()) != expected.to_ascii_lowercase() {
        return Err(LoaderError::InvalidMetadata(format!(
            "official SHA-1 mismatch: {}",
            path.display()
        )));
    }
    Ok(())
}
fn reject_symlinks(path: &Path) -> Result<(), LoaderError> {
    for ancestor in path.ancestors() {
        if ancestor.is_symlink() {
            return Err(LoaderError::InvalidMetadata(
                "symlink path in evidence cache".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROFILE: &str = include_str!("../tests/fixtures/forge_36229_official_profile.json");
    const DERIVED: &str = include_str!("../tests/fixtures/forge_36229_official_version.json");
    fn options(root: &Path) -> ForgeCacheImport {
        ForgeCacheImport {
            cache_roots: vec![root.into()],
            client: VerifiedMojangClient {
                path: root.join("client.jar"),
                url: "https://launcher.mojang.com/v1/objects/37fd3c903861eeff3bc24b71eed48f828b5269c8/client.jar".into(),
                sha1: "37fd3c903861eeff3bc24b71eed48f828b5269c8".into(),
                size_bytes: 17547153,
            },
            binpatch: InstallerEntryEvidence {
                installer: root.join("forge-1.16.5-36.2.29-installer.jar"),
                installer_sha1: "7d247279cf06eed6ffcf609f36d11e34c70350f3".into(),
                entry: "data/client.lzma".into(),
                extracted: root.join("evidence/client.lzma"),
                extracted_sha1: "6b494c622457566e6db2bed43a2de4a735ac72a6".into(),
                report: root.join("evidence/binpatch-provenance.json"),
            },
            java8: root.join("java8.exe"),
            stage_root: root.join("fresh-private-stage"),
            timeout_seconds: 120,
        }
    }
    #[test]
    fn missing_real_inputs_are_needs_action_without_java() {
        let root = std::env::temp_dir().join(format!("tl03-import-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let opt = options(&root);
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        let (cache, plan) = preflight_official_forge(PROFILE, &derived, &opt);
        assert_eq!(cache.state, CacheImportState::NeedsAction);
        assert!(cache
            .missing
            .iter()
            .any(|x| x.contains("generated-or-installer:")));
        assert!(cache
            .missing
            .iter()
            .any(|x| x.contains("verified-mojang-client")));
        assert_eq!(plan.state, PrepareState::NeedsAction);
        assert!(plan.request.is_none());
        assert!(!opt.stage_root.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_mojang_source_fails_closed() {
        let root = std::env::temp_dir().join(format!("tl03-mojang-import-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut opt = options(&root);
        opt.client.url = "https://mirror.invalid/client.jar".into();
        let derived: Value = serde_json::from_str(DERIVED).unwrap();
        let (cache, plan) = preflight_official_forge(PROFILE, &derived, &opt);
        assert_eq!(cache.state, CacheImportState::Failed);
        assert_eq!(plan.state, PrepareState::Failed);
        assert!(!opt.stage_root.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
