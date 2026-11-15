//! Restricted Modrinth provider staging. Does not execute any loader or
//! publish a playable instance. HTTPS downloads are anonymous and hash-checked.
use crate::{safe_path, PackError, PackMetadata, PackSource};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use zip::ZipArchive;

const MAX_INDEX: u64 = 4 * 1024 * 1024;
const MAX_FILE: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Index {
    files: Vec<IndexFile>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexFile {
    path: String,
    file_size: u64,
    hashes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthInstalledFile {
    pub target: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthStageReport {
    pub installed: Vec<ModrinthInstalledFile>,
    pub blocked: Vec<String>,
    pub failed: Vec<String>,
    pub cancelled: bool,
    pub ready: bool,
}

pub fn approved_modrinth_url(url: &str) -> bool {
    let Ok(uri) = reqwest::Url::parse(url) else {
        return false;
    };
    uri.scheme() == "https"
        && uri.host_str() == Some("cdn.modrinth.com")
        && uri.port().is_none()
        && uri.username().is_empty()
        && uri.password().is_none()
        && uri.fragment().is_none()
}

fn stage_copy<R: Read>(
    mut source: R,
    target: &mut File,
    expected_size: u64,
    sha512: &str,
    sha1: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<String, String> {
    if expected_size == 0
        || expected_size > MAX_FILE
        || sha512.len() != 128
        || !sha512.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("invalid trusted size or SHA-512".into());
    }
    let mut h512 = Sha512::new();
    let mut h1 = Sha1::new();
    let mut h256 = Sha256::new();
    let mut count = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let bytes = source.read(&mut buf).map_err(|_| "download read failure")?;
        if bytes == 0 {
            break;
        }
        count = count.checked_add(bytes as u64).ok_or("download overflow")?;
        if count > expected_size {
            return Err("download exceeds manifest size".into());
        }
        target
            .write_all(&buf[..bytes])
            .map_err(|_| "staging write failure")?;
        h512.update(&buf[..bytes]);
        h1.update(&buf[..bytes]);
        h256.update(&buf[..bytes]);
    }
    if count != expected_size || format!("{:x}", h512.finalize()) != sha512.to_ascii_lowercase() {
        return Err("Modrinth SHA-512 or size mismatch".into());
    }
    if let Some(known) = sha1 {
        if known.len() != 40
            || !known.bytes().all(|c| c.is_ascii_hexdigit())
            || format!("{:x}", h1.finalize()) != known.to_ascii_lowercase()
        {
            return Err("Modrinth SHA-1 mismatch".into());
        }
    }
    target.sync_all().map_err(|_| "staging sync failure")?;
    Ok(format!("{:x}", h256.finalize()))
}

/// The new stage must be under a private trusted root; archive integrity and
/// override extraction remain delegated to the existing ZIP-safe adapter.
/// Incompatible or blocked files are enumerated, never silently skipped.
pub fn stage_modrinth(
    archive: &Path,
    metadata: &PackMetadata,
    new_staging: &Path,
    cancelled: &AtomicBool,
) -> Result<ModrinthStageReport, PackError> {
    if metadata.source != PackSource::ModrinthMrpack {
        return Err(PackError::Unsupported("not a Modrinth mrpack".into()));
    }
    if new_staging.exists() {
        return Err(PackError::UnsafeArchive("stage already exists".into()));
    }
    let mut zip = ZipArchive::new(File::open(archive)?)?;
    let mut index_file = zip.by_name("modrinth.index.json")?;
    if index_file.size() > MAX_INDEX {
        return Err(PackError::UnsafeArchive("oversized Modrinth index".into()));
    }
    let mut bytes = Vec::new();
    index_file.take(MAX_INDEX + 1).read_to_end(&mut bytes)?;
    let index: Index = serde_json::from_slice(&bytes)
        .map_err(|_| PackError::Metadata("invalid Modrinth index".into()))?;
    if index.files.len() != metadata.files.len() {
        return Err(PackError::Metadata(
            "manifest file inventory mismatch".into(),
        ));
    }
    let mut by_path = HashMap::new();
    let mut aliases = HashSet::new();
    for file in index.files {
        safe_path(&file.path)?;
        if !aliases.insert(file.path.to_ascii_lowercase()) {
            return Err(PackError::Metadata("duplicate Modrinth path".into()));
        }
        by_path.insert(file.path.clone(), file);
    }
    for file in &metadata.files {
        let reference = by_path
            .get(&file.path)
            .ok_or_else(|| PackError::Metadata("unmatched Modrinth file".into()))?;
        if reference.file_size == 0
            || reference.file_size > MAX_FILE
            || file.sha512.as_deref() != reference.hashes.get("sha512").map(String::as_str)
        {
            return Err(PackError::Metadata(
                "missing trusted Modrinth size or checksum agreement".into(),
            ));
        }
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(ModrinthStageReport {
            installed: Vec::new(),
            blocked: Vec::new(),
            failed: Vec::new(),
            cancelled: true,
            ready: false,
        });
    }
    crate::stage_overrides(archive, metadata, new_staging)?;
    let mut report = ModrinthStageReport {
        installed: Vec::new(),
        blocked: Vec::new(),
        failed: Vec::new(),
        cancelled: false,
        ready: false,
    };
    for file in &metadata.files {
        if cancelled.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let reference = &by_path[&file.path];
        let Some(url) = file.urls.iter().find(|u| approved_modrinth_url(u)) else {
            report
                .blocked
                .push(format!("{}: no authorized Modrinth CDN URL", file.path));
            continue;
        };
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|_| PackError::Metadata("Modrinth client unavailable".into()))?;
        let response = match client.get(url).send() {
            Ok(response) if response.status().is_success() => response,
            _ => {
                report
                    .failed
                    .push(format!("{}: approved download failed", file.path));
                continue;
            }
        };
        let relative = safe_path(&file.path)?;
        let destination = new_staging.join(relative);
        if destination.exists() {
            report
                .failed
                .push(format!("{}: collides with existing override", file.path));
            continue;
        }
        if let Some(dir) = destination.parent() {
            fs::create_dir_all(dir)?;
        }
        let partial = destination.with_extension("tlpartial");
        let mut handle = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)
        {
            Ok(f) => f,
            Err(_) => {
                report
                    .failed
                    .push(format!("{}: staged path collision", file.path));
                continue;
            }
        };
        let checked = stage_copy(
            response,
            &mut handle,
            reference.file_size,
            file.sha512.as_deref().unwrap_or(""),
            file.sha1.as_deref(),
            cancelled,
        );
        drop(handle);
        match checked {
            Ok(sha256) => {
                if fs::rename(&partial, &destination).is_err() {
                    let _ = fs::remove_file(&partial);
                    report
                        .failed
                        .push(format!("{}: atomic staged rename failed", file.path));
                } else {
                    report.installed.push(ModrinthInstalledFile {
                        target: file.path.clone(),
                        sha256,
                        bytes: reference.file_size,
                    });
                }
            }
            Err(reason) => {
                let _ = fs::remove_file(&partial);
                if reason == "cancelled" {
                    report.cancelled = true;
                    break;
                }
                report.failed.push(format!("{}: {reason}", file.path));
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn blocks_unapproved_origins_and_credentials() {
        assert!(approved_modrinth_url(
            "https://cdn.modrinth.com/data/test/file.jar"
        ));
        for url in [
            "http://cdn.modrinth.com/x.jar",
            "https://cdn.modrinth.com.evil.test/x.jar",
            "https://user:pass@cdn.modrinth.com/x.jar",
            "https://127.0.0.1/x.jar",
            "https://cdn.modrinth.com:8443/x.jar",
        ] {
            assert!(!approved_modrinth_url(url), "{url}");
        }
    }
    #[test]
    fn mismatched_sha512_does_not_verify_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let mut out = File::create(temp.path().join("mod.tmp")).unwrap();
        let result = stage_copy(
            Cursor::new(b"mod fixture"),
            &mut out,
            11,
            &"a".repeat(128),
            None,
            &AtomicBool::new(false),
        );
        assert!(result.is_err());
    }
    #[test]
    fn validated_sha512_returns_calculated_sha256() {
        let bytes = b"mod fixture";
        let temp = tempfile::tempdir().unwrap();
        let mut out = File::create(temp.path().join("mod.tmp")).unwrap();
        let result = stage_copy(
            Cursor::new(bytes),
            &mut out,
            bytes.len() as u64,
            &format!("{:x}", Sha512::digest(bytes)),
            None,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result, format!("{:x}", Sha256::digest(bytes)));
    }
    #[test]
    fn cancellation_never_returns_verified_hash() {
        let bytes = b"mod fixture";
        let temp = tempfile::tempdir().unwrap();
        let mut out = File::create(temp.path().join("mod.tmp")).unwrap();
        let result = stage_copy(
            Cursor::new(bytes),
            &mut out,
            bytes.len() as u64,
            &format!("{:x}", Sha512::digest(bytes)),
            None,
            &AtomicBool::new(true),
        );
        assert_eq!(result.unwrap_err(), "cancelled");
    }
}
