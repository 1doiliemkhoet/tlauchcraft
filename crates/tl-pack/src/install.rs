//! Download only files explicitly resolved by the approved CurseForge API.
//! Creates an isolated staging directory and never publishes a READY instance.
use crate::{curseforge::{approved_download_url, CurseForgeFile, CurseForgeResolveReport, FileAvailability}, PackError, PackMetadata};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStatus {
    NeedsAction,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub project_id: u64,
    pub file_id: u64,
    pub relative_path: String,
    pub actual_sha256: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageReport {
    pub status: InstallStatus,
    pub installed: Vec<InstalledMod>,
    pub blocked: Vec<String>,
    pub failures: Vec<String>,
    pub staging_path: String,
    /// Always false until a trusted Minecraft+Forge installer and final validator exist.
    pub ready: bool,
}
const MAX_MOD_BYTES: u64 = 1024 * 1024 * 1024;

/// Validate the transfer using official size and SHA-1 when available.
/// A computed SHA-256 is recorded even when upstream publishes no hash.
fn copy_verify<R: Read>(mut reader: R, out: &mut File, expected_size: u64,
    expected_sha1: Option<&str>, cancelled: &AtomicBool) -> Result<(String, u64), String> {
    if expected_size == 0 || expected_size > MAX_MOD_BYTES {
        return Err("invalid declared file size".into());
    }
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut total = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let count = reader.read(&mut buf).map_err(|_| "network read failed")?;
        if count == 0 { break; }
        total = total.checked_add(count as u64).ok_or("download size overflow")?;
        if total > expected_size { return Err("download exceeds declared size".into()); }
        out.write_all(&buf[..count]).map_err(|_| "disk write failed")?;
        sha1.update(&buf[..count]);
        sha256.update(&buf[..count]);
    }
    if total != expected_size { return Err("download size mismatch".into()); }
    if let Some(expected) = expected_sha1 {
        if expected.len() != 40 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("invalid expected SHA-1".into());
        }
        if format!("{:x}", sha1.finalize()) != expected.to_ascii_lowercase() {
            return Err("download SHA-1 mismatch".into());
        }
    }
    out.sync_all().map_err(|_| "disk flush failed")?;
    Ok((format!("{:x}", sha256.finalize()), total))
}

/// Download a CurseForge file only through an exact officially approved HTTPS host.
/// Redirects disabled: no SSRF escape or leaking the key to redirect destinations.
fn download_file(entry: &CurseForgeFile, stage: &Path, api_key: &str,
    cancelled: &AtomicBool) -> Result<InstalledMod, String> {
    if cancelled.load(Ordering::Relaxed) { return Err("cancelled".into()); }
    if entry.availability != FileAvailability::Available {
        return Err(format!("file {}:{} unavailable: {}", entry.project_id, entry.file_id, entry.reason));
    }
    let url = entry.download_url.as_deref().ok_or("no download URL")?;
    if !approved_download_url(url) { return Err("unapproved download origin".into()); }
    let filename = entry.file_name.as_deref().ok_or("missing file name")?;
    if !crate::curseforge::safe_mod_file_name(filename) {
        return Err("unsafe mod filename".into());
    }
    let expected_size = entry.expected_size.ok_or("missing expected size")?;
    let relative_path = format!("mods/{filename}");
    crate::safe_path(&relative_path).map_err(|_| "unsafe mod relative path")?;
    let destination = stage.join(&relative_path);
    if destination.exists() { return Err("mod file collides with overrides or another mod".into()); }
    let part = stage.join(format!("mods/{filename}.part"));
    if part.exists() { return Err("partial download collision".into()); }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(180))
        .build().map_err(|_| "network client initialization failed")?;
    let response = client.get(url).header("x-api-key", api_key)
        .send().map_err(|_| "mod download network failure")?;
    if !response.status().is_success() { return Err(format!("mod CDN HTTP {}", response.status().as_u16())); }
    let mut out = OpenOptions::new().write(true).create_new(true).open(&part)
        .map_err(|_| "cannot create isolated partial download")?;
    let result = copy_verify(response, &mut out, expected_size, entry.sha1.as_deref(), cancelled);
    drop(out);
    match result {
        Ok((actual_sha256, bytes)) => {
            fs::rename(&part, &destination).map_err(|_| {
                let _ = fs::remove_file(&part);
                "atomic mod rename failed"
            })?;
            Ok(InstalledMod { project_id: entry.project_id,
                file_id: entry.file_id, relative_path, actual_sha256, bytes })
        }
        Err(err) => {
            let _ = fs::remove_file(&part);
            Err(err)
        }
    }
}

