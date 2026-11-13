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
    time::Duration,
};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MAX_JSON_BYTES: u64 = 32 * 1024 * 1024;

fn fetch_json(client: &Client, url: &str, sha: Option<&str>) -> Result<Value, GameError> {
    let u = reqwest::Url::parse(url).map_err(|e| GameError::InvalidInput(e.to_string()))?;
    if u.scheme() != "https"
        || u.port().is_some()
        || !matches!(
            u.host_str(),
            Some("piston-meta.mojang.com" | "launchermeta.mojang.com")
        )
    {
        return Err(GameError::InvalidInput(
            "untrusted metadata endpoint".into(),
        ));
    }
    let mut response = client
        .get(u)
        .send()
        .map_err(|e| GameError::NetworkFailure(e.to_string()))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(GameError::NetworkFailure(format!(
            "metadata HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_JSON_BYTES)
    {
        return Err(GameError::InvalidInput("metadata too large".into()));
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_JSON_BYTES {
        return Err(GameError::InvalidInput("metadata too large".into()));
    }
    if let Some(expected) = sha {
        if expected.len() != 40
            || !expected.bytes().all(|b| b.is_ascii_hexdigit())
            || format!("{:x}", Sha1::digest(&bytes)) != expected.to_ascii_lowercase()
        {
            return Err(GameError::IntegrityMismatch("version metadata SHA1".into()));
        }
    }
    Ok(serde_json::from_slice(&bytes)?)
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
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| GameError::NetworkFailure(e.to_string()))?;
    let manifest = fetch_json(&client, MANIFEST, None)?;
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
    let version = fetch_json(&client, url, Some(sha))?;
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

/// Validate archive file names before invoking the Windows system archive extractor.
/// Archives MUST already be hash-verified from official version metadata.
fn verify_native_listing(output: &[u8]) -> Result<(), GameError> {
    for line in String::from_utf8_lossy(output).lines() {
        let entry = line.trim_end_matches('\r');
        if entry.is_empty() {
            continue;
        }
        let s = entry.replace('\\', "/");
        if s.starts_with('/')
            || s.contains(':')
            || s.split('/').any(|part| part == ".." || part == ".")
        {
            return Err(GameError::InvalidInput("unsafe native archive path".into()));
        }
    }
    Ok(())
}

/// Extract verified Mojang native ZIP/JAR archives into a launcher-owned empty directory.
/// No user-provided archives. Does not follow archive symlinks and rejects unexpected files.
pub fn extract_natives(archives: &[PathBuf], destination: &Path) -> Result<(), GameError> {
    if !cfg!(windows) {
        return Err(GameError::UnsupportedVersion("Windows only".into()));
    }
    if destination.exists() {
        return Err(GameError::InvalidInput(
            "natives destination already exists".into(),
        ));
    }
    fs::create_dir_all(destination)?;
    let work = (|| -> Result<(), GameError> {
        for archive in archives {
            let listing = Command::new("tar.exe").arg("-tf").arg(archive).output()?;
            if !listing.status.success() {
                return Err(GameError::InvalidInput(
                    "could not inspect native archive".into(),
                ));
            }
            verify_native_listing(&listing.stdout)?;
            let status = Command::new("tar.exe")
                .arg("-xf")
                .arg(archive)
                .arg("-C")
                .arg(destination)
                .status()?;
            if !status.success() {
                return Err(GameError::InvalidInput("native extraction failed".into()));
            }
        }
        Ok(())
    })();
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

    #[test]
    fn native_archive_traversal_rejected() {
        assert!(verify_native_listing(b"safe.dll\nMETA-INF/\n").is_ok());
        assert!(verify_native_listing(b"../../outside.dll\n").is_err());
        assert!(verify_native_listing(b"C:\\Windows\\escape.dll\n").is_err());
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
