//! Assemble verified Mojang and Forge files into an EXISTING private pack stage.
//! Does not execute Forge processors, publish an instance or grant READY.
//! Uses a shared official Mojang cache to avoid downloading assets per instance.
use sha1::{Digest, Sha1};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use tl_game::{
    metadata::{plan_asset_objects, plan_version, FileRole},
    vanilla::{extract_natives, install_vanilla},
    vanilla_evidence::validate_install_files,
    Environment,
};
use tl_loader::forge_playable::ForgeRuntimeReceipt;

#[derive(Debug, Clone)]
pub struct ForgeStageAssembly {
    pub minecraft_files: usize,
    pub forge_libraries: usize,
    pub native_archives: usize,
    pub cache_root: PathBuf,
}

fn safe_ancestors(path: &Path) -> Result<(), String> {
    for part in path.ancestors() {
        if fs::symlink_metadata(part).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!(
                "symlinked Forge stage/cache path: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn verify_bytes(path: &Path, sha1: &str, size: u64) -> Result<(), String> {
    safe_ancestors(path)?;
    if sha1.len() != 40 || !sha1.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("source lacks official SHA-1".into());
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() != size {
        return Err("source file type/size mismatch".into());
    }
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut sha = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
    }
    if format!("{:x}", sha.finalize()) != sha1.to_ascii_lowercase() {
        return Err(format!(
            "official file checksum mismatch: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Immutable copy into a fresh stage. Never overwrites an existing world/mod
/// or an occupied destination with different bytes; verifies both sides.
fn copy_verified(
    source: &Path,
    stage: &Path,
    relative: &str,
    sha1: &str,
    bytes: u64,
) -> Result<(), String> {
    let rel = crate::safe_path(relative).map_err(|e| e.to_string())?;
    let target = stage.join(rel);
    verify_bytes(source, sha1, bytes)?;
    safe_ancestors(&target)?;
    if target.exists() {
        return verify_bytes(&target, sha1, bytes);
    }
    let parent = target.parent().ok_or("no destination parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    safe_ancestors(parent)?;
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| e.to_string())?;
    let copy_result = std::io::copy(&mut input, &mut output).and_then(|count| {
        if count != bytes {
            return Err(std::io::Error::other("partial cache copy"));
        }
        output.sync_all()
    });
    if let Err(e) = copy_result {
        drop(output);
        let _ = fs::remove_file(&target);
        return Err(e.to_string());
    }
    verify_bytes(&target, sha1, bytes)
}

/// Fetches only official Mojang files (or reuses their SHA-1-verified cache).
/// The Forge receipt is read-only staging input, NOT an issuer of a publish
/// permit. Later publication independently revalidates a sealed execution
/// proof and every installed artifact.
pub fn assemble_forge_stage(
    stage: &Path,
    mojang_cache: &Path,
    mojang_parent: &serde_json::Value,
    forge: &ForgeRuntimeReceipt,
    cancel: &AtomicBool,
) -> Result<ForgeStageAssembly, String> {
    if !stage.is_absolute()
        || !mojang_cache.is_absolute()
        || stage == mojang_cache
        || stage.starts_with(mojang_cache)
        || mojang_cache.starts_with(stage)
        || !stage
            .file_name()
            .and_then(|x| x.to_str())
            .is_some_and(|s| s.ends_with(".staging"))
        || !fs::symlink_metadata(stage).is_ok_and(|m| m.file_type().is_dir())
    {
        return Err(
            "existing private pack stage and distinct absolute Mojang cache required".into(),
        );
    }
    safe_ancestors(stage)?;
    safe_ancestors(mojang_cache)?;
    if forge.instance_ready
        || forge.game_launched
        || !forge.execution_verified
        || forge.processor_count != 4
        || forge.minecraft_version != mojang_parent["id"]
        || forge.java8_launch_version["inheritsFrom"] != forge.minecraft_version
        || forge.java8_launch_version["id"] != forge.derived_version_id
    {
        return Err("Forge stage requires matching verified runtime and Mojang parent".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled before preparing Mojang cache".into());
    }
    // Reuse cache when hashes agree. install_vanilla never overwrites a
    // pre-existing invalid cache object and downloads only from Mojang allowlist.
    let cached = install_vanilla(mojang_parent, mojang_cache, cancel)
        .map_err(|e| format!("official Minecraft cache install: {e}"))?;
    validate_install_files(mojang_parent, mojang_cache)
        .map_err(|e| format!("official Minecraft cache integrity: {e}"))?;
    if cached.native_archives.is_empty() {
        return Err("official Windows native archive inventory empty".into());
    }
    let plan = plan_version(mojang_parent, mojang_cache, &Environment::current())
        .map_err(|e| e.to_string())?;
    let index = plan
        .iter()
        .find(|f| f.role == FileRole::AssetIndex)
        .ok_or("Mojang asset index missing")?;
    let index_json = serde_json::from_slice(&fs::read(&index.target).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Mojang asset index unreadable: {e}"))?;
    let assets = plan_asset_objects(&index_json, mojang_cache).map_err(|e| e.to_string())?;
    let mut installed = 0usize;
    for entry in plan.iter().chain(&assets) {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled during cache assembly".into());
        }
        let relative = entry
            .target
            .strip_prefix(mojang_cache)
            .map_err(|_| "Minecraft cached path escaped root")?
            .to_str()
            .ok_or("invalid cached path encoding")?
            .replace('\\', "/");
        let expected = entry.size.unwrap_or(
            fs::metadata(&entry.target)
                .map_err(|e| e.to_string())?
                .len(),
        );
        copy_verified(&entry.target, stage, &relative, &entry.sha1, expected)?;
        installed += 1;
        if entry.role == FileRole::ClientJar {
            let id = &forge.derived_version_id;
            if !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
            {
                return Err("unsafe derived Forge version identifier".into());
            }
            // Inherited Forge uses parent Minecraft's official client bytes,
            // but tl-game plans the selected derived version's client path.
            let selected = format!("versions/{id}/{id}.jar");
            copy_verified(&entry.target, stage, &selected, &entry.sha1, expected)?;
        }
    }
    for entry in &forge.libraries {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled during Forge library assembly".into());
        }
        copy_verified(
            &entry.path,
            stage,
            &format!("libraries/{}", entry.relative_path),
            &entry.official_sha1,
            entry.size_bytes,
        )?;
    }
    // The native extractor takes verified official Windows archives and must
    // refuse an occupied target, so existing world/game files are never replaced.
    let natives = stage.join("natives");
    if natives.exists() {
        return Err("native extraction destination occupied".into());
    }
    extract_natives(&cached.native_archives, &natives)
        .map_err(|e| format!("Windows natives extraction failed: {e}"))?;
    Ok(ForgeStageAssembly {
        minecraft_files: installed,
        forge_libraries: forge.libraries.len(),
        native_archives: cached.native_archives.len(),
        cache_root: mojang_cache.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_copy_requires_digest_and_preserves_existing_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let stage = root.join("new.staging");
        fs::create_dir(&stage).unwrap();
        let input = root.join("source.jar");
        fs::write(&input, b"official").unwrap();
        let sha = format!("{:x}", Sha1::digest(b"official"));
        copy_verified(&input, &stage, "libraries/library.jar", &sha, 8).unwrap();
        fs::write(stage.join("libraries/library.jar"), b"tampered").unwrap();
        assert!(copy_verified(&input, &stage, "libraries/library.jar", &sha, 8).is_err());
        assert_eq!(
            fs::read(stage.join("libraries/library.jar")).unwrap(),
            b"tampered"
        );
        assert!(copy_verified(&input, &stage, "../escape", &sha, 8).is_err());
    }

    #[test]
    fn unchecked_receipt_cannot_trigger_network_or_publish() {
        let tmp = tempfile::tempdir().unwrap();
        let stage = tmp.path().canonicalize().unwrap();
        let empty = ForgeRuntimeReceipt {
            schema: "unchecked".into(),
            pack_archive_sha256: "f".repeat(64),
            minecraft_version: "1.16.5".into(),
            forge_version: "36.2.34".into(),
            derived_version_id: "1.16.5-forge-36.2.34".into(),
            required_java_major: 8,
            verified_java_sha1: "f".repeat(40),
            stage_root: stage.clone(),
            processor_count: 0,
            libraries: vec![],
            inherited_version: serde_json::json!({}),
            java8_launch_version: serde_json::json!({}),
            execution_verified: false,
            instance_ready: false,
            game_launched: false,
        };
        let cache = stage.join("official-cache");
        assert!(assemble_forge_stage(
            &stage,
            &cache,
            &serde_json::json!({"id":"1.16.5"}),
            &empty,
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(!cache.exists());
    }
}
