//! CurseForge Core API resolver. No guessed CDN links or bundled credentials.
//! A file without a permitted URL or verifiable metadata is NEVER silently skipped.
//! This module only resolves download plans; it cannot mark a game instance READY.
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, env, io::Read};
use thiserror::Error;

const API_BASE: &str = "https://api.curseforge.com/v1";
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("CurseForge API key is not configured")]
    MissingApiKey,
    #[error("CurseForge API access denied")]
    PermissionDenied,
    #[error("CurseForge API network failure")]
    NetworkFailure,
    #[error("CurseForge API request failed with status {0}")]
    HttpStatus(u16),
    #[error("invalid CurseForge response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileAvailability {
    Available,
    NeedsManualDownload,
    PermissionDenied,
    NotFound,
    InvalidMetadata,
    NetworkFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurseForgeFile {
    pub project_id: u64,
    pub file_id: u64,
    pub required: bool,
    pub file_name: Option<String>,
    /// Only officially returned HTTPS URLs. Never synthesize forgecdn URLs.
    pub download_url: Option<String>,
    pub sha1: Option<String>,
    pub expected_size: Option<u64>,
    pub availability: FileAvailability,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeResolveReport {
    pub files: Vec<CurseForgeFile>,
    /// This stage only checks API resolution, not local hashes, loader or game readiness.
    pub resolved_count: usize,
    pub needs_action_count: usize,
    pub ready: bool,
}

#[derive(Debug, Deserialize)]
struct FileEnvelope {
    data: ApiFile,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiFile {
    id: u64,
    mod_id: u64,
    file_name: String,
    file_length: u64,
    download_url: Option<String>,
    #[serde(default)]
    hashes: Vec<ApiHash>,
}
#[derive(Debug, Deserialize)]
struct ApiHash {
    value: String,
    algo: u8,
}
#[derive(Debug, Deserialize)]
struct UrlEnvelope {
    data: Option<String>,
}

/// Fetches only the fixed CurseForge API origin. Implementations must not
/// follow redirects or send API credentials to other hosts.
pub trait CurseForgeApi {
    fn get(&self, path: &str, api_key: &str) -> Result<(u16, String), ResolveError>;
}

/// Basic blocking HTTPS adapter. Never logs or serializes the API key.
/// HTTP redirects are deliberately disabled to prevent leaking the key.
pub struct OfficialApiClient;
impl CurseForgeApi for OfficialApiClient {
    fn get(&self, path: &str, api_key: &str) -> Result<(u16, String), ResolveError> {
        if !path.starts_with("/mods/")
            || !path
                .bytes()
                .all(|c| c.is_ascii_digit() || c == b'/' || c.is_ascii_lowercase() || c == b'-')
        {
            return Err(ResolveError::InvalidResponse("invalid API path".into()));
        }
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(25))
            .build()
            .map_err(|_| ResolveError::NetworkFailure)?;
        let response = client
            .get(format!("{API_BASE}{path}"))
            .header("x-api-key", api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| ResolveError::NetworkFailure)?;
        let status = response.status().as_u16();
        // Bound the response as it is streamed; never allocate an unbounded
        // upstream error body or include it in errors/logging.
        let mut bytes = Vec::new();
        response
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ResolveError::NetworkFailure)?;
        if bytes.len() > 1024 * 1024 {
            return Err(ResolveError::InvalidResponse(
                "API response too large".into(),
            ));
        }
        let body = String::from_utf8(bytes)
            .map_err(|_| ResolveError::InvalidResponse("API response is not UTF-8".into()))?;
        Ok((status, body))
    }
}

pub fn configured_api_key() -> Result<String, ResolveError> {
    env::var("CURSEFORGE_API_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty() && s.len() < 1024)
        .ok_or(ResolveError::MissingApiKey)
}

pub fn safe_mod_file_name(name: &str) -> bool {
    name.len() <= 255
        && name.to_ascii_lowercase().ends_with(".jar")
        && super::safe_path(name).is_ok()
        && !name.contains('/')
}

pub fn approved_download_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https"
        || parsed.port().is_some()
        || parsed.username() != ""
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    // Fixed official CurseForge delivery hosts, not user-controlled subdomains.
    matches!(
        host.to_ascii_lowercase().as_str(),
        "edge.forgecdn.net" | "mediafilez.forgecdn.net" | "media.forgecdn.net"
    )
}

fn make_blocked(
    project_id: u64,
    file_id: u64,
    required: bool,
    availability: FileAvailability,
    reason: &str,
) -> CurseForgeFile {
    CurseForgeFile {
        project_id,
        file_id,
        required,
        file_name: None,
        download_url: None,
        sha1: None,
        expected_size: None,
        availability,
        reason: reason.into(),
    }
}

