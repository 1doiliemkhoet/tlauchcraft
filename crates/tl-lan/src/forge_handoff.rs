//! Fail-closed LAN pack handoff for Chat 4's *verified and published* instance.
//! Reads installed bytes; does not install packs or authorize gameplay.
use crate::{LanError, PackFile, PackFingerprint, Result, Side};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path};

const MAX_FILES: usize = 20_000;
const MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedPackFile {
    pub relative_path: String,
    pub sha256: String,
    pub side: String,
    pub gameplay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackReceipt {
    pub run_id: String,
    pub instance_id: String,
    pub archive_sha256: String,
    pub minecraft_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub installed_and_verified: bool,
    pub loader_execution_verified: bool,
    pub files: Vec<VerifiedPackFile>,
}

fn sha256_file(path: &Path, expected: &str) -> Result<()> {
    let info = fs::symlink_metadata(path)?;
    if !info.file_type().is_file() || info.len() == 0 || info.len() > MAX_FILE_SIZE {
        return Err(LanError::InvalidInput(
            "missing or unsafe installed pack file".into(),
        ));
    }
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    if format!("{:x}", digest.finalize()) != expected.to_ascii_lowercase() {
        return Err(LanError::CompatibilityConflict(vec![
            "installed mod/config SHA-256 differs from the verified receipt".into(),
        ]));
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Independently rehash a published instance. A receipt with only a READY flag
/// cannot satisfy this guard: required files must be present and byte-matched.
/// This is *not* proof that the modpack boots or that Forge processors executed.
pub fn fingerprint_installed_pack(
    root: &Path,
    receipt: &InstalledPackReceipt,
) -> Result<PackFingerprint> {
    if !receipt.installed_and_verified || !receipt.loader_execution_verified {
        return Err(LanError::NotVerified(
            "pack publication or Forge execution is unverified".into(),
        ));
    }
    if receipt.run_id.trim().is_empty()
        || receipt.instance_id.trim().is_empty()
        || receipt.minecraft_version != "1.16.5"
        || receipt.loader_kind != "forge"
        || receipt.loader_version.trim().is_empty()
        || !valid_sha256(&receipt.archive_sha256)
        || receipt.files.is_empty()
        || receipt.files.len() > MAX_FILES
    {
        return Err(LanError::InvalidInput(
            "incomplete Forge 1.16.5 instance receipt".into(),
        ));
    }
    let base = fs::canonicalize(root)?;
    if !base.is_dir() {
        return Err(LanError::InvalidInput("instance is not a directory".into()));
    }
    let mut files = Vec::with_capacity(receipt.files.len());
    let mut seen = BTreeMap::new();
    for declared in &receipt.files {
        let relative = Path::new(&declared.relative_path);
        if declared.relative_path.contains('\\')
            || declared.relative_path.contains(':')
            || declared.relative_path.contains("//")
            || declared.relative_path.contains('\0')
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || !valid_sha256(&declared.sha256)
        {
            return Err(LanError::InvalidInput("unsafe pack receipt file".into()));
        }
        let key = declared.relative_path.to_ascii_lowercase();
        if seen.insert(key, ()).is_some() {
            return Err(LanError::InvalidInput("duplicate pack destination".into()));
        }
        let side = match declared.side.as_str() {
            "client" => Side::Client,
            "server" => Side::Server,
            "both" => Side::Both,
            _ => return Err(LanError::InvalidInput("unknown mod side".into())),
        };
        let path = base.join(relative);
        let canonical = fs::canonicalize(&path)?;
        if !canonical.starts_with(&base) {
            return Err(LanError::InvalidInput(
                "installed file escaped instance root".into(),
            ));
        }
        // Reject symlinks/reparse targets; do not trust a malicious substituted file.
        let mut current = base.clone();
        for part in relative.components() {
            current.push(part.as_os_str());
            if fs::symlink_metadata(&current)?.file_type().is_symlink() {
                return Err(LanError::InvalidInput(
                    "symlink in installed mod path".into(),
                ));
            }
        }
        sha256_file(&canonical, &declared.sha256)?;
        files.push(PackFile {
            relative_path: declared.relative_path.clone(),
            sha256: declared.sha256.to_ascii_lowercase(),
            side,
            gameplay: declared.gameplay,
        });
    }
    // Prevent an undeclared extra mod from silently bypassing parity just
    // because Chat 4's receipt omitted it. Read-only; do not delete mods.
    let mods = base.join("mods");
    if mods.exists() {
        let mut pending = vec![mods];
        let mut visited = 0usize;
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory)? {
                visited += 1;
                if visited > MAX_FILES {
                    return Err(LanError::InvalidInput(
                        "installed mod inventory exceeds safety bound".into(),
                    ));
                }
                let entry = entry?;
                let ty = entry.file_type()?;
                if ty.is_symlink() {
                    return Err(LanError::InvalidInput(
                        "symlink in installed mods directory".into(),
                    ));
                }
                if ty.is_dir() {
                    pending.push(entry.path());
                } else if ty.is_file()
                    && entry.path().extension().and_then(|v| v.to_str())
                        .is_some_and(|v| v.eq_ignore_ascii_case("jar"))
                {
                    let name = entry.path().strip_prefix(&base)
                        .map_err(|_| LanError::InvalidInput("mod escaped instance".into()))?
                        .to_string_lossy()
                        .replace('\\', "/")
                        .to_ascii_lowercase();
                    if !seen.contains_key(&name) {
                        return Err(LanError::CompatibilityConflict(vec![
                            "undeclared installed mod JAR prevents verified parity".into(),
                        ]));
                    }
                }
            }
        }
    }
    Ok(PackFingerprint {
        minecraft: receipt.minecraft_version.clone(),
        loader_kind: receipt.loader_kind.clone(),
        loader_version: receipt.loader_version.clone(),
        files,
    })
}

/// One shared run must name the same exact source manifest on both machines.
/// Different instance IDs are expected. No world files or profiles are touched.
pub fn verify_two_installed_clients(
    host_root: &Path,
    host: &InstalledPackReceipt,
    peer_root: &Path,
    peer: &InstalledPackReceipt,
) -> Result<()> {
    if host.run_id != peer.run_id
        || host.instance_id == peer.instance_id
        || !host
            .archive_sha256
            .eq_ignore_ascii_case(&peer.archive_sha256)
    {
        return Err(LanError::CompatibilityConflict(vec![
            "LAN peers do not share the same run, distinct instances and pack manifest".into(),
        ]));
    }
    let host_fingerprint = fingerprint_installed_pack(host_root, host)?;
    let peer_fingerprint = fingerprint_installed_pack(peer_root, peer)?;
    compare_two_clients(&host_fingerprint, &peer_fingerprint)
}

/// Two actual client installs should have exact side-aware hashes and paths.
/// Unlike the server/client comparison, client-only gameplay files must match
/// across both clients. This remains a *preflight*, not a handshake.
pub fn compare_two_clients(a: &PackFingerprint, b: &PackFingerprint) -> Result<()> {
    if a.minecraft != "1.16.5"
        || b.minecraft != "1.16.5"
        || a.loader_kind != "forge"
        || b.loader_kind != "forge"
        || a.loader_version.is_empty()
        || a.loader_version != b.loader_version
    {
        return Err(LanError::CompatibilityConflict(vec![
            "Minecraft/Forge versions do not match exactly".into(),
        ]));
    }
    fn index(pack: &PackFingerprint) -> Result<BTreeMap<String, (String, Side, bool)>> {
        let mut out = BTreeMap::new();
        if pack.files.is_empty() {
            return Err(LanError::NotVerified(
                "empty installed pack fingerprint".into(),
            ));
        }
        for f in &pack.files {
            if !valid_sha256(&f.sha256) {
                return Err(LanError::InvalidInput("invalid pack file hash".into()));
            }
            let key = f.relative_path.to_ascii_lowercase();
            if out
                .insert(
                    key,
                    (f.sha256.to_ascii_lowercase(), f.side.clone(), f.gameplay),
                )
                .is_some()
            {
                return Err(LanError::InvalidInput("duplicate pack file".into()));
            }
        }
        Ok(out)
    }
    if index(a)? != index(b)? {
        return Err(LanError::CompatibilityConflict(vec![
            "installed client pack files, sides or hashes differ".into(),
        ]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn receipt(digest: &str) -> InstalledPackReceipt {
        InstalledPackReceipt {
            run_id: "e2e-001".into(),
            instance_id: "instance-001".into(),
            archive_sha256: "a".repeat(64),
            minecraft_version: "1.16.5".into(),
            loader_kind: "forge".into(),
            loader_version: "36.2.29".into(),
            installed_and_verified: true,
            loader_execution_verified: true,
            files: vec![VerifiedPackFile {
                relative_path: "mods/test.jar".into(),
                sha256: digest.into(),
                side: "both".into(),
                gameplay: true,
            }],
        }
    }
    #[test]
    fn byte_verified_two_clients_match() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("mods")).unwrap();
        fs::write(root.join("mods/test.jar"), b"verified fixture").unwrap();
        let digest = format!("{:x}", Sha256::digest(b"verified fixture"));
        let first = fingerprint_installed_pack(root, &receipt(&digest)).unwrap();
        let second = fingerprint_installed_pack(root, &receipt(&digest)).unwrap();
        assert!(compare_two_clients(&first, &second).is_ok());
    }
    #[test]
    fn changed_mod_bytes_fail_preflight() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("mods")).unwrap();
        fs::write(temp.path().join("mods/test.jar"), b"tampered").unwrap();
        let digest = format!("{:x}", Sha256::digest(b"expected"));
        assert!(fingerprint_installed_pack(temp.path(), &receipt(&digest)).is_err());
    }
    #[test]
    fn failed_forge_receipt_never_claims_ready() {
        let mut r = receipt(&"a".repeat(64));
        r.loader_execution_verified = false;
        assert!(matches!(
            fingerprint_installed_pack(Path::new("missing"), &r),
            Err(LanError::NotVerified(_))
        ));
    }
    #[test]
    fn loader_or_client_only_hash_mismatch_blocks() {
        let mut a = PackFingerprint {
            minecraft: "1.16.5".into(),
            loader_kind: "forge".into(),
            loader_version: "36.2.29".into(),
            files: vec![PackFile {
                relative_path: "mods/cosmetic.jar".into(),
                sha256: "a".repeat(64),
                side: Side::Client,
                gameplay: false,
            }],
        };
        let mut b = a.clone();
        b.files[0].sha256 = "b".repeat(64);
        assert!(compare_two_clients(&a, &b).is_err());
        b = a.clone();
        b.loader_version = "36.2.34".into();
        assert!(compare_two_clients(&a, &b).is_err());
        a.files.push(a.files[0].clone());
        assert!(compare_two_clients(&a, &a).is_err());
    }
    #[test]
    fn source_manifest_or_instance_mismatch_blocks() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("mods")).unwrap();
        fs::write(temp.path().join("mods/test.jar"), b"fixture").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"fixture"));
        let host = receipt(&hash);
        let mut peer = receipt(&hash);
        peer.instance_id = "peer-002".into();
        assert!(verify_two_installed_clients(temp.path(), &host, temp.path(), &peer).is_ok());
        peer.archive_sha256 = "b".repeat(64);
        assert!(verify_two_installed_clients(temp.path(), &host, temp.path(), &peer).is_err());
        peer.archive_sha256 = host.archive_sha256.clone();
        peer.instance_id = host.instance_id.clone();
        assert!(verify_two_installed_clients(temp.path(), &host, temp.path(), &peer).is_err());
    }
    #[test]
    fn undeclared_extra_mod_blocks_even_when_receipt_files_match() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("mods")).unwrap();
        fs::write(temp.path().join("mods/test.jar"), b"fixture").unwrap();
        fs::write(temp.path().join("mods/untracked.jar"), b"extra").unwrap();
        let digest = format!("{:x}", Sha256::digest(b"fixture"));
        assert!(matches!(
            fingerprint_installed_pack(temp.path(), &receipt(&digest)),
            Err(LanError::CompatibilityConflict(_))
        ));
    }
    #[test]
    fn unsafe_receipt_path_rejected() {
        let mut r = receipt(&"a".repeat(64));
        r.files[0].relative_path = "../world/playerdata.dat".into();
        assert!(fingerprint_installed_pack(Path::new("."), &r).is_err());
    }
}
