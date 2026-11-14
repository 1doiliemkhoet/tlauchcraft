//! Independent, non-secret disk evidence for manual Windows Vanilla acceptance.
//! Hashes and snapshots are evidence, NOT assertions that a game menu/world was displayed.
use crate::{
    metadata::{plan_asset_objects, plan_version, PlannedFile},
    Environment, GameError,
};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

fn file_sha1(path: &Path) -> Result<(String, u64), GameError> {
    if path.is_symlink() || !path.is_file() {
        return Err(GameError::InvalidInput(format!(
            "expected regular evidence file: {}",
            path.display()
        )));
    }
    let mut file = File::open(path)?;
    let mut sha = Sha1::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        sha.update(&buffer[..n]);
        size += n as u64;
    }
    Ok((format!("{:x}", sha.finalize()), size))
}

fn attest(file: &PlannedFile) -> Result<Value, GameError> {
    let (actual, size) = file_sha1(&file.target)?;
    if actual != file.sha1.to_ascii_lowercase()
        || file.size.is_some_and(|expected| expected != size)
    {
        return Err(GameError::IntegrityMismatch(format!(
            "installed file mismatch: {}",
            file.target.display()
        )));
    }
    Ok(json!({
        "role": format!("{:?}", file.role),
        "path": file.target.to_string_lossy(),
        "sha1": actual,
        "size": size
    }))
}

fn save_new_json(path: &Path, value: &Value) -> Result<(), GameError> {
    let parent = path
        .parent()
        .ok_or_else(|| GameError::InvalidInput("evidence path missing parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut out = File::create_new(path)?;
    out.write_all(serde_json::to_string_pretty(value)?.as_bytes())?;
    out.sync_all()?;
    Ok(())
}

/// Rehash every official planned client/library/native/asset file from disk after install.
/// A complete manifest confirms G2 *file integrity*, not gameplay correctness.
pub fn write_install_checksums(
    version: &Value,
    root: &Path,
    evidence_path: &Path,
) -> Result<usize, GameError> {
    let planned = plan_version(version, root, &Environment::current())?;
    let index = planned
        .iter()
        .find(|item| item.role == crate::metadata::FileRole::AssetIndex)
        .ok_or_else(|| GameError::InvalidInput("missing asset index".into()))?;
    let asset_index: Value = serde_json::from_slice(&fs::read(&index.target)?)?;
    let mut files = planned;
    files.extend(plan_asset_objects(&asset_index, root)?);
    let manifest: Vec<Value> = files.iter().map(attest).collect::<Result<_, _>>()?;
    let count = manifest.len();
    save_new_json(
        evidence_path,
        &json!({
            "version": version.get("id"),
            "verified_file_count": count,
            "verified_file_sha1": manifest,
            "game_menu_verified": false,
            "world_reopened_verified": false
        }),
    )?;
    Ok(count)
}

/// Capture world save metadata after a game process has exited. A snapshot alone does not
/// establish that the user opened or played the world; screenshots/manual checks remain required.
pub fn write_world_snapshot(game_dir: &Path, evidence_path: &Path) -> Result<usize, GameError> {
    let saves = game_dir.join("saves");
    let mut worlds = Vec::new();
    if saves.is_dir() {
        for entry in fs::read_dir(&saves)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() || !entry.file_type()?.is_dir() {
                continue;
            }
            let world = entry.path();
            let level = world.join("level.dat");
            if level.is_file() && !level.is_symlink() {
                let (hash, size) = file_sha1(&level)?;
                worlds.push(json!({
                    "world_directory": entry.file_name().to_string_lossy(),
                    "level_dat_sha1": hash,
                    "level_dat_bytes": size
                }));
            }
        }
    }
    worlds.sort_by_key(|v| v["world_directory"].as_str().unwrap_or("").to_owned());
    let count = worlds.len();
    save_new_json(
        evidence_path,
        &json!({
            "worlds_found": count,
            "worlds": worlds,
            "world_reopened_verified": false
        }),
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::FileRole;
    use tempfile::tempdir;

    #[test]
    fn checksum_evidence_rejects_mismatch() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.jar");
        fs::write(&file, b"good").unwrap();
        let expected = format!("{:x}", Sha1::digest(b"good"));
        let plan = PlannedFile {
            role: FileRole::Library,
            url: String::new(),
            sha1: expected,
            size: Some(4),
            target: file.clone(),
        };
        assert!(attest(&plan).is_ok());
        fs::write(&file, b"corrupt").unwrap();
        assert!(attest(&plan).is_err());
    }

    #[test]
    fn snapshot_is_not_world_reopen_proof() {
        let dir = tempdir().unwrap();
        let world = dir.path().join("saves").join("ExampleWorld");
        fs::create_dir_all(&world).unwrap();
        fs::write(world.join("level.dat"), b"snapshot").unwrap();
        let evidence = dir.path().join("world-evidence.json");
        assert_eq!(write_world_snapshot(dir.path(), &evidence).unwrap(), 1);
        let recorded: Value = serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap();
        assert_eq!(recorded["worlds_found"], 1);
        assert_eq!(recorded["world_reopened_verified"], false);
    }
}
