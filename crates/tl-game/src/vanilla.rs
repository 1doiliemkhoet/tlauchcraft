//! Vanilla Windows install orchestration. No bundled game files or account tokens.
//! This module is intentionally explicit about unsupported versions and manual Java selection.
use crate::metadata::{plan_asset_objects, plan_version, FileRole, PlannedFile};
use crate::{
    expand_arguments, game_arguments, install_file, jvm_arguments, main_class, probe_java,
    required_java, DownloadSpec, Environment, GameError, LaunchConfig,
};
use reqwest::blocking::Client;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MAX_JSON_BYTES: u64 = 32 * 1024 * 1024;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

/// Each HTTP stage emits a duration and a fixed approved endpoint label only.
/// Request URLs (queries, credentials) and response bodies are never logged.
fn metadata_trace(stage: &str, endpoint: &str, event: &str, since: Instant) {
    eprintln!(
        "[vanilla-metadata] stage={stage} endpoint={endpoint} event={event} elapsed_ms={}",
        since.elapsed().as_millis()
    );
}

fn approved_metadata_endpoint(url: &str) -> Result<reqwest::Url, GameError> {
    let u = reqwest::Url::parse(url)
        .map_err(|e| GameError::InvalidInput(format!("invalid metadata URL: {e}")))?;
    if u.scheme() != "https"
        || u.port().is_some()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || !matches!(
            u.host_str(),
            Some("piston-meta.mojang.com" | "launchermeta.mojang.com")
        )
    {
        return Err(GameError::InvalidInput(
            "untrusted metadata endpoint".into(),
        ));
    }
    Ok(u)
}

fn metadata_client() -> Result<Client, GameError> {
    let start = Instant::now();
    eprintln!("[vanilla-metadata] stage=client event=start");
    let result = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| GameError::NetworkFailure(format!("metadata client initialization: {e}")));
    metadata_trace(
        "client",
        "local",
        if result.is_ok() { "ok" } else { "error" },
        start,
    );
    result
}

/// Transport is separately testable with a loopback HTTP server. Production
/// callers MUST validate HTTPS and the official host before invoking it.
fn fetch_json_transport(
    client: &Client,
    url: reqwest::Url,
    expected_sha: Option<&str>,
    stage: &str,
    endpoint: &str,
) -> Result<Value, GameError> {
    let begin = Instant::now();
    metadata_trace(stage, endpoint, "send_start", begin);
    // reqwest's timeout bounds connect, TLS negotiation, response headers AND
    // reading a stalled body. The connect timeout independently bounds TCP setup.
    let mut response = client.get(url).send().map_err(|e| {
        metadata_trace(stage, endpoint, "headers_error", begin);
        GameError::NetworkFailure(format!(
            "metadata stage={stage} endpoint={endpoint} phase=send_headers elapsed_ms={} timeout={} error={e}",
            begin.elapsed().as_millis(),
            e.is_timeout()
        ))
    })?;
    metadata_trace(stage, endpoint, "headers_ok", begin);
    if response.status() != reqwest::StatusCode::OK {
        return Err(GameError::NetworkFailure(format!(
            "metadata stage={stage} endpoint={endpoint} phase=headers HTTP {} elapsed_ms={}",
            response.status(),
            begin.elapsed().as_millis()
        )));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_JSON_BYTES)
    {
        return Err(GameError::InvalidInput(format!(
            "metadata stage={stage} endpoint={endpoint} body exceeds limit"
        )));
    }
    let body_start = Instant::now();
    metadata_trace(stage, endpoint, "body_start", begin);
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            metadata_trace(stage, endpoint, "body_error", begin);
            GameError::NetworkFailure(format!(
                "metadata stage={stage} endpoint={endpoint} phase=body elapsed_ms={} error={e}",
                begin.elapsed().as_millis()
            ))
        })?;
    metadata_trace(stage, endpoint, "body_ok", begin);
    eprintln!(
        "[vanilla-metadata] stage={stage} endpoint={endpoint} body_bytes={} body_ms={}",
        bytes.len(),
        body_start.elapsed().as_millis()
    );
    if bytes.len() as u64 > MAX_JSON_BYTES {
        return Err(GameError::InvalidInput(format!(
            "metadata stage={stage} endpoint={endpoint} body exceeds limit"
        )));
    }
    let verify_start = Instant::now();
    if let Some(expected) = expected_sha {
        if expected.len() != 40
            || !expected.bytes().all(|b| b.is_ascii_hexdigit())
            || format!("{:x}", Sha1::digest(&bytes)) != expected.to_ascii_lowercase()
        {
            metadata_trace(stage, endpoint, "sha1_mismatch", begin);
            return Err(GameError::IntegrityMismatch(format!(
                "metadata stage={stage} endpoint={endpoint} SHA1 mismatch elapsed_ms={}",
                begin.elapsed().as_millis()
            )));
        }
    }
    metadata_trace(stage, endpoint, "sha1_ok", begin);
    eprintln!(
        "[vanilla-metadata] stage={stage} endpoint={endpoint} sha1_ms={}",
        verify_start.elapsed().as_millis()
    );
    let parse_start = Instant::now();
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| {
        GameError::InvalidInput(format!(
            "metadata stage={stage} endpoint={endpoint} phase=json_parse elapsed_ms={} error={e}",
            begin.elapsed().as_millis()
        ))
    })?;
    metadata_trace(stage, endpoint, "json_ok", begin);
    eprintln!(
        "[vanilla-metadata] stage={stage} endpoint={endpoint} parse_ms={}",
        parse_start.elapsed().as_millis()
    );
    Ok(value)
}

