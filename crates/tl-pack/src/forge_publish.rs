//! Trusted Forge 1.16.5 publication boundary.
//! Only execution proof sealed by tl-loader's actual guarded runner can reach
//! the in-process verifier and permit issuer.
//! This module NEVER executes processors, downloads mods or trusts frontend
//! claims of readiness. A caller must provide actual approved execution inputs.
use crate::{
    curseforge::CurseForgeResolveReport,
    publisher::{publish_verified, VerifiedPublishPermit},
    staged_receipt::{audit_curseforge_staging, VerifiedModReceipt},
    PackMetadata, PackSource,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest as _, Sha1};
use sha2::Sha256;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::AtomicBool,
};
use tl_loader::{
    forge_playable::{
        verify_forge_runtime_for_pack, PackForgeSelection, VerifiedForgeJava, VerifiedForgeLibrary,
    },
    runtime::{VerifiedProcessorExecution, VerifiedProcessorJar},
};

pub const FORGE_INSTALL_MARKER: &str = ".tlauchcraft-forge-install.json";
const MAX_RECORD_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledForgeLibrary {
    pub relative_path: String,
    pub official_sha1: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedOverride {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Durable *file-integrity* evidence, not a claim that game UI/world/LAN ran.
/// The SQLite READY state is set separately after a successful atomic publish.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeInstallRecord {
    pub schema: String,
    pub minecraft_version: String,
    pub forge_version: String,
    /// Java version from the trusted loader compatibility adapter.
    pub required_java_major: u32,
    pub verified_main_class: String,
    pub archive_sha256: String,
    pub version: Value,
    pub natives_relative_path: String,
    pub installed_mods: Vec<VerifiedModReceipt>,
    /// Source of the mod inventory. Local imports have *observed-only* SHA-1,
    /// not authenticated upstream digests. The SQLite receipt anchor protects it.
    #[serde(default = "default_mod_provenance")]
    pub mod_provenance: String,
    pub installed_libraries: Vec<InstalledForgeLibrary>,
    pub verified_overrides: Vec<VerifiedOverride>,
    pub native_dlls: Vec<VerifiedOverride>,
    pub minecraft_verified_files: usize,
}

/// The binarypatcher output is not listed by version.json Maven libraries,
/// but FML requires it at this exact private-instance path during client launch.
fn require_patched_client(
    libraries: &[InstalledForgeLibrary],
    minecraft: &str,
    forge: &str,
) -> Result<(), String> {
    let version = format!("{minecraft}-{forge}");
    let expected =
        format!("libraries/net/minecraftforge/forge/{version}/forge-{version}-client.jar");
    if !libraries.iter().any(|lib| {
        lib.relative_path == expected
            && lib.size_bytes > 0
            && lib.official_sha1.len() == 40
            && lib.official_sha1.bytes().all(|c| c.is_ascii_hexdigit())
    }) {
        return Err(
            "verified Forge patched client.jar missing from published inventory; re-import the installed CurseForge folder with the updated launcher".into(),
        );
    }
    Ok(())
}

/// Resolve the MCP resource version from the actual verified Forge
/// launch arguments. Never infer a Maven filename from a user-provided path.
fn require_minecraft_extra(
    libraries: &[InstalledForgeLibrary],
    version: &Value,
    minecraft: &str,
) -> Result<(), String> {
    let game_args = version["arguments"]["game"]
        .as_array()
        .ok_or("Forge game arguments missing")?;
    let mut found = game_args
        .windows(2)
        .filter(|pair| pair[0].as_str() == Some("--fml.mcpVersion"));
    let mcp_version = found
        .next()
        .and_then(|pair| pair[1].as_str())
        .ok_or("Forge MCP version missing")?;
    if found.next().is_some()
        || mcp_version.is_empty()
        || mcp_version.len() > 64
        || !mcp_version.bytes().all(|c| c.is_ascii_digit() || c == b'.')
    {
        return Err("Forge MCP version is unsafe or ambiguous".into());
    }
    let version = format!("{minecraft}-{mcp_version}");
    let required = format!("libraries/net/minecraft/client/{version}/client-{version}-extra.jar");
    if !libraries.iter().any(|lib| {
        lib.relative_path == required
            && lib.size_bytes > 0
            && lib.official_sha1.len() == 40
            && lib.official_sha1.bytes().all(|c| c.is_ascii_hexdigit())
    }) {
        return Err(
            "verified Minecraft client extra.jar missing from Forge publish inventory; re-import the installed CurseForge folder using the updated launcher".into(),
        );
    }
    Ok(())
}

/// The profile-generated client SRG is produced by SpecialSource step #3.
/// The SLIM JAR is an SHA-1-pinned jarsplitter output. Both must be installed
/// with EXTRA and Forge PATCHED before marking an instance READY.
fn require_all_minecraft_generated_jars(
    libraries: &[InstalledForgeLibrary],
    version: &Value,
    minecraft: &str,
) -> Result<(), String> {
    require_minecraft_extra(libraries, version, minecraft)?;
    let game_args = version["arguments"]["game"]
        .as_array()
        .ok_or("Forge game arguments missing")?;
    let mut found = game_args
        .windows(2)
        .filter(|pair| pair[0].as_str() == Some("--fml.mcpVersion"));
    let mcp_version = found
        .next()
        .and_then(|pair| pair[1].as_str())
        .ok_or("Forge MCP version missing")?;
    if found.next().is_some()
        || mcp_version.is_empty()
        || mcp_version.len() > 64
        || !mcp_version.bytes().all(|c| c.is_ascii_digit() || c == b'.')
    {
        return Err("Forge MCP version unsafe or ambiguous".into());
    }
    let version = format!("{minecraft}-{mcp_version}");
    for classifier in ["srg", "slim"] {
        let required =
            format!("libraries/net/minecraft/client/{version}/client-{version}-{classifier}.jar");
        if !libraries.iter().any(|lib| {
            lib.relative_path == required
                && lib.size_bytes > 0
                && lib.official_sha1.len() == 40
                && lib.official_sha1.bytes().all(|c| c.is_ascii_hexdigit())
        }) {
            return Err(format!(
                "verified Minecraft client {classifier}.jar missing from Forge published inventory; re-import with updated launcher"
            ));
        }
    }
    Ok(())
}

fn default_mod_provenance() -> String {
    "curseforge-api-sha1/v1".into()
}

pub struct ApprovedForgePublishInputs<'a> {
    pub selection: &'a PackForgeSelection,
    pub profile_json: &'a str,
    pub derived: &'a Value,
    pub mojang_parent: &'a Value,
    pub processor_execution: &'a VerifiedProcessorExecution,
    pub java: &'a VerifiedForgeJava,
    pub maven: &'a [VerifiedProcessorJar],
    pub installer_or_generated: &'a [VerifiedForgeLibrary],
}

