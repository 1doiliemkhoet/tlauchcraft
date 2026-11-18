//! Version-specific audited Forge installer acquisition and approved host runner.
//! This supports the initial pinned 1.16.5/36.2.29 adapter only. Future loaders
//! must supply their own published version/checksum policy, not reuse this pin.
use crate::forge_publish::ApprovedForgePublishInputs;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tl_loader::{
    forge_graph::{BoundValue, ForgeBindings, InstallerEntryEvidence},
    forge_playable::{
        ForgeLibraryOrigin, PackForgeSelection, VerifiedForgeJava, VerifiedForgeLibrary,
    },
    forge_provenance::{audit_official_forge_metadata, SourceRequirement},
    runtime::{VerifiedProcessorExecution, VerifiedProcessorJar},
};
use zip::ZipArchive;

const MAX_LIB: u64 = 256 * 1024 * 1024;

/// Desktop-facing version gate. The Desktop crate intentionally consumes
/// tl-pack rather than depending directly on tl-loader.
pub fn ensure_supported_forge_release(minecraft: &str, forge: &str) -> Result<(), String> {
    tl_loader::forge_family::approved_forge_release(minecraft, forge)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub struct PreparedOfficialForge {
    selection: PackForgeSelection,
    profile_json: String,
    derived: Value,
    parent: Value,
    execution: VerifiedProcessorExecution,
    java: VerifiedForgeJava,
    maven: Vec<VerifiedProcessorJar>,
    generated: Vec<VerifiedForgeLibrary>,
}
impl PreparedOfficialForge {
    pub fn approved(&self) -> ApprovedForgePublishInputs<'_> {
        ApprovedForgePublishInputs {
            selection: &self.selection,
            profile_json: &self.profile_json,
            derived: &self.derived,
            mojang_parent: &self.parent,
            processor_execution: &self.execution,
            java: &self.java,
            maven: &self.maven,
            installer_or_generated: &self.generated,
        }
    }
}

fn no_symlink(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("official Forge file path must be absolute".into());
    }
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!(
                "official Forge input/cache path contains a symlink: {} (resolve the path or choose a real Java executable)",
                ancestor.display()
            ));
        }
    }
    Ok(())
}
/// The caller already checked the Java major by running the selected executable.
/// Windows installers can expose java.exe through a legitimate PATH alias
/// (Oracle `javapath` uses symlinks). Use its stable real executable for the
/// guarded Forge runner and local SHA1 binding; never drop path-safety checks
/// for cached Mojang/Forge libraries or staging destinations.
fn resolve_selected_java(java_path: &Path) -> Result<std::path::PathBuf, String> {
    if !java_path.is_absolute() {
        return Err("Selected Forge Java executable must have an absolute path".into());
    }
    let resolved = fs::canonicalize(java_path)
        .map_err(|e| format!("Could not resolve selected Java executable: {e}"))?;
    no_symlink(&resolved)?;
    let meta = fs::symlink_metadata(&resolved).map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() {
        return Err("Selected Java resolves to a non-file".into());
    }
    Ok(resolved)
}

fn verify(path: &Path, expected: &str, size: Option<u64>, limit: u64) -> Result<(), String> {
    no_symlink(path)?;
    if expected.len() != 40 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("missing trusted upstream SHA-1".into());
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file()
        || metadata.len() == 0
        || metadata.len() > limit
        || size.is_some_and(|s| s != metadata.len())
    {
        return Err(format!(
            "official Forge file size/type mismatch: {}",
            path.display()
        ));
    }
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    if format!("{:x}", digest.finalize()) != expected.to_ascii_lowercase() {
        return Err(format!("official Forge SHA-1 mismatch: {}", path.display()));
    }
    Ok(())
}
fn observed_sha1(path: &Path) -> Result<String, String> {
    no_symlink(path)?;
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn network_client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(45))
        .connect_timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())
}