fn fetch_json(
    client: &Client,
    url: &str,
    sha: Option<&str>,
    stage: &str,
    endpoint: &str,
) -> Result<Value, GameError> {
    let verified = approved_metadata_endpoint(url)?;
    fetch_json_transport(client, verified, sha, stage, endpoint)
}

pub fn resolve_official_version(version_id: &str) -> Result<Value, GameError> {
    if version_id.is_empty()
        || version_id.len() > 128
        || !version_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
    {
        return Err(GameError::InvalidInput("invalid version id".into()));
    }
    let client = metadata_client()?;
    let manifest = fetch_json(
        &client,
        MANIFEST,
        None,
        "manifest",
        "piston-meta.mojang.com",
    )?;
    let versions = manifest
        .get("versions")
        .and_then(Value::as_array)
        .ok_or_else(|| GameError::InvalidInput("missing manifest versions".into()))?;
    let entry = versions
        .iter()
        .find(|v| v.get("id").and_then(Value::as_str) == Some(version_id))
        .ok_or_else(|| GameError::UnsupportedVersion(version_id.into()))?;
    if entry.get("type").and_then(Value::as_str) != Some("release") {
        return Err(GameError::UnsupportedVersion(
            "only official Vanilla releases supported".into(),
        ));
    }
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| GameError::InvalidInput("version URL missing".into()))?;
    let sha = entry
        .get("sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| GameError::InvalidInput("version SHA1 missing".into()))?;
    let version = fetch_json(&client, url, Some(sha), "version", "piston-meta.mojang.com")?;
    if version.get("id").and_then(Value::as_str) != Some(version_id)
        || version.get("inheritsFrom").is_some()
    {
        return Err(GameError::UnsupportedVersion(
            "unexpected or inherited Vanilla version".into(),
        ));
    }
    Ok(version)
}

fn download_one(file: &PlannedFile, cancelled: &AtomicBool) -> Result<(), GameError> {
    install_file(
        &DownloadSpec {
            url: file.url.clone(),
            expected_sha1: file.sha1.clone(),
            expected_size: file.size,
            target: file.target.clone(),
        },
        cancelled,
    )
}

#[derive(Debug, Clone)]
pub struct VanillaInstall {
    pub version: Value,
    pub downloaded_files: usize,
    pub native_archives: Vec<PathBuf>,
}