fn reject_symlinks(path: &Path) -> Result<(), String> {
    for part in path.ancestors() {
        if fs::symlink_metadata(part).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Forge publication path contains a symlink".into());
        }
    }
    Ok(())
}

fn verify_tree_symlinks(dir: &Path) -> Result<(), String> {
    reject_symlinks(dir)?;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Forge staged instance contains a symlink".into());
        }
        if kind.is_dir() {
            verify_tree_symlinks(&entry.path())?;
        } else if !kind.is_file() {
            return Err("Forge staged instance contains a special file".into());
        }
    }
    Ok(())
}

fn file_hashes(path: &Path) -> Result<(u64, String, String), String> {
    reject_symlinks(path)?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() || meta.len() > crate::MAX_SINGLE_FILE {
        return Err("missing or oversized Forge file".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut total = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).ok_or("file size overflow")?;
        if total > meta.len() {
            return Err("file grew during validation".into());
        }
        sha1.update(&buf[..n]);
        sha256.update(&buf[..n]);
    }
    if total != meta.len() {
        return Err("file changed during validation".into());
    }
    Ok((
        total,
        format!("{:x}", sha1.finalize()),
        format!("{:x}", sha256.finalize()),
    ))
}

fn check_file(
    root: &Path,
    relative: &str,
    size: u64,
    sha1: Option<&str>,
    sha256: Option<&str>,
) -> Result<(), String> {
    let rel = crate::safe_path(relative).map_err(|e| e.to_string())?;
    let (actual_size, actual_sha1, actual_sha256) = file_hashes(&root.join(rel))?;
    if actual_size != size
        || sha1.is_some_and(|expected| !actual_sha1.eq_ignore_ascii_case(expected))
        || sha256.is_some_and(|expected| !actual_sha256.eq_ignore_ascii_case(expected))
    {
        return Err(format!("published Forge artifact changed: {relative}"));
    }
    Ok(())
}

fn verify_natives(root: &Path, relative: &str) -> Result<(), String> {
    let path = root.join(crate::safe_path(relative).map_err(|e| e.to_string())?);
    reject_symlinks(&path)?;
    let mut dlls = 0usize;
    for entry in fs::read_dir(&path).map_err(|_| "verified Windows natives missing")? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("native directory contains a symlink".into());
        }
        if kind.is_file()
            && entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("dll"))
        {
            dlls += 1;
        }
    }
    if dlls == 0 {
        return Err("verified Windows native DLLs missing".into());
    }
    Ok(())
}