fn resolve_one(
    api: &impl CurseForgeApi,
    key: &str,
    project_id: u64,
    file_id: u64,
    required: bool,
) -> CurseForgeFile {
    let path = format!("/mods/{project_id}/files/{file_id}");
    let (status, body) = match api.get(&path, key) {
        Ok(reply) => reply,
        Err(_) => {
            return make_blocked(
                project_id,
                file_id,
                required,
                FileAvailability::NetworkFailure,
                "Core API request failed",
            )
        }
    };
    if status == 401 || status == 403 {
        return make_blocked(
            project_id,
            file_id,
            required,
            FileAvailability::PermissionDenied,
            "API authorization denied",
        );
    }
    if status == 404 {
        return make_blocked(
            project_id,
            file_id,
            required,
            FileAvailability::NotFound,
            "File not found in approved API",
        );
    }
    if status != 200 {
        return make_blocked(
            project_id,
            file_id,
            required,
            FileAvailability::NetworkFailure,
            "Core API returned unsuccessful response",
        );
    }
    let Ok(FileEnvelope { data }) = serde_json::from_str::<FileEnvelope>(&body) else {
        return make_blocked(
            project_id,
            file_id,
            required,
            FileAvailability::InvalidMetadata,
            "Malformed file metadata",
        );
    };
    if data.id != file_id
        || data.mod_id != project_id
        || !safe_mod_file_name(&data.file_name)
        || data.file_length == 0
        || data.file_length > MAX_FILE_BYTES
    {
        return make_blocked(
            project_id,
            file_id,
            required,
            FileAvailability::InvalidMetadata,
            "File identity, name or size rejected",
        );
    }
    let sha1 = data
        .hashes
        .into_iter()
        .find(|h| h.algo == 1)
        .map(|h| h.value.to_ascii_lowercase())
        .filter(|h| h.len() == 40 && h.bytes().all(|c| c.is_ascii_hexdigit()));
    let mut url = data.download_url.filter(|u| approved_download_url(u));
    if url.is_none() {
        let endpoint = format!("{path}/download-url");
        if let Ok((200, body)) = api.get(&endpoint, key) {
            if let Ok(UrlEnvelope { data: candidate }) = serde_json::from_str::<UrlEnvelope>(&body)
            {
                url = candidate.filter(|u| approved_download_url(u));
            }
        }
    }
    let availability = if url.is_some() && sha1.is_some() {
        FileAvailability::Available
    } else {
        FileAvailability::NeedsManualDownload
    };
    let reason = if url.is_none() {
        "No permitted download URL; authorized manual download required"
    } else if sha1.is_none() {
        "No trusted SHA-1 provided; authorized manual verification required"
    } else {
        "Approved URL and trusted SHA-1 resolved"
    };
    CurseForgeFile {
        project_id,
        file_id,
        required,
        file_name: Some(data.file_name),
        download_url: url,
        sha1,
        expected_size: Some(data.file_length),
        availability,
        reason: reason.into(),
    }
}

