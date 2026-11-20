//! Pinned CustomSkinLoader release candidates. Download and integrity check only.
//! Does not install a mod or assert runtime compatibility.
use serde::Deserialize;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::{
    io::{Cursor, Read},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const PROJECT: &str = "idMHQ4n2";
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_METADATA: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub minecraft: &'static str,
    pub version_id: &'static str,
    pub filename: &'static str,
    pub sha256: &'static str,
}

pub fn candidate(minecraft: &str, loader: &str) -> Option<Candidate> {
    if loader != "forge" {
        return None;
    }
    match minecraft {
        "1.16.5" => Some(Candidate {
            minecraft: "1.16.5",
            version_id: "wLkp5dpJ",
            filename: "CustomSkinLoader_ForgeV1-14.26.1.jar",
            sha256: "9ff705d44c6543a3e7861841a1b081c3b7f4a148ba575ba7d97066c507eddf45",
        }),
        "1.20.1" => Some(Candidate {
            minecraft: "1.20.1",
            version_id: "CcAsu9W3",
            filename: "CustomSkinLoader_ForgeV2-14.26.1.jar",
            sha256: "b3484b9254e1f409d1b1402d3ad7ac751833e22d9dc069017bbaa405760b77fc",
        }),
        _ => None,
    }
}

impl Candidate {
    pub fn relative_path(self) -> String {
        format!("mods/{}", self.filename)
    }
}

pub fn pinned_override(path: &str, sha256: &str) -> bool {
    ["1.16.5", "1.20.1"].iter().any(|version| {
        candidate(version, "forge").is_some_and(|spec| {
            path == spec.relative_path() && sha256 == spec.sha256
        })
    })
}

pub fn possible_conflict(filename: &str) -> bool {
    let name: String = filename
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    ["customskinloader", "tlskin","skinloader","offline skins","offlineskins"]
        .iter()
        .any(|part| name.contains(part))
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

fn select_file(release: &Release, spec: Candidate) -> Result<&ReleaseFile, String> {
    if release.id != spec.version_id
        || release.project_id != PROJECT
        || !release.game_versions.iter().any(|v| v == spec.minecraft)
        || !release.loaders.iter().any(|v| v == "forge")
        || release
            .dependencies
            .iter()
            .any(|d| matches!(d.dependency_type.as_str(), "required" | "incompatible"))
    {
        return Err("skin add-on release identity, compatibility or dependencies changed".into());
    }
    let mut files = release.files.iter().filter(|f| f.primary);
    let file = files.next().ok_or("skin add-on has no primary file")?;
    if files.next().is_some()
        || file.filename != spec.filename
        || !crate::modrinth::approved_modrinth_url(&file.url)
        || file.url != format!(
            "https://cdn.modrinth.com/data/{PROJECT}/versions/{}/{}",
            spec.version_id, spec.filename
        )
        || file.size == 0
        || file.size > MAX_BYTES
        || file.hashes.sha1.len() != 40
        || file.hashes.sha512.len() != 128
        || !file.hashes.sha1.bytes().all(|b| b.is_ascii_hexdigit())
        || !file.hashes.sha512.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("skin add-on file metadata is invalid".into());
    }
    Ok(file)
}

fn verify_bytes(file: &ReleaseFile, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != file.size
        || !format!("{:x}", Sha1::digest(bytes)).eq_ignore_ascii_case(&file.hashes.sha1)
        || !format!("{:x}", Sha512::digest(bytes)).eq_ignore_ascii_case(&file.hashes.sha512)
    {
        return Err("skin add-on download size or checksum mismatch".into());
    }
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "skin add-on is not a JAR")?;
    archive
        .by_name("META-INF/MANIFEST.MF")
        .map_err(|_| "skin add-on lacks JAR manifest")?;
    Ok(())
}

fn bounded_read(input: impl Read, max: u64, cancelled: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut input = input.take(max + 1);
    let mut bytes = Vec::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err("skin add-on download cancelled".into());
        }
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..n]);
        if bytes.len() as u64 > max {
            return Err("skin add-on download exceeds size bound".into());
        }
    }
    Ok(bytes)
}

pub fn download(minecraft: &str, loader: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, String> {
    let spec = candidate(minecraft, loader).ok_or("unsupported skin provider candidate")?;
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .user_agent("TLaunchCraft/0.1 (github.com/1doiliemkhoet/tlauchcraft)")
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(format!(
            "https://api.modrinth.com/v2/version/{}",
            spec.version_id
        ))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let metadata = bounded_read(response, MAX_METADATA, cancelled)?;
    let release: Release = serde_json::from_slice(&metadata).map_err(|e| e.to_string())?;
    let file = select_file(&release, spec)?;
    let response = client
        .get(&file.url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let bytes = bounded_read(response, file.size, cancelled)?;
    verify_bytes(file, &bytes)?;
    if format!("{:x}", Sha256::digest(&bytes)) != spec.sha256 {
        return Err("skin add-on differs from audited binary".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_only_reviewed_candidate_versions() {
        assert_eq!(
            candidate("1.16.5", "forge").unwrap().version_id,
            "wLkp5dpJ"
        );
        assert_eq!(
            candidate("1.20.1", "forge").unwrap().version_id,
            "CcAsu9W3"
        );
        assert!(candidate("1.21.1", "forge").is_none());
        assert!(candidate("1.20.1", "vanilla").is_none());
    }

    #[test]
    fn rejects_bad_metadata_and_bytes() {
        let spec = candidate("1.20.1", "forge").unwrap();
        let mut release = Release {
            id: spec.version_id.into(),
            project_id: PROJECT.into(),
            game_versions: vec![spec.minecraft.into()],
            loaders: vec!["forge".into()],
            files: vec![ReleaseFile {
                filename: spec.filename.into(),
                primary: true,
                size: 1,
                url: format!(
                    "https://cdn.modrinth.com/data/{PROJECT}/versions/{}/{}",
                    spec.version_id, spec.filename
                ),
                hashes: Hashes {
                    sha1: "a".repeat(40),
                    sha512: "b".repeat(128),
                },
            }],
            dependencies: Vec::new(),
        };
        assert!(verify_bytes(select_file(&release, spec).unwrap(), b"x").is_err());
        release.dependencies.push(Dependency {
            dependency_type: "required".into(),
        });
        assert!(select_file(&release, spec).is_err());
        release.dependencies.clear();
        release.files[0].url = "https://example.org/mod.jar".into();
        assert!(select_file(&release, spec).is_err());
    }

    #[test]
    fn bounded_read_honors_cancel_and_size() {
        let token = AtomicBool::new(false);
        assert!(bounded_read(Cursor::new(b"1234"), 3, &token).is_err());
        token.store(true, Ordering::SeqCst);
        assert!(bounded_read(Cursor::new(b"123"), 3, &token).is_err());
    }
}