/// Snapshot every extracted native DLL digest for later Play-time validation.
fn collect_verified_natives(root: &Path) -> Result<Vec<VerifiedOverride>, String> {
    let dir = root.join("natives");
    reject_symlinks(&dir)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|_| "native DLL directory missing")? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.file_type().map_err(|e| e.to_string())?.is_file()
            || !name.to_ascii_lowercase().ends_with(".dll")
            || crate::safe_path(&name).is_err()
        {
            return Err("unexpected native DLL directory entry".into());
        }
        let (bytes, _, sha256) = file_hashes(&entry.path())?;
        if bytes == 0 {
            return Err("empty native DLL".into());
        }
        files.push(VerifiedOverride {
            relative_path: format!("natives/{name}"),
            sha256,
            bytes,
        });
    }
    if files.is_empty() {
        return Err("native DLL inventory empty".into());
    }
    files.sort_by(|a, b| {
        a.relative_path
            .to_ascii_lowercase()
            .cmp(&b.relative_path.to_ascii_lowercase())
    });
    if files
        .windows(2)
        .any(|w| w[0].relative_path.eq_ignore_ascii_case(&w[1].relative_path))
    {
        return Err("native DLL filename collision".into());
    }
    Ok(files)
}

/// Recheck overrides against the original ZIP bytes, not a JSON/boolean
/// supplied by the UI. Overrides are staged at the game-directory root.
fn verify_original_overrides(
    root: &Path,
    archive: &Path,
    manifest: &PackMetadata,
) -> Result<Vec<VerifiedOverride>, String> {
    let input = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(input).map_err(|e| e.to_string())?;
    let metadata: Value = {
        let mut entry = zip.by_name("manifest.json").map_err(|e| e.to_string())?;
        if entry.size() > crate::MAX_MANIFEST {
            return Err("oversized CurseForge manifest".into());
        }
        serde_json::from_reader(&mut entry).map_err(|e| e.to_string())?
    };
    let prefix = metadata["overrides"].as_str().unwrap_or("overrides");
    let prefix = crate::safe_path(prefix).map_err(|e| e.to_string())?;
    let prefix = prefix.to_string_lossy();
    let mut verified = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for relative in &manifest.overrides {
        let destination = crate::safe_path(relative).map_err(|e| e.to_string())?;
        if !seen.insert(relative.to_ascii_lowercase()) {
            return Err("duplicate override in pack manifest".into());
        }
        let source_path = format!("{prefix}/{relative}");
        // The normalized ZIP inspector strips a trailing slash on directory
        // entries; ZIP lookup must restore it before comparing directories.
        let zip_name = if zip.file_names().any(|name| name == source_path) {
            source_path
        } else {
            format!("{source_path}/")
        };
        let mut entry = zip
            .by_name(&zip_name)
            .map_err(|_| "missing original override")?;
        let target = root.join(&destination);
        reject_symlinks(&target)?;
        if entry.is_dir() {
            if !target.is_dir() {
                return Err("override directory missing".into());
            }
            continue;
        }
        if entry.size() > crate::MAX_SINGLE_FILE {
            return Err("invalid original override file size".into());
        }
        let mut source_hash = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = entry.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            source_hash.update(&buf[..n]);
        }
        let expected = format!("{:x}", source_hash.finalize());
        check_file(root, relative, entry.size(), None, Some(&expected))?;
        verified.push(VerifiedOverride {
            relative_path: relative.clone(),
            sha256: expected,
            bytes: entry.size(),
        });
    }
    Ok(verified)
}

/// Assemble the official Minecraft files and Forge libraries into the same
/// private modpack stage, reusing a separately verified Mojang cache.
/// An approved in-process runner must have produced the opaque proof first.
/// This never executes third-party Forge processors or grants READY.
pub fn assemble_from_approved_execution(
    stage: &Path,
    mojang_cache: &Path,
    evidence: &ApprovedForgePublishInputs<'_>,
    cancel: &AtomicBool,
) -> Result<crate::forge_stage::ForgeStageAssembly, String> {
    let runtime = verify_forge_runtime_for_pack(
        evidence.selection,
        evidence.profile_json,
        evidence.derived,
        evidence.mojang_parent,
        evidence.processor_execution.request(),
        evidence.processor_execution.report(),
        evidence.java,
        evidence.maven,
        evidence.installer_or_generated,
    )
    .map_err(|e| format!("approved Forge execution unavailable: {e}"))?;
    crate::forge_stage::assemble_forge_stage(
        stage,
        mojang_cache,
        evidence.mojang_parent,
        &runtime,
        cancel,
    )
}

