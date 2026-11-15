//! Source-neutral on-disk artifact audit before any loader attestation or
//! atomic publish. This is a necessary check, never a sufficient READY proof.
use crate::safe_path;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::Read,
    path::Path,
};

const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedArtifact {
    pub relative_path: String,
    pub required: bool,
    pub bytes: u64,
    /// SHA-256 is always required for bytes already staged.
    pub sha256: String,
    /// SHA-1 or SHA-512 must come from an authoritative source manifest or
    /// approved provider, not solely from a local, computed download digest.
    pub upstream_sha1: Option<String>,
    pub upstream_sha512: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactAudit {
    pub complete: bool,
    pub required_expected: usize,
    pub required_verified: usize,
    pub optional_verified: usize,
    pub blocked: Vec<String>,
    pub failures: Vec<String>,
    /// Always false. Loader/Java/game proof requires an independent adapter.
    pub ready: bool,
}

fn valid_digest(value: &str, digits: usize) -> bool {
    value.len() == digits && value.bytes().all(|c| c.is_ascii_hexdigit())
}

fn verify_file(root: &Path, artifact: &ExpectedArtifact) -> Result<(), String> {
    if artifact.bytes == 0 || artifact.bytes > MAX_ARTIFACT_BYTES
        || !valid_digest(&artifact.sha256, 64)
    {
        return Err("invalid claimed byte count or SHA-256".into());
    }
    let sha1 = artifact.upstream_sha1.as_deref();
    let sha512 = artifact.upstream_sha512.as_deref();
    if sha1.is_none() && sha512.is_none() {
        return Err("missing independently published upstream hash".into());
    }
    if sha1.is_some_and(|v| !valid_digest(v, 40))
        || sha512.is_some_and(|v| !valid_digest(v, 128))
    {
        return Err("invalid upstream hash".into());
    }
    let relative = safe_path(&artifact.relative_path)
        .map_err(|_| "unsafe artifact relative path")?;
    let path = root.join(relative);
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("symlinked artifact or parent".into());
        }
    }
    let metadata = fs::symlink_metadata(&path).map_err(|_| "artifact absent")?;
    if !metadata.file_type().is_file() || metadata.len() != artifact.bytes {
        return Err("artifact is not a regular file of expected size".into());
    }
    let mut file = File::open(&path).map_err(|_| "artifact unreadable")?;
    let mut sha256_state = Sha256::new();
    let mut sha1_state = Sha1::new();
    let mut sha512_state = Sha512::new();
    let mut read_total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|_| "artifact read failure")?;
        if n == 0 {
            break;
        }
        read_total = read_total.checked_add(n as u64)
            .ok_or("artifact length overflow")?;
        if read_total > artifact.bytes {
            return Err("artifact size changed during hashing".into());
        }
        sha256_state.update(&buffer[..n]);
        sha1_state.update(&buffer[..n]);
        sha512_state.update(&buffer[..n]);
    }
    if read_total != artifact.bytes
        || format!("{:x}", sha256_state.finalize()) != artifact.sha256.to_ascii_lowercase()
        || sha1.is_some_and(|v| format!("{:x}", sha1_state.finalize()) != v.to_ascii_lowercase())
        || sha512.is_some_and(|v| format!("{:x}", sha512_state.finalize()) != v.to_ascii_lowercase())
    {
        return Err("artifact digest mismatch".into());
    }
    Ok(())
}