/// Starts a new staging transaction, retains incomplete staging on failure for
/// diagnosis/manual repair, and reports EVERY required missing mod.
/// Caller must never treat a staged directory as a playable, READY instance.
pub fn stage_curseforge_downloads(
    archive: &Path, metadata: &PackMetadata, resolved: &CurseForgeResolveReport,
    new_staging: &Path, api_key: &str, cancelled: &AtomicBool,
) -> Result<StageReport, PackError> {
    if metadata.source != crate::PackSource::CurseforgeZip {
        return Err(PackError::Unsupported("not a CurseForge export".into()));
    }
    if api_key.trim().is_empty() { return Err(PackError::Metadata("API key required".into())); }
    if resolved.files.len() != metadata.files.len() {
        return Err(PackError::Metadata("incomplete dependency resolution".into()));
    }
    // ZIP integrity and override extraction happen before any remote requests.
    crate::stage_overrides(archive, metadata, new_staging)?;
    let mods = new_staging.join("mods");
    fs::create_dir_all(&mods)?;
    let mut installed = Vec::new();
    let mut blocked = Vec::new();
    let mut failures = Vec::new();
    for (declared, file) in metadata.files.iter().zip(&resolved.files) {
        if declared.project_id != Some(file.project_id) || declared.file_id != Some(file.file_id)
            || declared.required != file.required {
            failures.push(format!("mismatched resolver entry {}:{}", file.project_id, file.file_id));
            break;
        }
        if cancelled.load(Ordering::Relaxed) {
            return Ok(StageReport {
                status: InstallStatus::Cancelled, installed, blocked, failures,
                staging_path: new_staging.display().to_string(), ready: false,
            });
        }
        if file.availability != FileAvailability::Available {
            blocked.push(format!("{}:{} {:?}: {}", file.project_id, file.file_id,
                file.availability, file.reason));
            continue;
        }
        match download_file(file, new_staging, api_key, cancelled) {
            Ok(item) => installed.push(item),
            Err(e) if e == "cancelled" => return Ok(StageReport {
                status: InstallStatus::Cancelled, installed, blocked, failures,
                staging_path: new_staging.display().to_string(), ready: false,
            }),
            Err(e) => failures.push(format!("{}:{} {e}", file.project_id, file.file_id)),
        }
    }
    Ok(StageReport {
        status: if failures.is_empty() { InstallStatus::NeedsAction } else { InstallStatus::Failed },
        installed, blocked, failures,
        staging_path: new_staging.display().to_string(), ready: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn verified_fixture_requires_matching_sha1_and_size() {
        let sample = b"sample mod binary fixture";
        let mut out = tempfile::tempfile().unwrap();
        let cancel = AtomicBool::new(false);
        let hash = format!("{:x}", Sha1::digest(sample));
        let (sha256, count) = copy_verify(Cursor::new(sample), &mut out,
            sample.len() as u64, Some(&hash), &cancel).unwrap();
        assert_eq!(count, sample.len() as u64);
        assert_eq!(sha256.len(), 64);
    }
    #[test]
    fn rejects_corruption_and_truncation() {
        let data = b"local fixture";
        let cancel = AtomicBool::new(false);
        assert!(copy_verify(Cursor::new(data), &mut tempfile::tempfile().unwrap(),
            data.len() as u64, Some(&"0".repeat(40)), &cancel).is_err());
        assert!(copy_verify(Cursor::new(data), &mut tempfile::tempfile().unwrap(),
            data.len() as u64 + 1, None, &cancel).is_err());
    }
    #[test]
    fn cancellation_stops_before_copy() {
        let cancel = AtomicBool::new(true);
        assert!(copy_verify(Cursor::new(b"data"), &mut tempfile::tempfile().unwrap(),
            4, None, &cancel).is_err());
    }
}