/// Publication paths distinguish source-verified API JARs from locally copied
/// CurseForge JARs. Local SHA values are not upstream authentication.
enum ModEvidence<'a> {
    Provider(&'a CurseForgeResolveReport),
    Local(&'a [VerifiedModReceipt]),
}

/// Original API source-verified publication contract, preserved unchanged.
pub fn publish_forge_from_execution(
    stage: &Path,
    destination: &Path,
    source_archive: &Path,
    pack: &PackMetadata,
    resolved: &CurseForgeResolveReport,
    evidence: ApprovedForgePublishInputs<'_>,
    anchor_verified_receipt: impl FnOnce(&str) -> Result<(), String>,
) -> Result<ForgeInstallRecord, String> {
    publish_with_mod_evidence(
        stage,
        destination,
        source_archive,
        pack,
        ModEvidence::Provider(resolved),
        evidence,
        anchor_verified_receipt,
    )
}

/// Import from a locally installed CurseForge profile; no API key is required.
/// The Rust importer issues a local digest snapshot, NOT provider-verified SHA1.
/// Final publication still requires actual approved Forge execution and rehashes
/// every mod, native, library, override and official Mojang artifact.
pub fn publish_forge_from_local_execution(
    stage: &Path,
    destination: &Path,
    source_archive: &Path,
    pack: &PackMetadata,
    local_mods: &[VerifiedModReceipt],
    evidence: ApprovedForgePublishInputs<'_>,
    anchor_verified_receipt: impl FnOnce(&str) -> Result<(), String>,
) -> Result<ForgeInstallRecord, String> {
    publish_with_mod_evidence(
        stage,
        destination,
        source_archive,
        pack,
        ModEvidence::Local(local_mods),
        evidence,
        anchor_verified_receipt,
    )
}

/// This does NOT execute a third-party processor. Only sealed in-process runner
/// evidence and fully rehashed staged bytes can mint a new publish permit.
fn publish_with_mod_evidence(
    stage: &Path,
    destination: &Path,
    source_archive: &Path,
    pack: &PackMetadata,
    mod_evidence: ModEvidence<'_>,
    evidence: ApprovedForgePublishInputs<'_>,
    anchor_verified_receipt: impl FnOnce(&str) -> Result<(), String>,
) -> Result<ForgeInstallRecord, String> {
    if pack.source != PackSource::CurseforgeZip || !stage.is_absolute() {
        return Err("Forge requires an original CurseForge archive and private stage".into());
    }
    let current = crate::inspect_archive(source_archive).map_err(|e| e.to_string())?;
    if current.archive_sha256 != pack.archive_sha256
        || current.game_version != pack.game_version
        || current.loader != pack.loader
        || current.files != pack.files
        || current.overrides != pack.overrides
    {
        return Err("source ZIP differs from reviewed Forge manifest".into());
    }
    if pack.game_version != evidence.selection.minecraft_version
        || pack.loader.kind != "forge"
        || pack.loader.version.as_deref() != Some(evidence.selection.forge_version.as_str())
        || pack.archive_sha256 != evidence.selection.pack_archive_sha256
    {
        return Err("Forge runtime version differs from original modpack".into());
    }
    verify_tree_symlinks(stage)?;
    let (installed_mods, mod_provenance) = match mod_evidence {
        ModEvidence::Provider(resolved) => {
            let mods = audit_curseforge_staging(stage, pack, resolved)?;
            if !mods.complete_mod_inventory
                || !mods.blocked_optional.is_empty()
                || !mods.failures.is_empty()
                || mods.verified.len() != pack.files.len()
            {
                return Err("required/optional provider mod inventory incomplete".into());
            }
            (mods.verified, "curseforge-api-sha1/v1".to_owned())
        }
        ModEvidence::Local(local_mods) => {
            if local_mods.len() != pack.files.len() || local_mods.is_empty() {
                return Err("local installed profile does not match ZIP manifest count".into());
            }
            verify_exact_mod_inventory(stage, local_mods)?;
            let mut ids = std::collections::HashSet::new();
            for verified in local_mods {
                if !ids.insert((verified.project_id, verified.file_id))
                    || verified.bytes == 0
                    || verified.upstream_sha1.len() != 40
                    || !verified
                        .upstream_sha1
                        .bytes()
                        .all(|v| v.is_ascii_hexdigit())
                    || verified.observed_sha256.len() != 64
                    || !verified
                        .observed_sha256
                        .bytes()
                        .all(|v| v.is_ascii_hexdigit())
                    || !pack.files.iter().any(|declared| {
                        declared.project_id == Some(verified.project_id)
                            && declared.file_id == Some(verified.file_id)
                            && declared.required == verified.required
                    })
                {
                    return Err("local inventory identity, count or hash invalid".into());
                }
                // Hashes were taken from locally installed JARs, not from the
                // provider. Rehash them AGAIN before minting the SQLite anchor.
                check_file(
                    stage,
                    &verified.relative_path,
                    verified.bytes,
                    Some(&verified.upstream_sha1),
                    Some(&verified.observed_sha256),
                )?;
            }
            (
                local_mods.to_vec(),
                "local-installed-curseforge/v1".to_owned(),
            )
        }
    };
    let runtime = verify_forge_runtime_for_pack(
        evidence.selection,
        evidence.profile_json,
        evidence.derived,
        evidence.mojang_parent,
        evidence.processor_execution.request(),
        evidence.processor_execution.report(),
        evidence.java,
        evidence.maven,
        evidence.installer_or_generated,
    )
    .map_err(|e| format!("Forge processor verification failed: {e}"))?;
    if !runtime.execution_verified
        || runtime.required_java_major == 0
        || runtime.instance_ready
        || runtime.game_launched
        || runtime.processor_count != 4
    {
        return Err("Forge processors not accepted by trusted runtime".into());
    }
    let mut libraries = Vec::new();
    for library in runtime.libraries {
        let relative = format!("libraries/{}", library.relative_path);
        check_file(
            stage,
            &relative,
            library.size_bytes,
            Some(&library.official_sha1),
            None,
        )?;
        libraries.push(InstalledForgeLibrary {
            relative_path: relative,
            official_sha1: library.official_sha1,
            size_bytes: library.size_bytes,
        });
    }
    if libraries.is_empty() {
        return Err("Forge library inventory empty".into());
    }
    require_patched_client(
        &libraries,
        &pack.game_version,
        &evidence.selection.forge_version,
    )?;
    require_all_minecraft_generated_jars(
        &libraries,
        &runtime.java8_launch_version,
        &pack.game_version,
    )?;
    let overrides = verify_original_overrides(stage, source_archive, pack)?;
    verify_natives(stage, "natives")?;
    let native_dlls = collect_verified_natives(stage)?;
    let version = runtime.java8_launch_version;
    if version["id"] != runtime.derived_version_id
        || version["mainClass"] != runtime.inherited_version["mainClass"]
        || version["mainClass"].as_str().is_none_or(str::is_empty)
        || version["inheritsFrom"] != pack.game_version
        || version["javaVersion"]["majorVersion"] != runtime.required_java_major
    {
        return Err("inherited Forge launch metadata invalid".into());
    }
    let checked = tl_game::vanilla_evidence::validate_install_files(&version, stage)
        .map_err(|e| format!("Mojang/Forge client, natives, assets invalid: {e}"))?;
    fs::create_dir_all(stage.join("logs")).map_err(|e| e.to_string())?;
    let record = ForgeInstallRecord {
        schema: "tl-forge-published/v1".into(),
        minecraft_version: pack.game_version.clone(),
        forge_version: evidence.selection.forge_version.clone(),
        required_java_major: runtime.required_java_major,
        verified_main_class: runtime.inherited_version["mainClass"]
            .as_str()
            .ok_or("verified Forge main class missing")?
            .to_owned(),
        archive_sha256: pack.archive_sha256.clone(),
        version,
        natives_relative_path: "natives".into(),
        installed_mods,
        mod_provenance,
        installed_libraries: libraries,
        verified_overrides: overrides,
        native_dlls,
        minecraft_verified_files: checked,
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("published Forge record too large".into());
    }
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join(FORGE_INSTALL_MARKER))
        .map_err(|e| format!("Forge marker cannot be written create-new: {e}"))?;
    out.write_all(&bytes)
        .and_then(|_| out.sync_all())
        .map_err(|e| e.to_string())?;
    // Windows rename cannot reliably move a directory with an open receipt
    // handle. The durable file must be flushed AND closed before MoveFileW.
    drop(out);
    // Persist the immutable trusted digest BEFORE exposing the final directory.
    // Crash recovery must never derive trust from the user-writable JSON itself.
    let receipt_digest = format!("{:x}", Sha256::digest(&bytes));
    anchor_verified_receipt(&receipt_digest)?;
    publish_verified(
        stage,
        destination,
        VerifiedPublishPermit::after_verified_forge_publish_audit(),
    )?;
    Ok(record)
}