/// Install verified files from official metadata; never distributes them in this repo.
/// A previously existing mismatched file is preserved and causes an explicit failure.
pub fn install_vanilla(
    version: &Value,
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<VanillaInstall, GameError> {
    if !cfg!(windows) {
        return Err(GameError::UnsupportedVersion(
            "Vanilla install supported on Windows only".into(),
        ));
    }
    let env = Environment::current();
    if env.os != "windows" {
        return Err(GameError::UnsupportedVersion("Windows required".into()));
    }
    let files = plan_version(version, root, &env)?;
    if !files.iter().any(|f| f.role == FileRole::ClientJar)
        || !files.iter().any(|f| f.role == FileRole::AssetIndex)
    {
        return Err(GameError::InvalidInput(
            "Vanilla client or asset index missing".into(),
        ));
    }
    let mut natives = Vec::new();
    for file in &files {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GameError::Cancelled);
        }
        download_one(file, cancelled)?;
        if file.role == FileRole::NativeArchive {
            natives.push(file.target.clone());
        }
    }
    let index = files
        .iter()
        .find(|f| f.role == FileRole::AssetIndex)
        .ok_or_else(|| GameError::InvalidInput("asset index missing".into()))?;
    let meta: Value = serde_json::from_slice(&fs::read(&index.target)?)?;
    let objects = plan_asset_objects(&meta, root)?;
    for obj in &objects {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GameError::Cancelled);
        }
        download_one(obj, cancelled)?;
    }
    Ok(VanillaInstall {
        version: version.clone(),
        downloaded_files: files.len() + objects.len(),
        native_archives: natives,
    })
}

/// Extract only Windows DLL natives from previously verified official Mojang archives.
/// The bundled PowerShell/.NET ZipArchive implementation checks every entry before writing,
/// rejects links/path traversal/duplicate DLL names and bounds uncompressed sizes.
/// Creates a new destination and never reuses or overwrites a previous extraction.
pub fn extract_natives(archives: &[PathBuf], destination: &Path) -> Result<(), GameError> {
    if !cfg!(windows) {
        return Err(GameError::UnsupportedVersion("Windows only".into()));
    }
    if archives.is_empty() {
        return Err(GameError::InvalidInput("native archives missing".into()));
    }
    if destination.exists() {
        return Err(GameError::InvalidInput(
            "natives destination already exists".into(),
        ));
    }
    for path in std::iter::once(destination).chain(archives.iter().map(PathBuf::as_path)) {
        if path.ancestors().any(Path::is_symlink) {
            return Err(GameError::InvalidInput(
                "symlinked native path rejected".into(),
            ));
        }
    }
    for archive in archives {
        if !archive.is_file() {
            return Err(GameError::InvalidInput("native archive is missing".into()));
        }
    }
    let parent = destination
        .parent()
        .ok_or_else(|| GameError::InvalidInput("native destination parent missing".into()))?;
    fs::create_dir_all(parent)?;
    // Keep this bundled script outside the empty destination, and never execute
    // scripts supplied by downloaded metadata.
    let script = parent.join(format!(".tl-native-extract-{}.ps1", std::process::id()));
    let mut temp = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&script)?;
    let write_result = temp.write_all(include_str!("native_extract.ps1").as_bytes());
    drop(temp);
    if let Err(err) = write_result {
        let _ = fs::remove_file(&script);
        return Err(GameError::Io(err));
    }
    let list = parent.join(format!(".tl-native-archives-{}.json", std::process::id()));
    let work = (|| -> Result<(), GameError> {
        let manifest = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&list)?;
        serde_json::to_writer(manifest, archives)?;
        fs::create_dir(destination)?;
        let result = Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("RemoteSigned")
            .arg("-File")
            .arg(&script)
            .arg("-Destination")
            .arg(destination)
            .arg("-ArchiveListPath")
            .arg(&list)
            .output()?;
        if !result.status.success() {
            let error = String::from_utf8_lossy(&result.stderr);
            return Err(GameError::InvalidInput(format!(
                "native ZIP extraction rejected: {}",
                error.trim()
            )));
        }
        Ok(())
    })();
    let _ = fs::remove_file(&list);
    let _ = fs::remove_file(&script);
    if work.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    work
}