/// Re-read all planned files from a private staging root. Missing optional
/// files remain visible but do not independently invalidate a required audit.
/// No filesystem mutation occurs. This never mints a publish permit.
pub fn audit_staged_artifacts(root: &Path, expected: &[ExpectedArtifact]) -> ArtifactAudit {
    let mut result = ArtifactAudit {
        complete: false,
        required_expected: expected.iter().filter(|f| f.required).count(),
        required_verified: 0,
        optional_verified: 0,
        blocked: Vec::new(),
        failures: Vec::new(),
        ready: false,
    };
    if !root.is_absolute()
        || !fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_dir())
    {
        result.failures.push("staging root must be an existing, regular absolute directory".into());
        return result;
    }
    for ancestor in root.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            result.failures.push("staging root has symlinked ancestors".into());
            return result;
        }
    }
    let mut seen = HashSet::new();
    for artifact in expected {
        let key = artifact.relative_path.to_ascii_lowercase();
        if !seen.insert(key) {
            result.failures.push("duplicate destination in expected inventory".into());
            continue;
        }
        match verify_file(root, artifact) {
            Ok(()) => {
                if artifact.required {
                    result.required_verified += 1;
                } else {
                    result.optional_verified += 1;
                }
            }
            Err(reason) => {
                let detail = format!("{}: {reason}", artifact.relative_path);
                if artifact.required {
                    result.failures.push(detail);
                } else {
                    result.blocked.push(detail);
                }
            }
        }
    }
    result.complete = result.failures.is_empty()
        && result.required_verified == result.required_expected;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(bytes: &[u8], relative_path: &str, required: bool) -> ExpectedArtifact {
        ExpectedArtifact {
            relative_path: relative_path.into(),
            required,
            bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
            upstream_sha1: Some(format!("{:x}", Sha1::digest(bytes))),
            upstream_sha512: None,
        }
    }
    #[test]
    fn required_present_and_verified_still_never_ready() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir(root.join("mods")).unwrap();
        fs::write(root.join("mods/a.jar"), b"trusted fixture").unwrap();
        let a = audit_staged_artifacts(&root, &[fixture(b"trusted fixture", "mods/a.jar", true)]);
        assert!(a.complete);
        assert_eq!(a.required_verified, 1);
        assert!(!a.ready);
    }
    #[test]
    fn missing_required_and_tamper_prevent_completion_and_preserve_world() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir(root.join("mods")).unwrap();
        fs::create_dir(root.join("saves")).unwrap();
        fs::write(root.join("saves/level.dat"), b"existing").unwrap();
        fs::write(root.join("mods/a.jar"), b"tampered").unwrap();
        let expected = [
            fixture(b"intended", "mods/a.jar", true),
            fixture(b"missing", "mods/b.jar", true),
        ];
        let r = audit_staged_artifacts(&root, &expected);
        assert!(!r.complete);
        assert_eq!(r.required_verified, 0);
        assert_eq!(r.failures.len(), 2);
        assert_eq!(fs::read(root.join("saves/level.dat")).unwrap(), b"existing");
    }
    #[test]
    fn optional_absence_reported_but_required_completes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir(root.join("mods")).unwrap();
        fs::write(root.join("mods/a.jar"), b"a").unwrap();
        let r = audit_staged_artifacts(&root, &[
            fixture(b"a", "mods/a.jar", true),
            fixture(b"b", "mods/b.jar", false),
        ]);
        assert!(r.complete);
        assert_eq!(r.blocked.len(), 1);
        assert!(!r.ready);
    }
    #[test]
    fn unsafe_or_duplicate_artifact_does_not_complete() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir(root.join("mods")).unwrap();
        fs::write(root.join("mods/a.jar"), b"a").unwrap();
        let r = audit_staged_artifacts(&root, &[
            fixture(b"a", "mods/a.jar", true),
            fixture(b"a", "mods/A.jar", true),
            fixture(b"a", "../outside.jar", true),
        ]);
        assert!(!r.complete);
        assert_eq!(r.failures.len(), 2);
    }
    #[test]
    fn locally_computed_sha256_without_upstream_origin_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir(root.join("mods")).unwrap();
        fs::write(root.join("mods/a.jar"), b"a").unwrap();
        let mut artifact = fixture(b"a", "mods/a.jar", true);
        artifact.upstream_sha1 = None;
        let r = audit_staged_artifacts(&root, &[artifact]);
        assert!(!r.complete);
        assert!(!r.ready);
    }
}