/// Downloads only an exact canonical URL whose digest came from the pinned
/// installer/official Mojang metadata. Never follows redirects, never overwrites
/// an invalid existing cache artifact.
fn fetch_verified(
    client: &Client,
    target: &Path,
    url: &str,
    sha1: &str,
    size: Option<u64>,
    limit: u64,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    if cancelled.load(Ordering::SeqCst) {
        return Err("official Forge install cancelled".into());
    }
    no_symlink(target)?;
    if fs::symlink_metadata(target).is_ok() {
        return verify(target, sha1, size, limit);
    }
    if !url.starts_with("https://maven.minecraftforge.net/")
        && !url.starts_with("https://repo.maven.apache.org/maven2/")
        && !url.starts_with("https://libraries.minecraft.net/")
    {
        return Err("noncanonical Forge Maven URL rejected".into());
    }
    let mut reply = client.get(url).send().map_err(|e| e.to_string())?;
    if reply.status() != reqwest::StatusCode::OK
        || reply.content_length().is_some_and(|n| n == 0 || n > limit)
    {
        return Err("Forge Maven origin returned an invalid status/length".into());
    }
    let parent = target.parent().ok_or("invalid verified cache parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    no_symlink(parent)?;
    let temp = target.with_extension("tl-verified-download");
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut written = 0u64;
        let mut buf = [0u8; 64 * 1024];
        loop {
            if cancelled.load(Ordering::SeqCst) {
                return Err("official Forge download cancelled".into());
            }
            let n = reply.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            written = written.checked_add(n as u64).ok_or("size overflow")?;
            if written > limit {
                return Err("Forge Maven transfer exceeded file limit".into());
            }
            output.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        verify(&temp, sha1, size, limit)?;
        fs::rename(&temp, target).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
fn extract_verified_entry(
    installer: &Path,
    name: &str,
    target: &Path,
    sha1: Option<&str>,
    size: Option<u64>,
    limit: u64,
) -> Result<String, String> {
    no_symlink(target)?;
    if fs::symlink_metadata(target).is_ok() {
        return Err("Forge installer extraction destination occupied".into());
    }
    let zipfile = File::open(installer).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(zipfile).map_err(|e| e.to_string())?;
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("official installer entry missing: {name}"))?;
    if entry.is_dir()
        || entry.size() == 0
        || entry.size() > limit
        || size.is_some_and(|n| n != entry.size())
    {
        return Err("Forge embedded entry has invalid size/type".into());
    }
    let parent = target.parent().ok_or("no embedded target parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    no_symlink(parent)?;
    let mut out = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)
        .map_err(|e| e.to_string())?;
    let copy = std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string());
    if !copy.as_ref().is_ok_and(|n| *n == entry.size()) {
        drop(out);
        let _ = fs::remove_file(target);
        return Err("Forge embedded entry copy incomplete".into());
    }
    out.sync_all().map_err(|e| e.to_string())?;
    drop(out);
    let digest = observed_sha1(target)?;
    if let Some(expected) = sha1 {
        if let Err(error) = verify(target, expected, size, limit) {
            let _ = fs::remove_file(target);
            return Err(error);
        }
    }
    Ok(digest)
}
fn installer_json(installer: &Path, name: &str) -> Result<String, String> {
    let archive = File::open(installer).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(archive).map_err(|e| e.to_string())?;
    let mut entry = zip.by_name(name).map_err(|e| e.to_string())?;
    if entry.size() > 2 * 1024 * 1024 {
        return Err("official Forge metadata exceeds bounded size".into());
    }
    let mut data = String::new();
    entry.read_to_string(&mut data).map_err(|e| e.to_string())?;
    Ok(data)
}

/// Download/reuse immutable Mojang/Forge caches; execute the exact four verified
/// processors under a native Windows Job Object; retain an opaque Rust proof.
/// All user-visible READY state is left to the independent publisher.
#[allow(clippy::too_many_arguments)]
pub fn prepare_official_windows_1165(
    archive_sha256: &str,
    selected_minecraft: &str,
    selected_forge: &str,
    operation_id: &str,
    data_dir: &Path,
    java_path: &Path,
    explicitly_approved_host_execution: bool,
    cancelled: &AtomicBool,
) -> Result<PreparedOfficialForge, String> {
    let release =
        tl_loader::forge_family::approved_forge_release(selected_minecraft, selected_forge)
            .map_err(|e| e.to_string())?;
    if !explicitly_approved_host_execution
        || operation_id.len() < 8
        || operation_id.len() > 64
        || !operation_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || archive_sha256.len() != 64
        || !archive_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("no approved official Forge adapter or consent for this version".into());
    }
    no_symlink(data_dir)?;
    let java_executable = resolve_selected_java(java_path)?;
    let java_path = java_executable.as_path();
    let version = tl_game::vanilla::resolve_official_version(selected_minecraft)
        .map_err(|e| format!("official Minecraft metadata unavailable: {e}"))?;
    let mojang_cache = data_dir.join("cache").join("mojang");
    fs::create_dir_all(&mojang_cache).map_err(|e| e.to_string())?;
    // Official Mojang download metadata and object hashes are verified here,
    // not guessed from the modpack ZIP or frontend.
    tl_game::vanilla::install_vanilla(&version, &mojang_cache, cancelled)
        .map_err(|e| format!("Mojang client/cache preparation: {e}"))?;
    let minecraft_client = mojang_cache
        .join("versions")
        .join(release.minecraft)
        .join(format!("{}.jar", release.minecraft));
    let client_sha = version["downloads"]["client"]["sha1"]
        .as_str()
        .ok_or("missing official Mojang client SHA-1")?;
    let client_url = version["downloads"]["client"]["url"]
        .as_str()
        .ok_or("missing official Mojang client URL")?;
    verify(&minecraft_client, client_sha, None, MAX_LIB)?;
    let client = network_client()?;
    let forge_cache = data_dir.join("cache").join("forge-official");
    fs::create_dir_all(&forge_cache).map_err(|e| e.to_string())?;
    let installer = forge_cache.join(release.installer_filename());
    fetch_verified(
        &client,
        &installer,
        &release.installer_url(),
        release.installer_sha1,
        None,
        32 * 1024 * 1024,
        cancelled,
    )?;
    let profile_json = installer_json(&installer, "install_profile.json")?;
    let derived_text = installer_json(&installer, "version.json")?;
    let derived: Value = serde_json::from_str(&derived_text).map_err(|e| e.to_string())?;
    release
        .verify_metadata(&profile_json, &derived)
        .map_err(|e| format!("official Forge metadata audit failed: {e}"))?;
    let audit =
        audit_official_forge_metadata(&profile_json, &derived).map_err(|e| e.to_string())?;
    let mut artifact_map = BTreeMap::<String, VerifiedProcessorJar>::new();
    let mut artifact_targets = BTreeSet::new();
    for req in audit
        .profile_libraries
        .iter()
        .chain(audit.derived_libraries.iter())
    {
        if let SourceRequirement::CanonicalMaven { url } = &req.source {
            if !artifact_targets.insert((req.coordinate.clone(), req.sha1.clone())) {
                continue;
            }
            let rel = crate::safe_path(&req.relative_path).map_err(|e| e.to_string())?;
            let destination = forge_cache.join("libraries").join(rel);
            fetch_verified(
                &client,
                &destination,
                url,
                &req.sha1,
                Some(req.size_bytes),
                MAX_LIB,
                cancelled,
            )?;
            artifact_map.insert(
                req.coordinate.clone(),
                VerifiedProcessorJar {
                    coordinate: req.coordinate.clone(),
                    path: destination,
                    sha1: req.sha1.clone(),
                    source: url.to_owned(),
                },
            );
        }
    }
    let java_sha = observed_sha1(java_path)?;
    let exec_parent = data_dir.join("cache").join("forge-execution");
    fs::create_dir_all(&exec_parent).map_err(|e| e.to_string())?;
    let exec_stage = exec_parent.join(operation_id);
    if fs::symlink_metadata(&exec_stage).is_ok() {
        return Err("Forge processor stage already exists; use a fresh operation".into());
    }
    let extract_dir = data_dir
        .join("cache")
        .join("forge-inputs")
        .join(operation_id);
    let extract_parent = extract_dir.parent().ok_or("invalid Forge input root")?;
    fs::create_dir_all(extract_parent).map_err(|e| e.to_string())?;
    no_symlink(extract_parent)?;
    fs::create_dir(&extract_dir).map_err(|e| e.to_string())?;
    let patch_file = extract_dir.join("client.lzma");
    let binpatch_sha = extract_verified_entry(
        &installer,
        "data/client.lzma",
        &patch_file,
        None,
        None,
        32 * 1024 * 1024,
    )?;
    let patch_report = extract_dir.join("binpatch.json");
    let report = json!({
        "status": "VERIFIED_INSTALLER_ENTRY_NOT_EXECUTED",
        "entry": "data/client.lzma",
        "installer": installer.to_string_lossy().to_string(),
        "extracted": patch_file.to_string_lossy().to_string(),
        "installer_sha1": release.installer_sha1,
        "extracted_sha1": binpatch_sha,
        "bytes": fs::metadata(&patch_file).map_err(|e| e.to_string())?.len(),
    });
    fs::write(&patch_report, report.to_string()).map_err(|e| e.to_string())?;
    let mut data = BTreeMap::new();
    data.insert(
        "BINPATCH".into(),
        BoundValue::VerifiedInstallerEntry(InstallerEntryEvidence {
            installer: installer.clone(),
            installer_sha1: release.installer_sha1.into(),
            entry: "data/client.lzma".into(),
            extracted: patch_file,
            extracted_sha1: binpatch_sha,
            report: patch_report,
        }),
    );
    data.insert(
        "MINECRAFT_JAR".into(),
        BoundValue::VerifiedExternal(VerifiedProcessorJar {
            coordinate: format!("net.minecraft:client:{}", release.minecraft),
            path: minecraft_client,
            sha1: client_sha.to_owned(),
            source: client_url.to_owned(),
        }),
    );
    for (key, rel) in [
        ("MAPPINGS", "generated/mappings.txt"),
        ("MC_SLIM", "generated/client-slim.jar"),
        ("MC_EXTRA", "generated/client-extra.jar"),
        ("MC_SRG", "generated/client-srg.jar"),
        ("PATCHED", "generated/forge-client.jar"),
    ] {
        data.insert(key.into(), BoundValue::Staged(rel.into()));
    }
    let bindings = ForgeBindings {
        minecraft: release.minecraft.into(),
        forge_version: release.forge.into(),
        java: java_path.to_owned(),
        stage_root: exec_stage.clone(),
        timeout_seconds: 180,
        artifacts: artifact_map.clone(),
        data,
    };
    let selection = PackForgeSelection {
        minecraft_version: selected_minecraft.into(),
        forge_version: selected_forge.into(),
        pack_archive_sha256: archive_sha256.to_owned(),
    };
    let executed = VerifiedProcessorExecution::run_official_windows_1165(
        &selection,
        &installer,
        &profile_json,
        &derived,
        &bindings,
        &java_sha,
        explicitly_approved_host_execution,
        cancelled,
    )
    .map_err(|e| format!("official Forge processor execution refused/failed: {e}"))?;
    // Source-empty artifacts can only be extracted from this already checksum-
    // pinned installer (or produced with matching official output hash).
    let mut generated = Vec::new();
    for req in audit
        .profile_libraries
        .iter()
        .chain(audit.derived_libraries.iter())
    {
        if matches!(&req.source, SourceRequirement::VerifiedInstallerOrGenerated) {
            let dest = exec_stage
                .join("libraries")
                .join(crate::safe_path(&req.relative_path).map_err(|e| e.to_string())?);
            if fs::symlink_metadata(&dest).is_err() {
                extract_verified_entry(
                    &installer,
                    &format!("maven/{}", req.relative_path),
                    &dest,
                    Some(&req.sha1),
                    Some(req.size_bytes),
                    MAX_LIB,
                )?;
            }
            verify(&dest, &req.sha1, Some(req.size_bytes), MAX_LIB)?;
            generated.push(VerifiedForgeLibrary {
                coordinate: req.coordinate.clone(),
                relative_path: req.relative_path.clone(),
                path: dest,
                official_sha1: req.sha1.clone(),
                size_bytes: req.size_bytes,
                origin: ForgeLibraryOrigin::VerifiedInstallerOrProcessor,
            });
        }
    }
    let mut maven = artifact_map.into_values().collect::<Vec<_>>();
    maven.sort_by(|a, b| a.coordinate.cmp(&b.coordinate));
    Ok(PreparedOfficialForge {
        selection,
        profile_json,
        derived,
        parent: version,
        execution: executed,
        java: VerifiedForgeJava {
            executable: java_path.to_owned(),
            sha1: java_sha,
            major: 8,
        },
        maven,
        generated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_verified_release_metadata_is_complete() {
        let profile: Value = serde_json::from_str(include_str!(
            "../../tl-loader/tests/fixtures/forge_36229_official_profile.json"
        ))
        .unwrap();
        let derived: Value = serde_json::from_str(include_str!(
            "../../tl-loader/tests/fixtures/forge_36229_official_version.json"
        ))
        .unwrap();
        for key in ["_comment_", "icon", "logo", "mirrorList", "welcome"] {
            assert!(
                profile.get(key).is_some(),
                "missing official profile field {key}"
            );
        }
        for key in ["_comment_", "logging", "releaseTime", "time", "type"] {
            assert!(
                derived.get(key).is_some(),
                "missing official version field {key}"
            );
        }
    }

    #[test]
    fn direct_java_executable_remains_usable() {
        let temp = tempfile::tempdir().unwrap();
        let java = temp.path().join("java.exe");
        fs::write(&java, b"fixture").unwrap();
        assert_eq!(
            resolve_selected_java(&java).unwrap(),
            fs::canonicalize(&java).unwrap()
        );
    }

    #[cfg(windows)]
    #[test]
    fn forge_accepts_verified_java_path_alias_but_not_cache_links() {
        use std::os::windows::fs::{symlink_dir, symlink_file};
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("java.exe");
        fs::write(&real, b"fixture").unwrap();
        let alias = temp.path().join("java-alias.exe");
        if symlink_file(&real, &alias).is_err() {
            // Symlink creation may require Developer Mode or special privileges.
            return;
        }
        assert!(no_symlink(&alias).is_err());
        assert_eq!(
            resolve_selected_java(&alias).unwrap(),
            fs::canonicalize(&real).unwrap()
        );
        let safe_cache = temp.path().join("private");
        fs::create_dir(&safe_cache).unwrap();
        let linked_cache = temp.path().join("redirected-cache");
        if symlink_dir(&safe_cache, &linked_cache).is_ok() {
            let err = no_symlink(&linked_cache.join("forge.jar")).unwrap_err();
            assert!(err.contains("redirected-cache"), "{err}");
        }
    }
    #[test]
    fn no_host_execution_without_consent_and_release_adapter() {
        let dir = tempfile::tempdir().unwrap();
        let no_consent = prepare_official_windows_1165(
            &"a".repeat(64),
            "1.16.5",
            "36.2.29",
            "pack-test-0001",
            dir.path(),
            &dir.path().join("java.exe"),
            false,
            &AtomicBool::new(false),
        );
        assert!(no_consent.is_err());
        let unsupported = prepare_official_windows_1165(
            &"a".repeat(64),
            "1.20.1",
            "47.3.0",
            "pack-test-0001",
            dir.path(),
            &dir.path().join("java.exe"),
            true,
            &AtomicBool::new(false),
        );
        assert!(unsupported.is_err());
    }
}
