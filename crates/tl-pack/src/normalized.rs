//! Format-neutral installer planning boundary.
//!
//! The source archive is normalized to one plan; providers and loader runtimes
//! are selected independently. This module is deliberately fail-closed: neither
//! successful inspection nor downloaded mod bytes authorize Play/READY.
use crate::{safe_path, PackMetadata, PackSource, PlannedFile};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    CurseForge,
    Modrinth,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoaderKind {
    Forge,
    Fabric,
    NeoForge,
    Quilt,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallSupport {
    InspectOnly,
    ResolveOnly,
    StageOnly,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedFile {
    pub target: String,
    pub provider: ProviderKind,
    pub required: bool,
    pub project_id: Option<u64>,
    pub file_id: Option<u64>,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub download_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedPack {
    pub name: String,
    pub source: PackSource,
    pub archive_sha256: String,
    pub minecraft_version: String,
    pub loader: LoaderKind,
    pub loader_version: String,
    pub files: Vec<NormalizedFile>,
    pub support: InstallSupport,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderResult {
    Verified,
    NeedsAction(String),
    Failed(String),
    Unsupported(String),
}

/// Provider implementations must never transmit CurseForge secrets to a
/// download host, and must validate size/checksums before returning Verified.
pub trait ProviderAdapter {
    fn provider(&self) -> ProviderKind;
    fn resolve(&self, file: &NormalizedFile) -> ProviderResult;
}

/// Implemented by a trusted loader runtime, not by frontend booleans.
pub trait LoaderRuntimeAdapter {
    fn loader(&self) -> LoaderKind;
    fn verify_installation(&self, pack: &NormalizedPack) -> ProviderResult;
}

/// Backend can expose this verdict to the UI without enabling game launch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallGate {
    NeedsAction,
    Unsupported,
    Failed,
}

pub fn loader_kind(value: &str) -> LoaderKind {
    match value.to_ascii_lowercase().as_str() {
        "forge" => LoaderKind::Forge,
        "fabric" => LoaderKind::Fabric,
        "neoforge" | "neo_forge" => LoaderKind::NeoForge,
        "quilt" => LoaderKind::Quilt,
        _ => LoaderKind::Unsupported,
    }
}

fn provider_kind(file: &PlannedFile, source: &PackSource) -> ProviderKind {
    match file.provider.to_ascii_lowercase().as_str() {
        "curseforge" => ProviderKind::CurseForge,
        "modrinth" => ProviderKind::Modrinth,
        _ if *source == PackSource::PrismZip => ProviderKind::Local,
        _ => ProviderKind::Local,
    }
}

/// General inspection-to-normalized-plan conversion. No download or file
/// execution takes place; the source name is never used to select a loader.
/// Local and unknown providers remain inspect-only until independently
/// verified provenance and hashes exist.
pub fn normalize_pack(metadata: &PackMetadata) -> Result<NormalizedPack, String> {
    if metadata.game_version.trim().is_empty()
        || metadata.archive_sha256.len() != 64
        || !metadata
            .archive_sha256
            .bytes()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Err("verified archive identity and Minecraft version required".into());
    }
    let loader = loader_kind(&metadata.loader.kind);
    let version = metadata
        .loader
        .version
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .ok_or("loader version is missing")?;
    let mut files = Vec::with_capacity(metadata.files.len());
    let mut paths = HashSet::new();
    let mut projects = HashSet::new();
    for file in &metadata.files {
        safe_path(&file.path).map_err(|_| "unsafe normalized destination")?;
        if !paths.insert(file.path.to_lowercase()) {
            return Err("duplicate case-folded mod destination".into());
        }
        let provider = provider_kind(file, &metadata.source);
        if provider == ProviderKind::CurseForge {
            let id = (
                file.project_id.ok_or("missing project ID")?,
                file.file_id.ok_or("missing file ID")?,
            );
            if id.0 == 0 || id.1 == 0 || !projects.insert(id.0) {
                return Err("duplicate or invalid CurseForge project/file ID".into());
            }
        }
        if let Some(sha1) = &file.sha1 {
            if sha1.len() != 40 || !sha1.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err("invalid trusted SHA-1 field".into());
            }
        }
        if let Some(sha512) = &file.sha512 {
            if sha512.len() != 128 || !sha512.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err("invalid SHA-512 field".into());
            }
        }
        files.push(NormalizedFile {
            target: file.path.clone(),
            provider,
            required: file.required,
            project_id: file.project_id,
            file_id: file.file_id,
            sha1: file.sha1.clone(),
            sha512: file.sha512.clone(),
            download_urls: file.urls.clone(),
        });
    }
    let mut reasons = Vec::new();
    let support = match (&metadata.source, loader) {
        (PackSource::CurseforgeZip, LoaderKind::Forge) => {
            reasons.push("Forge runtime attestation and publisher not yet integrated".into());
            InstallSupport::StageOnly
        }
        (PackSource::ModrinthMrpack, LoaderKind::Fabric) => {
            reasons.push("Modrinth verified downloader and Fabric runtime unavailable".into());
            InstallSupport::InspectOnly
        }
        (PackSource::ModrinthMrpack, LoaderKind::Forge) => {
            reasons.push("Modrinth verified downloader and Forge integration unavailable".into());
            InstallSupport::InspectOnly
        }
        (PackSource::PrismZip, _) => {
            reasons.push(
                "Prism/local loader provenance and verified file resolution unavailable".into(),
            );
            InstallSupport::InspectOnly
        }
        (_, LoaderKind::NeoForge | LoaderKind::Quilt | LoaderKind::Unsupported) => {
            reasons.push("loader is not implemented".into());
            InstallSupport::Unsupported
        }
        _ => {
            reasons.push("pack source and loader combination is unsupported".into());
            InstallSupport::Unsupported
        }
    };
    Ok(NormalizedPack {
        name: metadata.name.clone(),
        source: metadata.source,
        archive_sha256: metadata.archive_sha256.clone(),
        minecraft_version: metadata.game_version.clone(),
        loader,
        loader_version: version.to_owned(),
        files,
        support,
        reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Loader;
    fn fixture(source: PackSource, kind: &str, version: &str) -> PackMetadata {
        PackMetadata {
            name: "any pack name".into(),
            source,
            game_version: "1.16.5".into(),
            loader: Loader {
                kind: kind.into(),
                version: Some(version.into()),
            },
            archive_sha256: "a".repeat(64),
            archive_entries: 1,
            uncompressed_bytes: 0,
            files: Vec::new(),
            overrides: Vec::new(),
            warnings: Vec::new(),
        }
    }
    #[test]
    fn curseforge_forge_not_bound_to_reference_pack_name_or_version() {
        for game in ["1.12.2", "1.16.5", "1.20.1"] {
            let mut source = fixture(PackSource::CurseforgeZip, "forge", "36.2.29");
            source.game_version = game.into();
            let plan = normalize_pack(&source).unwrap();
            assert_eq!(plan.minecraft_version, game);
            assert_eq!(plan.support, InstallSupport::StageOnly);
        }
    }
    #[test]
    fn modrinth_fabric_and_prism_are_explicit_inspect_only() {
        let fabric =
            normalize_pack(&fixture(PackSource::ModrinthMrpack, "fabric", "0.16.0")).unwrap();
        assert_eq!(fabric.loader, LoaderKind::Fabric);
        assert_eq!(fabric.support, InstallSupport::InspectOnly);
        let prism = normalize_pack(&fixture(PackSource::PrismZip, "forge", "47.0.0")).unwrap();
        assert_eq!(prism.support, InstallSupport::InspectOnly);
    }
    #[test]
    fn unimplemented_loader_is_not_advertised() {
        let plan =
            normalize_pack(&fixture(PackSource::CurseforgeZip, "neoforge", "20.4.1")).unwrap();
        assert_eq!(plan.support, InstallSupport::Unsupported);
    }
    #[test]
    fn windows_case_folded_duplicate_fails_before_download() {
        let mut pack = fixture(PackSource::ModrinthMrpack, "fabric", "0.16.0");
        let mut file = PlannedFile {
            path: "mods/Example.jar".into(),
            provider: "modrinth".into(),
            project_id: None,
            file_id: None,
            urls: Vec::new(),
            sha1: Some("a".repeat(40)),
            sha512: None,
            required: true,
            side: "both".into(),
        };
        pack.files.push(file.clone());
        file.path = "mods/example.jar".into();
        pack.files.push(file);
        assert!(normalize_pack(&pack).is_err());
    }
    #[test]
    fn file_provider_is_derived_from_manifest_not_pack_name() {
        let mut pack = fixture(PackSource::ModrinthMrpack, "fabric", "0.16.0");
        pack.files.push(PlannedFile {
            path: "mods/mod.jar".into(),
            provider: "modrinth".into(),
            project_id: None,
            file_id: None,
            urls: Vec::new(),
            sha1: None,
            sha512: Some("f".repeat(128)),
            required: false,
            side: "client".into(),
        });
        let plan = normalize_pack(&pack).unwrap();
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].provider, ProviderKind::Modrinth);
        assert!(!plan.files[0].required);
    }
}