/// Modpacks may rewrite ordinary user configuration during gameplay.
/// Configs are hashed against the original ZIP before READY, while later
/// launches permit normal user edits (never changes to mods/JARs/scripts).
fn is_mutable_user_config(relative: &str) -> bool {
    let path = relative.to_ascii_lowercase();
    let in_config = path.starts_with("config/") || path.starts_with("defaultconfigs/");
    let ext_allowed = [
        ".toml",
        ".cfg",
        ".json",
        ".properties",
        ".txt",
        ".yaml",
        ".yml",
    ]
    .iter()
    .any(|ext| path.ends_with(ext));
    (in_config && ext_allowed)
        || matches!(
            path.as_str(),
            "options.txt" | "optionsof.txt" | "servers.dat"
        )
}

fn verify_mutable_config_path(root: &Path, relative: &str) -> Result<(), String> {
    let rel = crate::safe_path(relative).map_err(|e| e.to_string())?;
    let target = root.join(rel);
    reject_symlinks(&target)?;
    let info = fs::symlink_metadata(&target).map_err(|_| "user configuration file missing")?;
    if !info.file_type().is_file() || info.len() > crate::MAX_SINGLE_FILE {
        return Err("user configuration file missing or unsafe".into());
    }
    Ok(())
}

fn verify_exact_mod_inventory(root: &Path, mods: &[VerifiedModReceipt]) -> Result<(), String> {
    let mods_dir = root.join("mods");
    reject_symlinks(&mods_dir)?;
    let mut expected = std::collections::HashSet::new();
    for item in mods {
        let relative = crate::safe_path(&item.relative_path).map_err(|e| e.to_string())?;
        if relative.parent() != Some(Path::new("mods")) {
            return Err("Forge mod receipt targets a non-mod directory".into());
        }
        let filename = relative
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid Forge mod filename")?
            .to_ascii_lowercase();
        if !filename.ends_with(".jar") || !expected.insert(filename) {
            return Err("invalid or duplicate Forge mod inventory entry".into());
        }
    }
    if expected.is_empty() {
        return Err("Forge mod inventory empty".into());
    }
    let mut observed = std::collections::HashSet::new();
    for entry in fs::read_dir(&mods_dir).map_err(|_| "Forge mods directory missing")? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err("Forge mods directory contains a symlink or non-file".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "invalid Unicode in Forge mod filename")?
            .to_ascii_lowercase();
        if !expected.contains(&name) || !observed.insert(name) {
            return Err("unexpected or case-duplicate Forge mod in published instance".into());
        }
    }
    if observed != expected {
        return Err("missing Forge mod from published inventory".into());
    }
    Ok(())
}