/// Resolve every declared CurseForge manifest file. Never mark READY from a
/// successful API response: installation, loader and JVM must still be verified.
pub fn resolve_curseforge_files(
    metadata: &super::PackMetadata,
    api: &impl CurseForgeApi,
    api_key: &str,
) -> Result<CurseForgeResolveReport, ResolveError> {
    if metadata.source != super::PackSource::CurseforgeZip {
        return Err(ResolveError::InvalidResponse(
            "not a CurseForge pack".into(),
        ));
    }
    if api_key.trim().is_empty() {
        return Err(ResolveError::MissingApiKey);
    }
    let mut unique = HashSet::new();
    let mut files = Vec::with_capacity(metadata.files.len());
    for item in &metadata.files {
        let (Some(project), Some(file)) = (item.project_id, item.file_id) else {
            return Err(ResolveError::InvalidResponse("missing file IDs".into()));
        };
        if !unique.insert((project, file)) {
            return Err(ResolveError::InvalidResponse("duplicate file IDs".into()));
        }
        files.push(resolve_one(api, api_key, project, file, item.required));
    }
    let resolved_count = files
        .iter()
        .filter(|f| f.availability == FileAvailability::Available)
        .count();
    let needs_action_count = files.len() - resolved_count;
    Ok(CurseForgeResolveReport {
        files,
        resolved_count,
        needs_action_count,
        ready: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct MockApi {
        file_status: u16,
        body: &'static str,
        fallback: &'static str,
    }
    impl CurseForgeApi for MockApi {
        fn get(&self, path: &str, _key: &str) -> Result<(u16, String), ResolveError> {
            if path.ends_with("download-url") {
                Ok((200, self.fallback.into()))
            } else {
                Ok((self.file_status, self.body.into()))
            }
        }
    }
    fn metadata() -> super::super::PackMetadata {
        super::super::PackMetadata {
            name: "fixture".into(),
            source: super::super::PackSource::CurseforgeZip,
            game_version: "1.16.5".into(),
            loader: super::super::Loader {
                kind: "forge".into(),
                version: Some("36.2.34".into()),
            },
            archive_sha256: "sample".into(),
            archive_entries: 2,
            uncompressed_bytes: 1024,
            files: vec![super::super::PlannedFile {
                path: "mods/placeholder.jar".into(),
                provider: "curseforge".into(),
                project_id: Some(100),
                file_id: Some(200),
                urls: vec![],
                sha1: None,
                sha512: None,
                required: true,
                side: "both".into(),
            }],
            overrides: vec![],
            warnings: vec![],
        }
    }
    #[test]
    fn resolved_url_does_not_mean_installed_or_ready() {
        let api = MockApi {
            file_status: 200,
            body: r#"{"data":{"id":200,"modId":100,"fileName":"actual.jar","fileLength":100,"downloadUrl":"https://edge.forgecdn.net/files/1/2/actual.jar","hashes":[{"value":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","algo":1}]}}"#,
            fallback: "",
        };
        let result = resolve_curseforge_files(&metadata(), &api, "test-key").unwrap();
        assert_eq!(result.resolved_count, 1);
        assert_eq!(
            result.files[0].sha1.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(!result.ready);
    }
    #[test]
    fn blocked_file_is_reported_not_dropped() {
        let api = MockApi {
            file_status: 200,
            body: r#"{"data":{"id":200,"modId":100,"fileName":"actual.jar","fileLength":100,"downloadUrl":null,"hashes":[]}}"#,
            fallback: r#"{"data":null}"#,
        };
        let result = resolve_curseforge_files(&metadata(), &api, "key").unwrap();
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.needs_action_count, 1);
        assert_eq!(
            result.files[0].availability,
            FileAvailability::NeedsManualDownload
        );
        assert!(!result.ready);
    }
    #[test]
    fn rejects_untrusted_delivery_hosts() {
        for url in [
            "http://edge.forgecdn.net/file",
            "https://127.0.0.1/file",
            "https://edge.forgecdn.net.evil.test/file",
            "https://user@edge.forgecdn.net/file",
            "https://edge.forgecdn.net:8443/file",
        ] {
            assert!(!approved_download_url(url), "{url}");
        }
    }
    #[test]
    fn id_mismatch_and_unsafe_name_fail_closed() {
        let api = MockApi {
            file_status: 200,
            body: r#"{"data":{"id":201,"modId":100,"fileName":"../evil.jar","fileLength":100,"downloadUrl":null,"hashes":[]}}"#,
            fallback: "",
        };
        let result = resolve_curseforge_files(&metadata(), &api, "key").unwrap();
        assert_eq!(
            result.files[0].availability,
            FileAvailability::InvalidMetadata
        );
    }
    #[test]
    fn missing_trusted_checksum_is_never_downloadable() {
        let api = MockApi {
            file_status: 200,
            body: r#"{"data":{"id":200,"modId":100,"fileName":"actual.jar","fileLength":100,"downloadUrl":"https://edge.forgecdn.net/files/1/2/actual.jar","hashes":[]}}"#,
            fallback: "",
        };
        let report = resolve_curseforge_files(&metadata(), &api, "fake-key").unwrap();
        assert_eq!(report.files[0].availability, FileAvailability::NeedsManualDownload);
        assert_eq!(report.needs_action_count, 1);
        assert!(!report.ready);
    }

    #[test]
    fn redirect_response_is_not_followed() {
        let api = MockApi { file_status: 302, body: "https://evil.test/", fallback: "" };
        let report = resolve_curseforge_files(&metadata(), &api, "fake-key").unwrap();
        assert_eq!(report.files[0].availability, FileAvailability::NetworkFailure);
        assert!(!report.ready);
    }

    #[test]
    fn permission_denied_is_explicit() {
        let api = MockApi {
            file_status: 403,
            body: "denied",
            fallback: "",
        };
        let result = resolve_curseforge_files(&metadata(), &api, "key").unwrap();
        assert_eq!(
            result.files[0].availability,
            FileAvailability::PermissionDenied
        );
    }
}
