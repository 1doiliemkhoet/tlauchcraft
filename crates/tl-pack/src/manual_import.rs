//! User-authorized manual import of a restricted CurseForge mod.
//! A local file only satisfies a specific official project/fileID pair when
//! the authoritative CurseForge API supplied both size and SHA-1. No mirror,
//! guessed URL, or unverified binary can be accepted.
use crate::{
    curseforge::{CurseForgeFile, FileAvailability},
    install::InstalledMod,
    safe_path,
};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const LIMIT: u64 = 1024 * 1024 * 1024;

/// Validate the user's local mod before creating any destination files.
/// The directory must be a private, pre-existing staging root, never a live
/// game directory. A correct hash does not imply Forge or pack READY.
pub fn import_restricted_mod(
    local_file: &Path,
    stage: &Path,
    official: &CurseForgeFile,
    cancel: &AtomicBool,
) -> Result<InstalledMod, String> {
    if official.availability == FileAvailability::Available {
        return Err("file is already approved for normal CDN download".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if !local_file.is_absolute() || !stage.is_absolute() {
        return Err("import paths must be absolute".into());
    }
    if !stage.is_dir()
        || stage
            .file_name()
            .and_then(|x| x.to_str())
            .is_none_or(|x| !x.ends_with(".staging"))
    {
        return Err("manual import requires an existing private .staging directory".into());
    }
    for dir in [local_file, stage] {
        for ancestor in dir.ancestors() {
            if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("symlinked input or staging root is forbidden".into());
            }
        }
    }
    let info = fs::symlink_metadata(local_file).map_err(|_| "manual import source missing")?;
    if !info.file_type().is_file() {
        return Err("manual import source must be a regular file".into());
    }
    let size = official
        .expected_size
        .ok_or("provider did not publish authoritative size")?;
    let sha1 = official
        .sha1
        .as_deref()
        .ok_or("provider did not publish SHA-1; needs action")?;
    if size == 0
        || size > LIMIT
        || size != info.len()
        || sha1.len() != 40
        || !sha1.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("manual import size or trusted SHA-1 invalid".into());
    }
    let name = official
        .file_name
        .as_deref()
        .ok_or("provider did not publish file name")?;
    if !crate::curseforge::safe_mod_file_name(name) {
        return Err("unsafe provider file name".into());
    }
    let relative = format!("mods/{name}");
    let relative_path = safe_path(&relative).map_err(|_| "unsafe provider mod path")?;
    let mods = stage.join("mods");
    if fs::symlink_metadata(&mods).is_ok_and(|m| !m.file_type().is_dir()) {
        return Err("staging mods directory is not a regular directory".into());
    }
    fs::create_dir_all(&mods).map_err(|_| "cannot create isolated mods directory")?;
    let target = stage.join(relative_path);
    if fs::symlink_metadata(&target).is_ok() {
        return Err("manual mod target already exists; never overwrite".into());
    }
    let partial = stage.join(format!("mods/{name}.tlimport"));
    let mut source = File::open(local_file).map_err(|_| "cannot read local mod")?;
    let mut h1 = Sha1::new();
    let mut h256 = Sha256::new();
    // create_new is atomic and rejects concurrent import collisions.
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&partial)
        .map_err(|_| "manual import partial already exists")?;
    let transfer = (|| -> Result<(), String> {
        let mut bytes = 0u64;
        let mut buf = [0u8; 64 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let n = source
                .read(&mut buf)
                .map_err(|_| "manual source read failed")?;
            if n == 0 {
                break;
            }
            bytes = bytes
                .checked_add(n as u64)
                .ok_or("manual source size overflow")?;
            if bytes > size {
                return Err("manual file exceeds official size".into());
            }
            output
                .write_all(&buf[..n])
                .map_err(|_| "manual stage write failed")?;
            h1.update(&buf[..n]);
            h256.update(&buf[..n]);
        }
        if bytes != size || format!("{:x}", h1.finalize()) != sha1.to_ascii_lowercase() {
            return Err("manual import checksum mismatch; cannot satisfy required mod".into());
        }
        output
            .sync_all()
            .map_err(|_| "manual stage sync failed".into())?;
        Ok(())
    })();
    drop(output);
    if let Err(error) = transfer {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    // A link swap/target collision must never overwrite another mod.
    if fs::symlink_metadata(&target).is_ok() {
        let _ = fs::remove_file(&partial);
        return Err("manual mod target appeared during import".into());
    }
    // Hard links (not rename) provide a no-clobber publication into staging.
    // The private temp remains until this succeeds; never overwrite on Windows.
    if fs::hard_link(&partial, &target).is_err() {
        let _ = fs::remove_file(&partial);
        return Err("manual import atomic no-clobber publish failed".into());
    }
    let _ = fs::remove_file(&partial);
    Ok(InstalledMod {
        project_id: official.project_id,
        file_id: official.file_id,
        relative_path: relative,
        actual_sha256: format!("{:x}", h256.finalize()),
        bytes: size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curseforge::CurseForgeFile;
    fn fixture(bytes: &[u8]) -> CurseForgeFile {
        CurseForgeFile {
            project_id: 100,
            file_id: 200,
            required: true,
            file_name: Some("restricted.jar".into()),
            download_url: None,
            sha1: Some(format!("{:x}", Sha1::digest(bytes))),
            expected_size: Some(bytes.len() as u64),
            availability: FileAvailability::NeedsManualDownload,
            reason: "download restricted".into(),
            dependencies: Vec::new(),
        }
    }
    #[test]
    fn verified_manual_file_is_imported_without_claiming_instance_ready() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().canonicalize().unwrap();
        let stage = parent.join("user-pack.staging");
        fs::create_dir(&stage).unwrap();
        let src = parent.join("user-selected.jar");
        fs::write(&src, b"authorized source").unwrap();
        let result = import_restricted_mod(
            &src,
            &stage,
            &fixture(b"authorized source"),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.bytes, 17);
        assert_eq!(
            fs::read(stage.join("mods/restricted.jar")).unwrap(),
            b"authorized source"
        );
        assert!(import_restricted_mod(
            &src,
            &stage,
            &fixture(b"authorized source"),
            &AtomicBool::new(false)
        )
        .is_err());
    }
    #[test]
    fn invalid_checksum_and_cancel_never_create_installed_mod() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let stage = root.join("failed-pack.staging");
        fs::create_dir(&stage).unwrap();
        let source = root.join("source.jar");
        fs::write(&source, b"altered bytes").unwrap();
        assert!(import_restricted_mod(
            &source,
            &stage,
            &fixture(b"expected bytes"),
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(!stage.join("mods/restricted.jar").exists());
        assert!(import_restricted_mod(
            &source,
            &stage,
            &fixture(b"altered bytes"),
            &AtomicBool::new(true)
        )
        .is_err());
        assert!(!stage.join("mods/restricted.jar").exists());
    }
    #[test]
    fn no_provider_hash_never_passes_even_when_local_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let stage = root.join("test-pack.staging");
        fs::create_dir(&stage).unwrap();
        let source = root.join("source.jar");
        fs::write(&source, b"data").unwrap();
        let mut official = fixture(b"data");
        official.sha1 = None;
        assert!(
            import_restricted_mod(&source, &stage, &official, &AtomicBool::new(false)).is_err()
        );
    }
}
