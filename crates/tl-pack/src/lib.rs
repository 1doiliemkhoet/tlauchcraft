//! Standalone modpack archive inspector. No external downloads or executable hooks.
//! This crate does not assert that an imported instance is runnable.
pub mod artifact_audit;
pub mod curseforge;
pub mod fingerprint;
#[cfg(windows)]
pub mod forge_official;
pub mod forge_publish;
pub mod forge_stage;
pub mod install;
pub mod installed_import;
pub mod lan_compat;
pub mod instance_manifest;
pub mod manual_import;
pub mod modrinth;
pub mod normalized;
pub mod publisher;
pub mod staged_receipt;
pub mod transaction;
pub mod validation;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use thiserror::Error;
use zip::ZipArchive;

pub const MAX_FILES: usize = 20_000;
pub const MAX_UNCOMPRESSED: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_SINGLE_FILE: u64 = 1024 * 1024 * 1024;
pub const MAX_MANIFEST: u64 = 4 * 1024 * 1024;
pub const MAX_RATIO: u64 = 200;

#[derive(Debug, Error)]
pub enum PackError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("malformed pack metadata: {0}")]
    Metadata(String),
    #[error("unsafe archive: {0}")]
    UnsafeArchive(String),
    #[error("unsupported pack: {0}")]
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackSource {
    CurseforgeZip,
    ModrinthMrpack,
    PrismZip,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Loader {
    pub kind: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackMetadata {
    pub name: String,
    pub source: PackSource,
    pub game_version: String,
    pub loader: Loader,
    pub archive_sha256: String,
    pub archive_entries: usize,
    pub uncompressed_bytes: u64,
    pub files: Vec<PlannedFile>,
    pub overrides: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedFile {
    pub path: String,
    pub provider: String,
    pub project_id: Option<u64>,
    pub file_id: Option<u64>,
    pub urls: Vec<String>,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub required: bool,
    pub side: String,
}

#[derive(Debug, Deserialize)]
struct CfManifest {
    name: String,
    minecraft: CfMinecraft,
    files: Vec<CfFile>,
    overrides: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfMinecraft {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<CfLoader>,
}
#[derive(Debug, Deserialize)]
struct CfLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFile {
    #[serde(rename = "projectID", alias = "projectId")]
    project_id: u64,
    #[serde(rename = "fileID", alias = "fileId")]
    file_id: u64,
    #[serde(default = "default_true")]
    required: bool,
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct MrIndex {
    name: String,
    dependencies: std::collections::HashMap<String, String>,
    files: Vec<MrFile>,
}
#[derive(Debug, Deserialize)]
struct MrFile {
    path: String,
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
    downloads: Vec<String>,
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
    #[serde(default)]
    file_size: Option<u64>,
}
#[derive(Debug, Deserialize)]
struct PrismPack {
    components: Vec<PrismComponent>,
}
#[derive(Debug, Deserialize)]
struct PrismComponent {
    uid: String,
    version: String,
}

/// Normalize archive paths independently of host OS (including Windows collision rules).
pub fn safe_path(path: &str) -> Result<PathBuf, PackError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path.contains(':')
        || path.chars().any(|c| c.is_control())
    {
        return Err(PackError::UnsafeArchive(format!(
            "invalid entry path: {path:?}"
        )));
    }
    let mut result = PathBuf::new();
    for part in path.trim_end_matches('/').split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with('.')
            || part.ends_with(' ')
            || part.len() > 255
        {
            return Err(PackError::UnsafeArchive(format!(
                "unsafe component in {path:?}"
            )));
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        ) {
            return Err(PackError::UnsafeArchive(format!(
                "Windows reserved entry: {path:?}"
            )));
        }
        result.push(part);
    }
    if result
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(PackError::UnsafeArchive(format!("unsafe entry: {path:?}")));
    }
    Ok(result)
}

fn read_metadata<R: Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, PackError> {
    let f = zip.by_name(name)?;
    if f.size() > MAX_MANIFEST {
        return Err(PackError::UnsafeArchive("metadata too large".into()));
    }
    let mut out = Vec::new();
    f.take(MAX_MANIFEST + 1).read_to_end(&mut out)?;
    if out.len() as u64 > MAX_MANIFEST {
        return Err(PackError::UnsafeArchive("metadata exceeds limit".into()));
    }
    Ok(out)
}

fn parse_loader(id: &str) -> Result<Loader, PackError> {
    let (kind, version) = id
        .split_once('-')
        .ok_or_else(|| PackError::Metadata(format!("loader needs version: {id}")))?;
    if !matches!(kind, "fabric" | "forge" | "neoforge") || version.trim().is_empty() {
        return Err(PackError::Unsupported(format!("loader {id}")));
    }
    Ok(Loader {
        kind: kind.into(),
        version: Some(version.into()),
    })
}

type ParsedPack = (
    String,
    String,
    Loader,
    Vec<PlannedFile>,
    String,
    Vec<String>,
);

fn from_cf<R: Read + std::io::Seek>(zip: &mut ZipArchive<R>) -> Result<ParsedPack, PackError> {
    let doc: CfManifest = serde_json::from_slice(&read_metadata(zip, "manifest.json")?)
        .map_err(|e| PackError::Metadata(e.to_string()))?;
    if doc.name.trim().is_empty() || doc.minecraft.version.trim().is_empty() {
        return Err(PackError::Metadata(
            "missing name or Minecraft version".into(),
        ));
    }
    let chosen = doc
        .minecraft
        .mod_loaders
        .iter()
        .find(|x| x.primary)
        .or_else(|| doc.minecraft.mod_loaders.first());
    let loader = match chosen {
        Some(l) => parse_loader(&l.id)?,
        None => Loader {
            kind: "vanilla".into(),
            version: None,
        },
    };
    let mut ids = HashSet::new();
    let mut projects = HashSet::new();
    let mut files = Vec::new();
    for file in doc.files {
        if file.project_id == 0
            || file.file_id == 0
            || !ids.insert((file.project_id, file.file_id))
            || !projects.insert(file.project_id)
        {
            return Err(PackError::Metadata(
                "invalid or duplicate CurseForge project/file ID".into(),
            ));
        }
        files.push(PlannedFile {
            path: format!("mods/curseforge-{}-{}.jar", file.project_id, file.file_id),
            provider: "curseforge".into(),
            project_id: Some(file.project_id),
            file_id: Some(file.file_id),
            urls: vec![],
            sha1: None,
            sha512: None,
            required: file.required,
            side: "both".into(),
        });
    }
    let prefix = doc.overrides.unwrap_or_else(|| "overrides".into());
    let normalized = safe_path(&prefix)?.to_string_lossy().replace('\\', "/");
    Ok((doc.name, doc.minecraft.version, loader, files, format!("{normalized}/"),
        vec!["CurseForge project/file IDs require an authorized API resolver; files are NOT downloaded or READY".into()]))
}
fn from_mr<R: Read + std::io::Seek>(zip: &mut ZipArchive<R>) -> Result<ParsedPack, PackError> {
    let doc: MrIndex = serde_json::from_slice(&read_metadata(zip, "modrinth.index.json")?)
        .map_err(|e| PackError::Metadata(e.to_string()))?;
    let game = doc
        .dependencies
        .get("minecraft")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| PackError::Metadata("Modrinth missing minecraft dependency".into()))?
        .clone();
    let loaders: Vec<_> = ["fabric-loader", "forge", "neoforge", "quilt-loader"]
        .iter()
        .filter_map(|key| doc.dependencies.get(*key).map(|v| (*key, v)))
        .collect();
    if loaders.len() > 1 {
        return Err(PackError::Metadata("multiple loaders conflict".into()));
    }
    let loader = match loaders.first() {
        Some((kind, _version)) if *kind == "quilt-loader" => {
            return Err(PackError::Unsupported("Quilt loader".into()))
        }
        Some((kind, version)) if !version.is_empty() => Loader {
            kind: if *kind == "fabric-loader" {
                "fabric"
            } else {
                *kind
            }
            .into(),
            version: Some((*version).clone()),
        },
        Some(_) => return Err(PackError::Metadata("empty loader version".into())),
        None => Loader {
            kind: "vanilla".into(),
            version: None,
        },
    };
    let mut files = Vec::new();
    let mut paths = HashSet::new();
    for f in doc.files {
        let clean = safe_path(&f.path)?.to_string_lossy().replace('\\', "/");
        if !paths.insert(clean.to_lowercase()) {
            return Err(PackError::Metadata("duplicate Modrinth path".into()));
        }
        if !clean.starts_with("mods/")
            && !clean.starts_with("resourcepacks/")
            && !clean.starts_with("shaderpacks/")
            && !clean.starts_with("config/")
        {
            return Err(PackError::Unsupported(format!(
                "unreviewed Modrinth destination {clean}"
            )));
        }
        let side = f
            .env
            .get("client")
            .map(String::as_str)
            .unwrap_or("required");
        if !matches!(side, "required" | "optional" | "unsupported") {
            return Err(PackError::Metadata(
                "invalid Modrinth client environment".into(),
            ));
        }
        if let Some(size) = f.file_size {
            if size > MAX_SINGLE_FILE {
                return Err(PackError::UnsafeArchive("large Modrinth file".into()));
            }
        }
        let sha1 = f.hashes.get("sha1").cloned();
        let sha512 = f.hashes.get("sha512").cloned();
        if sha1
            .as_ref()
            .is_none_or(|v| v.len() != 40 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
            && sha512
                .as_ref()
                .is_none_or(|v| v.len() != 128 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(PackError::Metadata(format!(
                "no valid integrity hash for {clean}"
            )));
        }
        if f.downloads.is_empty() {
            return Err(PackError::Metadata(format!(
                "no authorized download for {clean}"
            )));
        }
        for url in &f.downloads {
            if !url.starts_with("https://") || url.contains('@') || url.contains('#') {
                return Err(PackError::Unsupported(format!(
                    "non-HTTPS or unsupported download URL for {clean}"
                )));
            }
        }
        files.push(PlannedFile {
            path: clean,
            provider: "modrinth".into(),
            project_id: None,
            file_id: None,
            urls: f.downloads,
            sha1,
            sha512,
            required: side == "required",
            side: if side == "unsupported" {
                "server"
            } else {
                "client"
            }
            .into(),
        });
    }
    if doc.name.trim().is_empty() {
        return Err(PackError::Metadata("missing pack name".into()));
    }
    Ok((doc.name, game, loader, files, "overrides/".into(),
        vec!["URLs are metadata only; future downloader MUST validate DNS, redirects, IP ranges and hashes".into()]))
}
fn from_prism<R: Read + std::io::Seek>(zip: &mut ZipArchive<R>) -> Result<ParsedPack, PackError> {
    let doc: PrismPack = serde_json::from_slice(&read_metadata(zip, "mmc-pack.json")?)
        .map_err(|e| PackError::Metadata(e.to_string()))?;
    let game = doc
        .components
        .iter()
        .find(|c| c.uid == "net.minecraft")
        .map(|c| c.version.clone())
        .ok_or_else(|| PackError::Metadata("Prism missing net.minecraft component".into()))?;
    let mods: Vec<_> = doc
        .components
        .iter()
        .filter(|c| c.uid != "net.minecraft")
        .collect();
    let mut loader = Loader {
        kind: "vanilla".into(),
        version: None,
    };
    for comp in mods {
        let kind = match comp.uid.as_str() {
            "net.fabricmc.fabric-loader" => "fabric",
            "net.minecraftforge" => "forge",
            "net.neoforged" => "neoforge",
            _ => {
                return Err(PackError::Unsupported(format!(
                    "Prism component {}",
                    comp.uid
                )))
            }
        };
        if loader.kind != "vanilla" {
            return Err(PackError::Metadata("multiple Prism loaders".into()));
        }
        loader = Loader {
            kind: kind.into(),
            version: Some(comp.version.clone()),
        };
    }
    Ok(("Prism instance".into(), game, loader, Vec::new(), "minecraft/".into(),
        vec!["Prism files are local overrides only; installed mods and loader still require verification".into()]))
}

/// Inspect archive and return a plan. Never extracts or executes archive content.
pub fn inspect_archive(path: &Path) -> Result<PackMetadata, PackError> {
    let mut hash_input = File::open(path)?;
    let mut sha = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = hash_input.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        sha.update(&chunk[..n]);
    }
    let mut zip = ZipArchive::new(File::open(path)?)?;
    if zip.len() > MAX_FILES {
        return Err(PackError::UnsafeArchive("too many archive entries".into()));
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    let mut has_cf = false;
    let mut has_mr = false;
    let mut has_prism = false;
    let mut cfg = false;
    let mut paths = Vec::new();
    for index in 0..zip.len() {
        let f = zip.by_index(index)?;
        let name = f.name().to_string();
        let clean = safe_path(&name)?.to_string_lossy().replace('\\', "/");
        let key = clean.to_lowercase();
        if !names.insert(key.clone()) {
            return Err(PackError::UnsafeArchive(format!("duplicate path: {name}")));
        }
        if f.is_dir() && !name.ends_with('/') {
            return Err(PackError::UnsafeArchive(format!(
                "invalid directory: {name}"
            )));
        }
        if let Some(mode) = f.unix_mode() {
            let file_type = mode & 0o170000;
            if file_type != 0 && file_type != 0o100000 && file_type != 0o040000 {
                return Err(PackError::UnsafeArchive(format!(
                    "symlink/special entry: {name}"
                )));
            }
        }
        if f.size() > MAX_SINGLE_FILE {
            return Err(PackError::UnsafeArchive(format!("file too large: {name}")));
        }
        total = total
            .checked_add(f.size())
            .ok_or_else(|| PackError::UnsafeArchive("size overflow".into()))?;
        if total > MAX_UNCOMPRESSED {
            return Err(PackError::UnsafeArchive(
                "archive expands beyond limit".into(),
            ));
        }
        if f.size() > 1024 * 1024 && f.size() / f.compressed_size().max(1) > MAX_RATIO {
            return Err(PackError::UnsafeArchive(format!(
                "compression ratio suspicious: {name}"
            )));
        }
        match key.as_str() {
            "manifest.json" => has_cf = true,
            "modrinth.index.json" => has_mr = true,
            "mmc-pack.json" => has_prism = true,
            "instance.cfg" => cfg = true,
            _ => {}
        }
        paths.push(clean);
    }
    let formats = has_cf as u8 + has_mr as u8 + (has_prism && cfg) as u8;
    if formats != 1 {
        return Err(PackError::Unsupported(
            "unknown or ambiguous modpack format".into(),
        ));
    }
    let (source, data) = if has_cf {
        (PackSource::CurseforgeZip, from_cf(&mut zip)?)
    } else if has_mr {
        (PackSource::ModrinthMrpack, from_mr(&mut zip)?)
    } else {
        (PackSource::PrismZip, from_prism(&mut zip)?)
    };
    let (name, game_version, loader, files, prefix, warnings) = data;
    let mut overrides: Vec<String> = paths
        .iter()
        .filter(|p| p.starts_with(&prefix) && p.len() > prefix.len())
        .map(|p| p[prefix.len()..].to_owned())
        .collect();
    if source == PackSource::ModrinthMrpack {
        overrides.extend(
            paths
                .iter()
                .filter(|p| {
                    p.starts_with("client-overrides/") && p.len() > "client-overrides/".len()
                })
                .map(|p| p["client-overrides/".len()..].to_owned()),
        );
    }
    Ok(PackMetadata {
        name,
        source,
        game_version,
        loader,
        archive_sha256: hex::encode(sha.finalize()),
        archive_entries: zip.len(),
        uncompressed_bytes: total,
        files,
        overrides,
        warnings,
    })
}

/// Extract only declared override entries into a fresh staging directory.
/// The caller owns final atomic publication and must NOT call this on a game/save directory.
/// Refuses existing staging directories; successful staging is NOT READY.
pub fn stage_overrides(
    archive: &Path,
    metadata: &PackMetadata,
    staging: &Path,
) -> Result<(), PackError> {
    if staging.exists() {
        return Err(PackError::UnsafeArchive("staging already exists".into()));
    }
    let actual = inspect_archive(archive)?;
    if actual.archive_sha256 != metadata.archive_sha256 || actual.source != metadata.source {
        return Err(PackError::UnsafeArchive(
            "archive changed since inspection".into(),
        ));
    }
    fs::create_dir(staging)?;
    let prefixes: Vec<String> = match metadata.source {
        PackSource::CurseforgeZip => {
            let mut zip = ZipArchive::new(File::open(archive)?)?;
            let doc: CfManifest =
                serde_json::from_slice(&read_metadata(&mut zip, "manifest.json")?)
                    .map_err(|e| PackError::Metadata(e.to_string()))?;
            let override_dir = doc.overrides.unwrap_or_else(|| "overrides".into());
            vec![format!(
                "{}/",
                safe_path(&override_dir)?
                    .to_string_lossy()
                    .replace('\\', "/")
            )]
        }
        // Later client overrides intentionally take precedence over shared overrides.
        PackSource::ModrinthMrpack => vec!["overrides/".into(), "client-overrides/".into()],
        PackSource::PrismZip => vec!["minecraft/".into()],
    };
    let operation = (|| -> Result<(), PackError> {
        let mut zip = ZipArchive::new(File::open(archive)?)?;
        for prefix in &prefixes {
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i)?;
                let name = entry.name().to_owned();
                if !name.starts_with(prefix) || name == *prefix {
                    continue;
                }
                let rel = safe_path(&name[prefix.len()..])?;
                if metadata.source == PackSource::PrismZip
                    && (name.ends_with(".json") && name.contains("/patches/"))
                {
                    return Err(PackError::Unsupported(
                        "legacy Prism patches require loader adapter".into(),
                    ));
                }
                let dest = staging.join(rel);
                if entry.is_dir() {
                    fs::create_dir_all(&dest)?;
                } else {
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let mut out = File::create(&dest)?;
                    let size = std::io::copy(&mut entry, &mut out)?;
                    if size != entry.size() {
                        return Err(PackError::UnsafeArchive("truncated entry".into()));
                    }
                    out.flush()?;
                }
            }
        }
        Ok(())
    })();
    if operation.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    operation
}

/// No downloader integration exists yet; required remote files must block READY.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportState {
    Planned,
    NeedsAction,
    Failed,
}
pub fn initial_state(metadata: &PackMetadata) -> ImportState {
    if !metadata.files.is_empty() {
        ImportState::NeedsAction
    } else {
        ImportState::Planned
    }
}
