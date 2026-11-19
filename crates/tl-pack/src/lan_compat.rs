//! Opt-in, version-selected LAN configuration add-on. No account or session
//! settings are changed by the launcher. The released JAR is obtained from
//! Modrinth, verified against its metadata, then included in private staging.
use serde::Deserialize;
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::{
    io::{Cursor, Read},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const PROJECT: &str = "deYSo63V";
const VERSION: &str = "pQ1OCU76";
const FILENAME: &str = "lan-properties_forge-1.0.0+1.20-1.20.2.jar";
pub const RELATIVE_PATH: &str = "mods/lan-properties_forge-1.0.0+1.20-1.20.2.jar";
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_METADATA: u64 = 1024 * 1024;

pub fn supports(minecraft: &str, loader: &str, version: &str) -> bool {
    minecraft == "1.20.1" && loader == "forge" && version == "47.4.0"
}

pub fn possible_conflict(filename: &str) -> bool {
    let normalized: String = filename
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    [
        "lanproperties",
        "lanserverproperties",
        "allowofflinetojoinlan",
        "moreplayerslan",
        "offlinelan",
    ]
    .iter()
    .any(|name| normalized.contains(*name))
}

#[derive(Deserialize)]
struct Release {
    id: String,
    project_id: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<ReleaseFile>,
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    dependency_type: String,
}

#[derive(Deserialize)]
struct ReleaseFile {
    filename: String,
    url: String,
    size: u64,
    primary: bool,
    hashes: Hashes,
}

#[derive(Deserialize)]
struct Hashes {
    sha1: String,
    sha512: String,
}

fn select_file(release: &Release) -> Result<&ReleaseFile, String> {
    if release.id != VERSION
        || release.project_id != PROJECT
        || !release.game_versions.iter().any(|v| v == "1.20.1")
        || !release.loaders.iter().any(|v| v == "forge")
        || release
            .dependencies
            .iter()
            .any(|d| matches!(d.dependency_type.as_str(), "required" | "incompatible"))
    {
        return Err("LAN add-on release identity, compatibility or dependencies changed".into());
    }
    let mut files = release.files.iter().filter(|f| f.primary);
    let file = files.next().ok_or("LAN add-on has no primary file")?;
    if files.next().is_some()
        || file.filename != FILENAME
        || !crate::modrinth::approved_modrinth_url(&file.url)
        || !file
            .url
            .starts_with("https://cdn.modrinth.com/data/deYSo63V/versions/pQ1OCU76/")
        || file.size == 0
        || file.size > MAX_BYTES
        || file.hashes.sha1.len() != 40
        || file.hashes.sha512.len() != 128
        || !file.hashes.sha1.bytes().all(|b| b.is_ascii_hexdigit())
        || !file.hashes.sha512.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("LAN add-on file metadata is invalid".into());
    }
    Ok(file)
}

fn verify_bytes(file: &ReleaseFile, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != file.size
        || !format!("{:x}", Sha1::digest(bytes)).eq_ignore_ascii_case(&file.hashes.sha1)
        || !format!("{:x}", Sha512::digest(bytes)).eq_ignore_ascii_case(&file.hashes.sha512)
    {
        return Err("LAN add-on download size or checksum mismatch".into());
    }
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "LAN add-on is not a JAR")?;
    archive
        .by_name("META-INF/mods.toml")
        .map_err(|_| "LAN add-on lacks Forge metadata")?;
    Ok(())
}

fn bounded_read(input: impl Read, max: u64, cancelled: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut input = input.take(max + 1);
    let mut bytes = Vec::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err("LAN add-on download cancelled".into());
        }
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..n]);
        if bytes.len() as u64 > max {
            return Err("LAN add-on download exceeds size bound".into());
        }
    }
    Ok(bytes)
}

pub fn download(cancelled: &AtomicBool) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .user_agent("TLaunchCraft/0.1 (github.com/1doiliemkhoet/tlauchcraft)")
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get("https://api.modrinth.com/v2/version/pQ1OCU76")
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let metadata = bounded_read(response, MAX_METADATA, cancelled)?;
    let release: Release = serde_json::from_slice(&metadata).map_err(|e| e.to_string())?;
    let file = select_file(&release)?;
    let response = client
        .get(&file.url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let bytes = bounded_read(response, file.size, cancelled)?;
    verify_bytes(file, &bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_and_duplicate_detection() {
        assert!(supports("1.20.1", "forge", "47.4.0"));
        assert!(!supports("1.16.5", "forge", "36.2.29"));
        assert!(!supports("1.20.1", "fabric", "47.4.0"));
        assert!(possible_conflict(FILENAME));
        assert!(possible_conflict("LanServerProperties-1.11.jar"));
        assert!(!possible_conflict("jei-1.20.1.jar"));
    }

    #[test]
    fn bounded_download_fails_closed() {
        let token = AtomicBool::new(false);
        assert!(bounded_read(Cursor::new(b"1234"), 3, &token).is_err());
        token.store(true, Ordering::SeqCst);
        assert!(bounded_read(Cursor::new(b"123"), 3, &token).is_err());
    }

    #[test]
    fn wrong_origin_dependency_and_checksum_fail() {
        let mut release = Release {
            id: VERSION.into(),
            project_id: PROJECT.into(),
            game_versions: vec!["1.20.1".into()],
            loaders: vec!["forge".into()],
            files: vec![ReleaseFile {
                filename: FILENAME.into(),
                primary: true,
                size: 1,
                url: format!("https://cdn.modrinth.com/data/{PROJECT}/versions/{VERSION}/{FILENAME}"),
                hashes: Hashes {
                    sha1: "a".repeat(40),
                    sha512: "b".repeat(128),
                },
            }],
            dependencies: Vec::new(),
        };
        assert!(verify_bytes(select_file(&release).unwrap(), b"x").is_err());
        release.dependencies.push(Dependency {
            dependency_type: "required".into(),
        });
        assert!(select_file(&release).is_err());
        release.dependencies.clear();
        release.files[0].url = "https://example.org/mod.jar".into();
        assert!(select_file(&release).is_err());
    }
}
