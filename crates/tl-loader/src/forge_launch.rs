//! Chat4 -> tl-game Forge 1.16.5 launch adapter after new instance staging.
//! Verifies the staged on-disk Forge classpath and Mojang client; deliberately
//! does NOT spawn Minecraft. Uses tl-game's production Windows argument builder.
use crate::{
    forge_playable::{ForgeInstalledArtifact, ForgePlayableHandoff},
    LoaderError,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use tl_game::{
    vanilla::{build_vanilla_launch, VanillaIdentity, VanillaPaths},
    LaunchConfig,
};

/// Java 8 cannot parse Java 9+ module --add-opens/--add-exports options.
/// Official Forge's legacy installer metadata includes these flags for newer
/// JVMs. Strip only the known module flags while preserving all other arguments.
pub fn java8_compatible_forge_version(
    handoff: &ForgePlayableHandoff,
) -> Result<Value, LoaderError> {
    if handoff.schema_version != 1
        || handoff.java_major != 8
        || handoff.verified_processor_count != 4
        || handoff.instance_ready
        || !handoff.can_stage_new_instance
        || handoff.minecraft_version != "1.16.5"
        || handoff.version_id != format!("1.16.5-forge-{}", handoff.forge_version)
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let mut version = handoff.resolved_version.clone();
    if version["id"] != handoff.version_id
        || version["mainClass"] != "cpw.mods.modlauncher.Launcher"
        || version["inheritsFrom"] != "1.16.5"
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    let obj = version.as_object_mut().ok_or(LoaderError::InvalidInput)?;
    obj.insert(
        "javaVersion".into(),
        json!({"component":"jre-legacy","majorVersion":8}),
    );
    if let Some(items) = obj
        .get_mut("arguments")
        .and_then(Value::as_object_mut)
        .and_then(|args| args.get_mut("jvm"))
        .and_then(Value::as_array_mut)
    {
        items.retain(|item| {
            !item
                .as_str()
                .is_some_and(|v| v.starts_with("--add-opens=") || v.starts_with("--add-exports="))
        });
    }
    Ok(version)
}

fn canonical_path(root: &Path, relative: &str) -> Result<PathBuf, LoaderError> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(LoaderError::InvalidMetadata(
            "unsafe official Maven classpath relative path".into(),
        ));
    }
    Ok(root.join("libraries").join(path))
}

/// Never starts Java. Chat 4 first stages its new verified Mojang client JAR
/// under versions/<Forge ID>/<Forge ID>.jar and its audited Forge/Mojang library
/// inventory under libraries/, native DLLs and assets under dedicated paths.
/// Chat 2's validated tl-game builder then resolves exact inherited Forge main
/// class, game args, JVM args and classpath. This function never creates files.
pub fn build_verified_forge_launch(
    receipt: &ForgePlayableHandoff,
    paths: &VanillaPaths,
    identity: &VanillaIdentity,
    min_ram_mb: u32,
    max_ram_mb: u32,
) -> Result<LaunchConfig, LoaderError> {
    let version = java8_compatible_forge_version(receipt)?;
    if !paths.root.is_absolute()
        || !paths.game_dir.is_absolute()
        || !paths.natives_dir.is_absolute()
        || !paths.java.is_absolute()
        || paths.root.ancestors().any(|x| x.is_symlink())
        || paths.game_dir.ancestors().any(|x| x.is_symlink())
    {
        return Err(LoaderError::InvalidInput);
    }
    let mut seen = BTreeSet::new();
    for entry in receipt
        .generated_artifacts
        .iter()
        .chain(&receipt.official_libraries)
    {
        if !seen.insert(entry.coordinate.as_str()) {
            return Err(LoaderError::InvalidMetadata(
                "duplicate verified Forge runtime coordinate".into(),
            ));
        }
        let target = canonical_path(&paths.root, &entry.relative_path)?;
        verify_installed_file(&target, entry)?;
    }
    if receipt.generated_artifacts.len() != 2 || receipt.official_libraries.is_empty() {
        return Err(LoaderError::InvalidMetadata(
            "Forge generated or Maven inventory incomplete".into(),
        ));
    }
    let client = paths
        .root
        .join("versions")
        .join(&receipt.version_id)
        .join(format!("{}.jar", receipt.version_id));
    let expected = version["downloads"]["client"]["sha1"]
        .as_str()
        .ok_or_else(|| LoaderError::InvalidMetadata("missing Mojang client SHA-1".into()))?;
    let size = version["downloads"]["client"]["size"]
        .as_u64()
        .ok_or_else(|| LoaderError::InvalidMetadata("missing Mojang client size".into()))?;
    verify_staged_bytes(&client, expected, size)?;
    let launch =
        build_vanilla_launch(&version, paths, identity, min_ram_mb, max_ram_mb).map_err(|e| {
            LoaderError::InvalidMetadata(format!("verified Forge launch staging incomplete: {e}"))
        })?;
    if launch.main_class != "cpw.mods.modlauncher.Launcher" {
        return Err(LoaderError::CompatibilityConflict);
    }
    Ok(launch)
}