fn verify_receipt_anchor(bytes: &[u8], trusted_sha256: &str) -> Result<(), String> {
    if trusted_sha256.len() != 64 || !trusted_sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("missing trusted Forge receipt digest".into());
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(trusted_sha256) {
        return Err("Forge receipt changed since trusted SQLite VERIFYING intent".into());
    }
    Ok(())
}

/// Disk read is not enough to issue a publish permit. Used only at Play time,
/// after the corresponding persisted SQLite instance is already READY.
pub fn reverify_published_forge(
    root: &Path,
    trusted_sha256: &str,
) -> Result<ForgeInstallRecord, String> {
    if !root.is_absolute() {
        return Err("published Forge path must be absolute".into());
    }
    verify_tree_symlinks(root)?;
    let marker = root.join(FORGE_INSTALL_MARKER);
    if fs::metadata(&marker)
        .map_err(|_| "Forge install record missing")?
        .len()
        > MAX_RECORD_BYTES
    {
        return Err("Forge install record oversized".into());
    }
    let bytes = fs::read(&marker).map_err(|e| e.to_string())?;
    verify_receipt_anchor(&bytes, trusted_sha256)?;
    let record: ForgeInstallRecord = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if record.schema != "tl-forge-published/v1"
        || !matches!(
            record.mod_provenance.as_str(),
            "curseforge-api-sha1/v1" | "local-installed-curseforge/v1"
        )
        || record.minecraft_version.is_empty()
        || record.forge_version.is_empty()
        || !(8..=25).contains(&record.required_java_major)
        || record.archive_sha256.len() != 64
        || !record.archive_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || record.version["id"]
            != format!(
                "{}-forge-{}",
                record.minecraft_version, record.forge_version
            )
        || record.verified_main_class.is_empty()
        || record.version["mainClass"] != record.verified_main_class
        || record.version["inheritsFrom"] != record.minecraft_version
        || record.version["javaVersion"]["majorVersion"] != record.required_java_major
        || record.installed_libraries.is_empty()
        || record.installed_mods.is_empty()
    {
        return Err("Forge installed marker is not valid for its recorded loader selection".into());
    }
    require_patched_client(
        &record.installed_libraries,
        &record.minecraft_version,
        &record.forge_version,
    )?;
    require_all_minecraft_generated_jars(
        &record.installed_libraries,
        &record.version,
        &record.minecraft_version,
    )?;
    for library in &record.installed_libraries {
        check_file(
            root,
            &library.relative_path,
            library.size_bytes,
            Some(&library.official_sha1),
            None,
        )?;
    }
    verify_exact_mod_inventory(root, &record.installed_mods)?;
    for file in &record.installed_mods {
        check_file(
            root,
            &file.relative_path,
            file.bytes,
            Some(&file.upstream_sha1),
            Some(&file.observed_sha256),
        )?;
    }
    for file in &record.verified_overrides {
        if is_mutable_user_config(&file.relative_path) {
            verify_mutable_config_path(root, &file.relative_path)?;
        } else {
            check_file(
                root,
                &file.relative_path,
                file.bytes,
                None,
                Some(&file.sha256),
            )?;
        }
    }
    verify_natives(root, &record.natives_relative_path)?;
    if record.native_dlls != collect_verified_natives(root)? {
        return Err("extracted native DLL bytes or inventory changed".into());
    }
    let count = tl_game::vanilla_evidence::validate_install_files(&record.version, root)
        .map_err(|e| e.to_string())?;
    if count != record.minecraft_verified_files {
        return Err("Forge Minecraft file inventory changed".into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_config_remains_user_editable_but_scripts_and_mods_do_not() {
        assert!(is_mutable_user_config("config/modsettings.toml"));
        assert!(is_mutable_user_config("options.txt"));
        assert!(!is_mutable_user_config("mods/mod.jar"));
        assert!(!is_mutable_user_config("kubejs/startup_scripts/init.js"));
        assert!(!is_mutable_user_config("config/unsafe.jar"));
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::create_dir(dir.join("config")).unwrap();
        fs::write(dir.join("config/modsettings.toml"), b"changed by Minecraft").unwrap();
        assert!(verify_mutable_config_path(&dir, "config/modsettings.toml").is_ok());
        assert!(verify_mutable_config_path(&dir, "../other").is_err());
    }

    #[test]
    fn patched_client_is_required_for_forge_ready_and_reverify() {
        let required = InstalledForgeLibrary {
            relative_path:
                "libraries/net/minecraftforge/forge/1.16.5-36.2.29/forge-1.16.5-36.2.29-client.jar"
                    .into(),
            official_sha1: "42710a50971139d0f218e438a211a0dbe9bbb3d2".into(),
            size_bytes: 1,
        };
        assert!(require_patched_client(&[required.clone()], "1.16.5", "36.2.29").is_ok());
        assert!(require_patched_client(&[], "1.16.5", "36.2.29").is_err());
        let stale = InstalledForgeLibrary {
            relative_path:
                "libraries/net/minecraftforge/forge/1.16.5-36.2.29/forge-1.16.5-36.2.29.jar".into(),
            ..required.clone()
        };
        assert!(require_patched_client(&[stale], "1.16.5", "36.2.29").is_err());
        assert!(require_patched_client(&[required], "1.16.5", "36.2.34").is_err());
    }

    #[test]
    fn jarsplitter_extra_is_required_for_ready_and_rehash_inventory() {
        let profile_version = serde_json::json!({
            "arguments":{"game":["--launchTarget","fmlclient","--fml.mcpVersion","20210115.111550"]}
        });
        let extra = InstalledForgeLibrary {
            relative_path: "libraries/net/minecraft/client/1.16.5-20210115.111550/client-1.16.5-20210115.111550-extra.jar".into(),
            official_sha1: "4aef4168f94d97bc73bf510213b2518942f931d1".into(),
            size_bytes: 1234,
        };
        assert!(require_minecraft_extra(&[extra.clone()], &profile_version, "1.16.5").is_ok());
        assert!(require_minecraft_extra(&[], &profile_version, "1.16.5").is_err());
        assert!(require_minecraft_extra(&[extra.clone()], &profile_version, "1.17").is_err());
        let altered = InstalledForgeLibrary {
            relative_path: "libraries/net/minecraft/client/1.16.5-20210115.111550/client-1.16.5-20210115.111550.jar".into(),
            ..extra.clone()
        };
        assert!(require_minecraft_extra(&[altered], &profile_version, "1.16.5").is_err());
        let no_sha = InstalledForgeLibrary {
            official_sha1: String::new(),
            ..extra
        };
        assert!(require_minecraft_extra(&[no_sha], &profile_version, "1.16.5").is_err());
        let unsafe_version = serde_json::json!({
            "arguments":{"game":["--fml.mcpVersion","../escape"]}
        });
        assert!(require_minecraft_extra(&[], &unsafe_version, "1.16.5").is_err());
    }

    #[test]
    fn ready_requires_all_generated_minecraft_client_resources() {
        let version = serde_json::json!({"arguments":{
            "game":["--launchTarget","fmlclient","--fml.mcpVersion","20210115.111550"]
        }});
        let entry = |classifier: &str| InstalledForgeLibrary {
            relative_path: format!(
                "libraries/net/minecraft/client/1.16.5-20210115.111550/client-1.16.5-20210115.111550-{classifier}.jar"
            ),
            official_sha1: "4aef4168f94d97bc73bf510213b2518942f931d1".into(),
            size_bytes: 42,
        };
        let extra = entry("extra");
        let slim = entry("slim");
        let srg = entry("srg");
        assert!(require_all_minecraft_generated_jars(
            &[extra.clone(), slim.clone(), srg.clone()], &version, "1.16.5"
        ).is_ok());
        assert!(require_all_minecraft_generated_jars(
            &[extra.clone(), slim.clone()], &version, "1.16.5"
        ).is_err());
        assert!(require_all_minecraft_generated_jars(
            &[extra.clone(), srg.clone()], &version, "1.16.5"
        ).is_err());
        assert!(require_all_minecraft_generated_jars(
            &[slim, srg], &version, "1.16.5"
        ).is_err());
        assert!(require_all_minecraft_generated_jars(
            &[extra], &version, "1.16.6"
        ).is_err());
    }

    #[test]
    fn absent_publish_marker_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        assert!(reverify_published_forge(temp.path(), &"0".repeat(64)).is_err());
    }

    #[test]
    fn forged_ready_json_cannot_issue_publish_permit() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(FORGE_INSTALL_MARKER),
            r#"{"schema":"tl-forge-published/v1","state":"READY"}"#,
        )
        .unwrap();
        assert!(reverify_published_forge(temp.path(), &"0".repeat(64)).is_err());
    }

    #[test]
    fn published_file_hash_changes_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("mod.jar"), b"original").unwrap();
        let expected = format!("{:x}", Sha256::digest(b"original"));
        assert!(check_file(temp.path(), "mod.jar", 8, None, Some(&expected)).is_ok());
        fs::write(temp.path().join("mod.jar"), b"modified").unwrap();
        assert!(check_file(temp.path(), "mod.jar", 8, None, Some(&expected)).is_err());
        assert!(check_file(temp.path(), "../escape", 8, None, Some(&expected)).is_err());
    }

    #[test]
    fn forge_mod_inventory_rejects_extra_missing_and_case_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let mods_dir = temp.path().join("mods");
        fs::create_dir(&mods_dir).unwrap();
        fs::write(mods_dir.join("Main.jar"), b"main").unwrap();
        let original = VerifiedModReceipt {
            project_id: 1,
            file_id: 2,
            required: true,
            relative_path: "mods/Main.jar".into(),
            bytes: 4,
            upstream_sha1: "a".repeat(40),
            observed_sha256: "b".repeat(64),
        };
        assert!(verify_exact_mod_inventory(temp.path(), &[original.clone()]).is_ok());
        fs::write(mods_dir.join("extra.jar"), b"unknown").unwrap();
        assert!(verify_exact_mod_inventory(temp.path(), &[original.clone()]).is_err());
        fs::remove_file(mods_dir.join("extra.jar")).unwrap();
        let mut case_duplicate = original.clone();
        case_duplicate.relative_path = "mods/main.JAR".into();
        assert!(
            verify_exact_mod_inventory(temp.path(), &[original.clone(), case_duplicate]).is_err()
        );
        fs::remove_file(mods_dir.join("Main.jar")).unwrap();
        assert!(verify_exact_mod_inventory(temp.path(), &[original]).is_err());
    }

    #[test]
    fn forge_receipt_digest_rejects_self_consistent_mutable_json_changes() {
        let trusted = br#"{"mainClass":"trusted","hash":"abcd"}"#;
        let digest = format!("{:x}", Sha256::digest(trusted));
        assert!(verify_receipt_anchor(trusted, &digest).is_ok());
        let forged = br#"{"mainClass":"evil","hash":"changed"}"#;
        assert!(verify_receipt_anchor(forged, &digest).is_err());
        assert!(verify_receipt_anchor(trusted, "").is_err());
    }
}