#[derive(Debug, Clone)]
pub struct VanillaIdentity {
    /// The in-game UUID is separate from the persisted tl-core profile ID.
    pub game_uuid: String,
    pub username: String,
    /// Caller supplies an appropriate token from their authorized game session.
    pub access_token: String,
    pub user_type: String,
    /// Optional session claims used by current Mojang argument templates.
    pub client_id: String,
    pub xuid: String,
}

impl VanillaIdentity {
    /// Use the authoritative tl-core offline UUID contract; internal profile IDs are distinct.
    /// Username case matters, and renames may change server-side player data.
    pub fn from_offline_profile(profile: &tl_core::OfflineProfile) -> Result<Self, GameError> {
        let identity = tl_core::derive_vanilla_offline_identity(profile)
            .map_err(|e| GameError::InvalidInput(e.to_string()))?;
        Ok(Self {
            game_uuid: identity.effective_game_uuid.simple().to_string(),
            username: identity.username,
            access_token: "0".to_owned(),
            user_type: "legacy".to_owned(),
            client_id: String::new(),
            xuid: String::new(),
        })
    }

    /// CLI test helper. The product launcher should use from_offline_profile
    /// with its persisted SQLite profile rather than creating a new profile ID.
    pub fn from_offline_username(username: &str) -> Result<Self, GameError> {
        let uuid = tl_core::vanilla_offline_server_uuid(username)
            .map_err(|e| GameError::InvalidInput(e.to_string()))?;
        Ok(Self {
            game_uuid: uuid.simple().to_string(),
            username: username.to_owned(),
            access_token: "0".to_owned(),
            user_type: "legacy".to_owned(),
            client_id: String::new(),
            xuid: String::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct VanillaPaths {
    pub root: PathBuf,
    pub game_dir: PathBuf,
    pub natives_dir: PathBuf,
    pub java: PathBuf,
}

fn version_text<'a>(json: &'a Value, key: &str) -> Result<&'a str, GameError> {
    json.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| GameError::InvalidInput(format!("missing {key}")))
}

pub fn build_vanilla_launch(
    version: &Value,
    paths: &VanillaPaths,
    identity: &VanillaIdentity,
    min_ram_mb: u32,
    max_ram_mb: u32,
) -> Result<LaunchConfig, GameError> {
    let env = Environment::current();
    if env.os != "windows" {
        return Err(GameError::UnsupportedVersion("Windows only".into()));
    }
    let required = required_java(version)?;
    probe_java(&paths.java, required)?;
    let planned = plan_version(version, &paths.root, &env)?;
    let client = planned
        .iter()
        .find(|p| p.role == FileRole::ClientJar)
        .ok_or_else(|| GameError::InvalidInput("missing client jar".into()))?;
    let mut classpath: Vec<PathBuf> = planned
        .iter()
        .filter(|p| p.role == FileRole::Library)
        .map(|p| p.target.clone())
        .collect();
    classpath.push(client.target.clone());
    if classpath.iter().any(|path| !path.is_file()) {
        return Err(GameError::InvalidInput(
            "classpath missing installed file".into(),
        ));
    }
    if !paths.natives_dir.is_dir() {
        return Err(GameError::InvalidInput("natives directory missing".into()));
    }
    let asset_index = version
        .get("assetIndex")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| GameError::InvalidInput("asset index id missing".into()))?;
    if !identity
        .username
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        || !(3..=16).contains(&identity.username.len())
    {
        return Err(GameError::InvalidInput("invalid player username".into()));
    }
    if !(identity.game_uuid.len() == 32 || identity.game_uuid.len() == 36)
        || !identity
            .game_uuid
            .bytes()
            .all(|c| c.is_ascii_hexdigit() || c == b'-')
    {
        return Err(GameError::InvalidInput("invalid game UUID".into()));
    }
    let id = version_text(version, "id")?;
    let values = HashMap::from([
        ("auth_player_name".to_owned(), identity.username.clone()),
        ("auth_uuid".to_owned(), identity.game_uuid.clone()),
        (
            "auth_access_token".to_owned(),
            identity.access_token.clone(),
        ),
        ("user_type".to_owned(), identity.user_type.clone()),
        ("clientid".to_owned(), identity.client_id.clone()),
        ("auth_xuid".to_owned(), identity.xuid.clone()),
        ("version_name".to_owned(), id.to_owned()),
        ("version_type".to_owned(), "release".to_owned()),
        (
            "game_directory".to_owned(),
            paths.game_dir.to_string_lossy().into_owned(),
        ),
        (
            "assets_root".to_owned(),
            paths.root.join("assets").to_string_lossy().into_owned(),
        ),
        ("assets_index_name".to_owned(), asset_index.to_owned()),
        (
            "natives_directory".to_owned(),
            paths.natives_dir.to_string_lossy().into_owned(),
        ),
        ("launcher_name".to_owned(), "TLaunchCraft".to_owned()),
        (
            "launcher_version".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        ),
        ("resolution_width".to_owned(), "854".to_owned()),
        ("resolution_height".to_owned(), "480".to_owned()),
    ]);
    let mut game_args = expand_arguments(&game_arguments(version, &env)?, &values)?;
    // Never leak access token into installer metadata or diagnostic report.
    if game_args.iter().any(|arg| arg.contains("${")) {
        return Err(GameError::InvalidInput("unresolved game arguments".into()));
    }
    let mut jvm_args = jvm_arguments(version, &env)?;
    // Core command sets classpath and native search paths; strip Mojang's duplicates.
    let mut filtered = Vec::new();
    let mut skip = false;
    for argument in jvm_args.drain(..) {
        if skip {
            skip = false;
            continue;
        }
        if argument == "-cp" || argument == "-classpath" {
            skip = true;
            continue;
        }
        if argument.starts_with("-Djava.library.path=") {
            continue;
        }
        filtered.push(argument);
    }
    let classpath_value = std::env::join_paths(&classpath)
        .map_err(|e| GameError::InvalidInput(e.to_string()))?
        .to_string_lossy()
        .into_owned();
    let mut jvm_values = values;
    jvm_values.insert("classpath".into(), classpath_value);
    jvm_values.insert("classpath_separator".into(), ";".into());
    jvm_args = expand_arguments(&filtered, &jvm_values)?;
    if let Some(argument) = version
        .get("logging")
        .and_then(|v| v.get("client"))
        .and_then(|v| v.get("argument"))
        .and_then(Value::as_str)
    {
        let log_config = planned
            .iter()
            .find(|file| file.role == FileRole::LoggingConfig)
            .ok_or_else(|| GameError::InvalidInput("logging config missing".into()))?;
        if !log_config.target.is_file() {
            return Err(GameError::InvalidInput(
                "logging config not installed".into(),
            ));
        }
        let expanded = argument.replace("${path}", &log_config.target.to_string_lossy());
        if expanded.contains("${") {
            return Err(GameError::InvalidInput(
                "unresolved logging argument".into(),
            ));
        }
        jvm_args.push(expanded);
    }
    game_args.shrink_to_fit();
    Ok(LaunchConfig {
        java: paths.java.clone(),
        classpath,
        natives_dir: paths.natives_dir.clone(),
        game_dir: paths.game_dir.clone(),
        main_class: main_class(version)?.to_owned(),
        jvm_args,
        game_args,
        min_ram_mb,
        max_ram_mb,
    })
}

