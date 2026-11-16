//! Atomic publication boundary, intentionally inaccessible to the UI.
//! A permit is minted only after the in-process Forge runtime verifier and
//! complete on-disk pack/asset audit succeed. UI JSON is never a permit.
use std::{fs, path::Path};

/// Not serializable and not constructible outside tl-pack.
#[derive(Debug)]
pub struct VerifiedPublishPermit {
    _sealed: (),
}

impl VerifiedPublishPermit {
    /// Restricted to the internal, fail-closed Forge publication bridge.
    /// This does not deserialize an attestation or accept UI-supplied flags.
    pub(crate) fn after_verified_forge_publish_audit() -> Self {
        Self { _sealed: () }
    }
}

/// Native no-replace move. std::fs::rename may replace an existing
/// destination on Unix, even if a preflight existence check passed.
#[cfg(windows)]
fn atomic_create_new_move(stage: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileW(existing: *const u16, new: *const u16) -> i32;
    }
    let from: Vec<u16> = stage.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileW(from.as_ptr(), to.as_ptr()) } == 0 {
        return Err(format!("atomic create-new instance move failed: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn atomic_create_new_move(stage: &Path, destination: &Path) -> Result<(), String> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    extern "C" {
        fn renameat2(old_dir: i32, old_path: *const i8, new_dir: i32, new_path: *const i8, flags: u32) -> i32;
    }
    const AT_FDCWD: i32 = -100;
    const RENAME_NOREPLACE: u32 = 1;
    let from = CString::new(stage.as_os_str().as_bytes()).map_err(|_| "invalid stage path")?;
    let to = CString::new(destination.as_os_str().as_bytes()).map_err(|_| "invalid destination path")?;
    if unsafe { renameat2(AT_FDCWD, from.as_ptr(), AT_FDCWD, to.as_ptr(), RENAME_NOREPLACE) } != 0 {
        return Err(format!("atomic create-new instance move failed: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn atomic_create_new_move(_stage: &Path, _destination: &Path) -> Result<(), String> {
    Err("atomic no-replace publish is unsupported on this platform".into())
}

/// Publish an *already verified* private staging directory as a new instance.
/// The stage and final directory must be siblings on one filesystem. Existing
/// instances/worlds are never overwritten or deleted. No cross-volume copy.
pub fn publish_verified(
    stage: &Path,
    destination: &Path,
    _proof: VerifiedPublishPermit,
) -> Result<(), String> {
    if !stage.is_absolute() || !destination.is_absolute() {
        return Err("publish paths must be absolute".into());
    }
    let stage_parent = stage.parent().ok_or("stage has no parent")?;
    if destination.parent() != Some(stage_parent) {
        return Err("stage and new instance must be in the same parent".into());
    }
    let stage_name = stage
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid stage name")?;
    let destination_name = destination
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid instance name")?;
    if !stage_name.ends_with(".staging")
        || stage_name.len() <= ".staging".len()
        || !destination_name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || destination_name.len() < 8
    {
        return Err("unsafe private staging or instance identifier".into());
    }
    for dir in [stage_parent, stage, destination] {
        for ancestor in dir.ancestors() {
            if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("publish path contains symlink".into());
            }
        }
    }
    let stage_metadata = fs::symlink_metadata(stage).map_err(|_| "stage missing")?;
    if !stage_metadata.file_type().is_dir() {
        return Err("private staging directory is not regular directory".into());
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err("existing target instance must not be replaced".into());
    }
    atomic_create_new_move(stage, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_existing_world_and_forbids_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let stage = base.join("first-job.staging");
        let existing = base.join("instance-1");
        fs::create_dir(&stage).unwrap();
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("level.dat"), b"saved world").unwrap();
        assert!(
            publish_verified(&stage, &existing, VerifiedPublishPermit { _sealed: () },).is_err()
        );
        assert!(stage.exists());
        assert_eq!(
            fs::read(existing.join("level.dat")).unwrap(),
            b"saved world"
        );
    }
    #[test]
    fn publishes_only_a_new_sibling_when_trusted_permit_exists() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let stage = base.join("new-job.staging");
        let final_dir = base.join("new-instance-1");
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("instance.json"), b"verified fixture").unwrap();
        publish_verified(&stage, &final_dir, VerifiedPublishPermit { _sealed: () }).unwrap();
        assert!(!stage.exists());
        assert_eq!(
            fs::read(final_dir.join("instance.json")).unwrap(),
            b"verified fixture"
        );
    }
    #[test]
    fn refuses_cross_parent_and_unsafe_destination() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let stage = base.join("job-stage.staging");
        fs::create_dir(&stage).unwrap();
        assert!(publish_verified(
            &stage,
            &base.join("..").join("outside-instance"),
            VerifiedPublishPermit { _sealed: () },
        )
        .is_err());
        assert!(publish_verified(
            &stage,
            &base.join("world instance"),
            VerifiedPublishPermit { _sealed: () },
        )
        .is_err());
        assert!(stage.exists());
    }
}