fn verify_installed_file(target: &Path, entry: &ForgeInstalledArtifact) -> Result<(), LoaderError> {
    verify_staged_bytes(target, &entry.sha1, entry.size_bytes)
}
fn verify_staged_bytes(target: &Path, digest: &str, len: u64) -> Result<(), LoaderError> {
    if !target.is_file()
        || target.ancestors().any(|a| a.is_symlink())
        || digest.len() != 40
        || !digest.bytes().all(|b| b.is_ascii_hexdigit())
        || len == 0
    {
        return Err(LoaderError::InvalidMetadata(format!(
            "missing or unsafe staged Forge file {}",
            target.display()
        )));
    }
    if std::fs::metadata(target)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?
        .len()
        != len
        || crate::runtime::verify_sha1(target, digest).is_err()
    {
        return Err(LoaderError::InvalidMetadata(format!(
            "staged Forge file integrity mismatch {}",
            target.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_jvm_flags_removed_but_actual_launch_target_kept() {
        let report = ForgePlayableHandoff {
            schema_version: 1,
            pack_archive_sha256: "a".repeat(64),
            minecraft_version: "1.16.5".into(),
            forge_version: "36.2.29".into(),
            version_id: "1.16.5-forge-36.2.29".into(),
            java_major: 8,
            derived_main_class: "cpw.mods.modlauncher.Launcher".into(),
            resolved_version: json!({"id":"1.16.5-forge-36.2.29",
            "inheritsFrom":"1.16.5","mainClass":"cpw.mods.modlauncher.Launcher",
            "arguments":{"jvm":["-XX:+IgnoreUnrecognizedVMOptions",
              "--add-opens=java.base/java.util.jar=ALL-UNNAMED"],
              "game":["--launchTarget","fmlclient"]}}),
            verified_processor_count: 4,
            generated_artifacts: vec![],
            official_libraries: vec![],
            stage_root: PathBuf::from("C:/fresh"),
            can_stage_new_instance: true,
            instance_ready: false,
            java_executed_by_this_function: false,
        };
        let valid = java8_compatible_forge_version(&report).unwrap();
        assert_eq!(valid["javaVersion"]["majorVersion"], 8);
        assert_eq!(valid["mainClass"], "cpw.mods.modlauncher.Launcher");
        assert_eq!(
            valid["arguments"]["game"],
            json!(["--launchTarget", "fmlclient"])
        );
        assert_eq!(
            valid["arguments"]["jvm"],
            json!(["-XX:+IgnoreUnrecognizedVMOptions"])
        );
        let mut false_ready = report;
        false_ready.instance_ready = true;
        assert!(java8_compatible_forge_version(&false_ready).is_err());
    }
}