/// Store reproducible non-secret evidence without asserting menu/world success.
pub fn write_launch_evidence(
    path: &Path,
    version: &str,
    java: &Path,
    pid: u32,
    stdout: &Path,
    stderr: &Path,
) -> Result<(), GameError> {
    let report = serde_json::json!({
        "version": version,
        "pid": pid,
        "java_path": java.to_string_lossy(),
        "stdout_path": stdout.to_string_lossy(),
        "stderr_path": stderr.to_string_lossy(),
        "menu_verified": false,
        "world_reopened_verified": false
    });
    let parent = path
        .parent()
        .ok_or_else(|| GameError::InvalidInput("evidence path".into()))?;
    fs::create_dir_all(parent)?;
    let mut out = fs::File::create_new(path)?;
    out.write_all(serde_json::to_string_pretty(&report)?.as_bytes())?;
    out.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_client() -> Client {
        Client::builder()
            .connect_timeout(Duration::from_millis(120))
            .timeout(Duration::from_millis(250))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    fn local_http<F>(handler: F) -> (reqwest::Url, std::thread::JoinHandle<()>)
    where
        F: FnOnce(std::net::TcpStream) + Send + 'static,
    {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handler(stream);
        });
        (format!("http://{addr}/metadata").parse().unwrap(), handle)
    }

    #[test]
    fn metadata_headers_stall_is_bounded() {
        let (url, server) = local_http(|_stream| {
            std::thread::sleep(Duration::from_millis(600));
        });
        let start = Instant::now();
        let error = fetch_json_transport(&test_client(), url, None, "test", "loopback")
            .unwrap_err()
            .to_string();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(error.contains("send_headers"), "{error}");
        assert!(error.contains("endpoint=loopback"), "{error}");
        server.join().unwrap();
    }

    #[test]
    fn metadata_body_stall_is_bounded() {
        let (url, server) = local_http(|mut stream| {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 200\r\nConnection: close\r\n\r\n{\"id\":",
                )
                .unwrap();
            stream.flush().unwrap();
            std::thread::sleep(Duration::from_millis(600));
        });
        let start = Instant::now();
        let error = fetch_json_transport(&test_client(), url, None, "test", "loopback")
            .unwrap_err()
            .to_string();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(error.contains("phase=body"), "{error}");
        server.join().unwrap();
    }

    #[test]
    fn metadata_connection_refused_fails_with_stage() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url: reqwest::Url = format!("http://{}/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        drop(listener);
        let start = Instant::now();
        let error = fetch_json_transport(&test_client(), url, None, "test", "loopback")
            .unwrap_err()
            .to_string();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(error.contains("phase=send_headers"), "{error}");
    }

    #[test]
    fn metadata_checksum_mismatch_has_precise_error() {
        let (url, server) = local_http(|mut stream| {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                )
                .unwrap();
        });
        let error = fetch_json_transport(
            &test_client(),
            url,
            Some("0000000000000000000000000000000000000000"),
            "version",
            "loopback",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("SHA1 mismatch"), "{error}");
        server.join().unwrap();
    }

    #[test]
    fn metadata_endpoint_is_strictly_allowlisted() {
        for url in [
            "http://piston-meta.mojang.com/manifest",
            "https://example.com/metadata",
            "https://piston-meta.mojang.com:8443/a",
            "https://user:pass@piston-meta.mojang.com/a",
            "https://piston-meta.mojang.com/a?secret=x",
            "https://piston-meta.mojang.com/a#secret",
            "file:///C:/something",
        ] {
            assert!(approved_metadata_endpoint(url).is_err(), "{url}");
        }
        assert!(approved_metadata_endpoint(MANIFEST).is_ok());
    }

    #[test]
    fn offline_uuid_uses_tl_core_contract_not_profile_id() {
        let profile = tl_core::OfflineProfile::new("Notch").unwrap();
        let player = VanillaIdentity::from_offline_profile(&profile).unwrap();
        assert_eq!(player.username, "Notch");
        assert_eq!(player.game_uuid, "b50ad385829d3141a2167e7d7539ba7f");
        assert_ne!(player.game_uuid, profile.id.simple().to_string());
        assert_eq!(
            VanillaIdentity::from_offline_username("Notch")
                .unwrap()
                .game_uuid,
            player.game_uuid
        );
    }

    #[test]
    fn offline_uuid_rename_and_case_have_explicit_impact() {
        let first = VanillaIdentity::from_offline_username("PlayerOne").unwrap();
        let second = VanillaIdentity::from_offline_username("PlayerTwo").unwrap();
        assert_ne!(first.game_uuid, second.game_uuid);
        assert_ne!(
            VanillaIdentity::from_offline_username("CaseName")
                .unwrap()
                .game_uuid,
            VanillaIdentity::from_offline_username("casename")
                .unwrap()
                .game_uuid
        );
        assert!(VanillaIdentity::from_offline_username("bad name").is_err());
    }

    #[test]
    fn native_extractor_has_strict_zip_gate() {
        let code = include_str!("native_extract.ps1");
        assert!(code.contains("ZipFile]::OpenRead"));
        assert!(code.contains("FileMode]::CreateNew"));
        assert!(code.contains("Duplicate DLL name"));
        assert!(code.contains("Native extraction limit exceeded"));
    }

    #[cfg(windows)]
    #[test]
    fn native_extractor_rejects_missing_archive() {
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("natives");
        assert!(extract_natives(&[d.path().join("missing.jar")], &out).is_err());
        assert!(!out.exists());
    }

    #[cfg(windows)]
    fn create_native_zip(path: &Path, entry_name: &str) {
        let script = r#"
            $ErrorActionPreference = 'Stop'
            Add-Type -AssemblyName System.IO.Compression
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            $z = [System.IO.Compression.ZipFile]::Open(
                $env:TLC_TEST_ZIP,
                [System.IO.Compression.ZipArchiveMode]::Create)
            try {
                $entry = $z.CreateEntry($env:TLC_TEST_ENTRY)
                $writer = [System.IO.StreamWriter]::new($entry.Open())
                try { $writer.Write('test-dll') } finally { $writer.Dispose() }
            } finally { $z.Dispose() }
        "#;
        let result = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("TLC_TEST_ZIP", path)
            .env("TLC_TEST_ENTRY", entry_name)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "test ZIP setup failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[cfg(windows)]
    #[test]
    fn real_native_zip_dll_is_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("safe.jar");
        create_native_zip(&archive, "org/lwjgl/native-test.dll");
        let dest = dir.path().join("natives");
        extract_natives(&[archive], &dest).unwrap();
        assert_eq!(fs::read(dest.join("native-test.dll")).unwrap(), b"test-dll");
    }

    #[cfg(windows)]
    #[test]
    fn malicious_native_zip_traversal_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("traversal.jar");
        create_native_zip(&archive, "../../outside.dll");
        let dest = dir.path().join("natives");
        assert!(extract_natives(&[archive], &dest).is_err());
        assert!(!dest.exists());
        assert!(!dir.path().join("outside.dll").exists());
    }

    #[cfg(windows)]
    #[test]
    fn duplicate_dll_across_archives_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("a.jar");
        let second = dir.path().join("b.jar");
        create_native_zip(&first, "windows/native-test.dll");
        create_native_zip(&second, "another/native-test.dll");
        let dest = dir.path().join("natives");
        assert!(extract_natives(&[first, second], &dest).is_err());
        assert!(!dest.exists());
    }

    #[test]
    fn wrong_platform_cannot_claim_install() {
        let flag = AtomicBool::new(false);
        if !cfg!(windows) {
            assert!(install_vanilla(&json!({}), Path::new("x"), &flag).is_err());
        }
    }

    #[test]
    fn no_java_or_files_does_not_build_launch() {
        let paths = VanillaPaths {
            root: "C:/nonexistent-game".into(),
            game_dir: "C:/nonexistent-game/instance".into(),
            natives_dir: "C:/nonexistent-game/natives".into(),
            java: "C:/nonexistent-java/bin/java.exe".into(),
        };
        let identity = VanillaIdentity {
            game_uuid: "00000000000000000000000000000000".into(),
            username: "Player_01".into(),
            access_token: String::new(),
            user_type: "legacy".into(),
            client_id: String::new(),
            xuid: String::new(),
        };
        assert!(build_vanilla_launch(
            &json!({"id":"1.21.1","javaVersion":{"majorVersion":21}}),
            &paths,
            &identity,
            512,
            2048
        )
        .is_err());
    }
}
