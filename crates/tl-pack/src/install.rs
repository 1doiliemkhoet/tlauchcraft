//! Download only files explicitly resolved by the approved CurseForge API.
//! Creates an isolated staging directory and never publishes a READY instance.
use crate::{
    curseforge::{
        approved_download_url, CurseForgeFile, CurseForgeResolveReport, FileAvailability,
    },
    PackError, PackMetadata,
};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

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
fn copy_verify<R: Read>(
    mut reader: R,
    out: &mut File,
    expected_size: u64,
    expected_sha1: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<(String, u64), String> {
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
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or("download size overflow")?;
        if total > expected_size {
            return Err("download exceeds declared size".into());
        }
        out.write_all(&buf[..count])
            .map_err(|_| "disk write failed")?;
        sha1.update(&buf[..count]);
        sha256.update(&buf[..count]);
    }
    if total != expected_size {
        return Err("download size mismatch".into());
    }
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

/// Construct a completely anonymous CDN request. The key is deliberately
/// absent from this API so a future caller cannot accidentally leak it.
fn build_cdn_request(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<reqwest::blocking::Request, String> {
    if !approved_download_url(url) {
        return Err("unapproved download origin".into());
    }
    client
        .get(url)
        .build()
        .map_err(|_| "invalid CDN request".into())
}

/// Download a CurseForge file only through an exact officially approved HTTPS host.
/// Redirects disabled: no SSRF escape or leaking the key to redirect destinations.
fn download_file(
    entry: &CurseForgeFile,
    stage: &Path,
    cancelled: &AtomicBool,
) -> Result<InstalledMod, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if entry.availability != FileAvailability::Available {
        return Err(format!(
            "file {}:{} unavailable: {}",
            entry.project_id, entry.file_id, entry.reason
        ));
    }
    let url = entry.download_url.as_deref().ok_or("no download URL")?;
    if !approved_download_url(url) {
        return Err("unapproved download origin".into());
    }
    let filename = entry.file_name.as_deref().ok_or("missing file name")?;
    if !crate::curseforge::safe_mod_file_name(filename) {
        return Err("unsafe mod filename".into());
    }
    let expected_size = entry.expected_size.ok_or("missing expected size")?;
    // Upstream must provide an independently known checksum; a freshly computed
    // SHA-256 alone cannot establish that the downloaded bytes are authentic.
    let expected_sha1 = entry
        .sha1
        .as_deref()
        .ok_or("missing trusted SHA-1; manual verification required")?;
    let relative_path = format!("mods/{filename}");
    crate::safe_path(&relative_path).map_err(|_| "unsafe mod relative path")?;
    let destination = stage.join(&relative_path);
    if destination.exists() {
        return Err("mod file collides with overrides or another mod".into());
    }
    let part = stage.join(format!("mods/{filename}.part"));
    if part.exists() {
        return Err("partial download collision".into());
    }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| "network client initialization failed")?;
    // Do not attach API credentials, Authorization, cookies, or user-configured headers
    // to third-party download origins, regardless of transport security.
    let request = build_cdn_request(&client, url)?;
    let response = client
        .execute(request)
        .map_err(|_| "mod download network failure")?;
    if !response.status().is_success() {
        return Err(format!("mod CDN HTTP {}", response.status().as_u16()));
    }
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)
        .map_err(|_| "cannot create isolated partial download")?;
    let result = copy_verify(
        response,
        &mut out,
        expected_size,
        Some(expected_sha1),
        cancelled,
    );
    drop(out);
    match result {
        Ok((actual_sha256, bytes)) => {
            fs::rename(&part, &destination).map_err(|_| {
                let _ = fs::remove_file(&part);
                "atomic mod rename failed"
            })?;
            Ok(InstalledMod {
                project_id: entry.project_id,
                file_id: entry.file_id,
                relative_path,
                actual_sha256,
                bytes,
            })
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
    archive: &Path,
    metadata: &PackMetadata,
    resolved: &CurseForgeResolveReport,
    new_staging: &Path,
    cancelled: &AtomicBool,
) -> Result<StageReport, PackError> {
    if metadata.source != crate::PackSource::CurseforgeZip {
        return Err(PackError::Unsupported("not a CurseForge export".into()));
    }
    if resolved.files.len() != metadata.files.len() {
        return Err(PackError::Metadata(
            "incomplete dependency resolution".into(),
        ));
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(StageReport {
            status: InstallStatus::Cancelled,
            installed: Vec::new(),
            blocked: Vec::new(),
            failures: Vec::new(),
            staging_path: new_staging.display().to_string(),
            ready: false,
        });
    }
    // Validate the complete resolver plan before touching disk or network.
    for (declared, file) in metadata.files.iter().zip(&resolved.files) {
        if declared.project_id != Some(file.project_id)
            || declared.file_id != Some(file.file_id)
            || declared.required != file.required
        {
            return Err(PackError::Metadata(
                "resolver plan does not match archive".into(),
            ));
        }
    }
    // On Windows two distinct IDs may resolve to the same case-folded mod path.
    // Detect collisions *before* extracting or making any network requests.
    let mut planned_mod_names = HashSet::new();
    for file in &resolved.files {
        if file.availability != FileAvailability::Available {
            continue;
        }
        let name = file.file_name.as_deref().ok_or_else(|| {
            PackError::Metadata("approved file has no target filename".into())
        })?;
        if !crate::curseforge::safe_mod_file_name(name)
            || !planned_mod_names.insert(name.to_ascii_lowercase())
        {
            return Err(PackError::Metadata(
                "unsafe or duplicate Windows mod destination".into(),
            ));
        }
        if file.sha1.as_ref().is_none_or(|sha| {
            sha.len() != 40 || !sha.bytes().all(|c| c.is_ascii_hexdigit())
        }) || file.expected_size.is_none_or(|size| size == 0 || size > MAX_MOD_BYTES)
        {
            return Err(PackError::Metadata(
                "approved file has no valid trusted SHA-1/size".into(),
            ));
        }
    }
    // ZIP integrity and override extraction happen before any remote requests.
    crate::stage_overrides(archive, metadata, new_staging)?;
    let mods = new_staging.join("mods");
    fs::create_dir_all(&mods)?;
    let mut installed = Vec::new();
    let mut blocked = resolved
        .dependency_actions
        .iter()
        .map(|action| {
            format!(
                "dependency projectID={} parent={}/{} required={} fileID=UNRESOLVED: {}",
                action.project_id,
                action.parent_project_id,
                action.parent_file_id,
                action.required,
                action.reason
            )
        })
        .collect::<Vec<_>>();
    let mut failures = Vec::new();
    for (declared, file) in metadata.files.iter().zip(&resolved.files) {
        if declared.project_id != Some(file.project_id)
            || declared.file_id != Some(file.file_id)
            || declared.required != file.required
        {
            failures.push(format!(
                "mismatched resolver entry {}:{}",
                file.project_id, file.file_id
            ));
            break;
        }
        if cancelled.load(Ordering::Relaxed) {
            return Ok(StageReport {
                status: InstallStatus::Cancelled,
                installed,
                blocked,
                failures,
                staging_path: new_staging.display().to_string(),
                ready: false,
            });
        }
        if file.availability != FileAvailability::Available {
            blocked.push(format!(
                "{}:{} {:?}: {}",
                file.project_id, file.file_id, file.availability, file.reason
            ));
            continue;
        }
        match download_file(file, new_staging, cancelled) {
            Ok(item) => installed.push(item),
            Err(e) if e == "cancelled" => {
                return Ok(StageReport {
                    status: InstallStatus::Cancelled,
                    installed,
                    blocked,
                    failures,
                    staging_path: new_staging.display().to_string(),
                    ready: false,
                })
            }
            Err(e) => failures.push(format!("{}:{} {e}", file.project_id, file.file_id)),
        }
    }
    Ok(StageReport {
        status: if failures.is_empty() {
            InstallStatus::NeedsAction
        } else {
            InstallStatus::Failed
        },
        installed,
        blocked,
        failures,
        staging_path: new_staging.display().to_string(),
        ready: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn cdn_request_is_anonymous_even_when_api_key_is_configured() {
        // Fake HTTP request: does not send network traffic or need a real secret.
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let secret = "fake-api-key-DO-NOT-TRANSMIT";
        let _ = secret; // Intentionally no credential argument to the request builder.
        let request = build_cdn_request(
            &client,
            "https://edge.forgecdn.net/files/100/200/example.jar",
        )
        .unwrap();
        assert_eq!(request.method(), reqwest::Method::GET);
        assert!(
            request.headers().is_empty(),
            "CDN request must have zero custom headers"
        );
        for name in [
            "x-api-key",
            "authorization",
            "proxy-authorization",
            "cookie",
        ] {
            assert!(!request.headers().contains_key(name), "leaked {name}");
        }
        assert_eq!(request.url().host_str(), Some("edge.forgecdn.net"));
    }

    #[test]
    fn cdn_request_rejects_redirect_target_and_credentials() {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        for url in [
            "https://127.0.0.1/x.jar",
            "https://edge.forgecdn.net.evil.test/x.jar",
            "https://user:pass@edge.forgecdn.net/x.jar",
            "http://edge.forgecdn.net/x.jar",
            "https://edge.forgecdn.net:8443/x.jar",
        ] {
            assert!(build_cdn_request(&client, url).is_err(), "{url}");
        }
    }

    #[test]
    fn cancelled_staging_does_not_touch_disk_or_return_ready() {
        let cancelled = AtomicBool::new(true);
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("unused.zip");
        let stage = temp.path().join("unused-stage");
        let metadata = PackMetadata {
            name: "fixture".into(),
            source: crate::PackSource::CurseforgeZip,
            game_version: "1.16.5".into(),
            loader: crate::Loader {
                kind: "forge".into(),
                version: Some("36.2.34".into()),
            },
            archive_sha256: String::new(),
            archive_entries: 0,
            uncompressed_bytes: 0,
            files: Vec::new(),
            overrides: Vec::new(),
            warnings: Vec::new(),
        };
        let resolved = CurseForgeResolveReport {
            files: Vec::new(),
            resolved_count: 0,
            needs_action_count: 0,
            dependency_actions: Vec::new(),
            ready: false,
        };
        let outcome =
            stage_curseforge_downloads(&archive, &metadata, &resolved, &stage, &cancelled).unwrap();
        assert_eq!(outcome.status, InstallStatus::Cancelled);
        assert!(!outcome.ready);
        assert!(!stage.exists());
    }

    #[test]
    fn untrusted_hash_is_rejected_before_any_network_request() {
        let item = CurseForgeFile {
            project_id: 100,
            file_id: 200,
            required: true,
            file_name: Some("fixture.jar".into()),
            download_url: Some("https://edge.forgecdn.net/files/100/200/fixture.jar".into()),
            sha1: None,
            expected_size: Some(12),
            availability: FileAvailability::Available,
            reason: String::new(),
            dependencies: Vec::new(),
        };
        let temp = tempfile::tempdir().unwrap();
        let outcome = download_file(&item, temp.path(), &AtomicBool::new(false));
        assert_eq!(
            outcome.unwrap_err(),
            "missing trusted SHA-1; manual verification required"
        );
    }

    #[test]
    fn verified_fixture_requires_matching_sha1_and_size() {
        let sample = b"sample mod binary fixture";
        let mut out = tempfile::tempfile().unwrap();
        let cancel = AtomicBool::new(false);
        let hash = format!("{:x}", Sha1::digest(sample));
        let (sha256, count) = copy_verify(
            Cursor::new(sample),
            &mut out,
            sample.len() as u64,
            Some(&hash),
            &cancel,
        )
        .unwrap();
        assert_eq!(count, sample.len() as u64);
        assert_eq!(sha256.len(), 64);
    }
    #[test]
    fn rejects_corruption_and_truncation() {
        let data = b"local fixture";
        let cancel = AtomicBool::new(false);
        assert!(copy_verify(
            Cursor::new(data),
            &mut tempfile::tempfile().unwrap(),
            data.len() as u64,
            Some(&"0".repeat(40)),
            &cancel
        )
        .is_err());
        assert!(copy_verify(
            Cursor::new(data),
            &mut tempfile::tempfile().unwrap(),
            data.len() as u64 + 1,
            None,
            &cancel
        )
        .is_err());
    }
    #[test]
    fn cancellation_stops_before_copy() {
        let cancel = AtomicBool::new(true);
        assert!(copy_verify(
            Cursor::new(b"data"),
            &mut tempfile::tempfile().unwrap(),
            4,
            None,
            &cancel
        )
        .is_err());
    }
}
