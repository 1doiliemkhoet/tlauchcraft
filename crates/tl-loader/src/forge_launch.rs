//! Chat4 -> tl-game Forge 1.16.5 launch adapter after new instance staging.
//! Verifies the staged on-disk Forge classpath and Mojang client; deliberately
//! does NOT spawn Minecraft. Uses tl-game's production Windows argument builder.
use crate::{
    forge_playable::{forge_java8_launch_version, ForgeRuntimeReceipt, VerifiedForgeLibrary},
    LoaderError,
};
use serde_json::Value;
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
    handoff: &ForgeRuntimeReceipt,
) -> Result<Value, LoaderError> {
    if handoff.schema != "tl-forge-runtime-receipt/v1"
        || handoff.required_java_major != 8
        || handoff.processor_count != 4
        || handoff.instance_ready
        || handoff.game_launched
        || !handoff.execution_verified
        || handoff.minecraft_version != "1.16.5"
        || handoff.derived_version_id
            != format!("1.16.5-forge-{}", handoff.forge_version)
    {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let rebuilt = forge_java8_launch_version(&handoff.inherited_version)?;
    if rebuilt != handoff.java8_launch_version {
        return Err(LoaderError::InvalidMetadata(
            "Java 8 Forge launch view has been modified since receipt verification".into(),
        ));
    }
    Ok(rebuilt)
}

fn canonical_path(root: &Path, relative: &str) -> Result<PathBuf, LoaderError> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
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
    receipt: &ForgeRuntimeReceipt,
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
    for entry in &receipt.libraries {
        if !seen.insert(entry.coordinate.as_str()) {
            return Err(LoaderError::InvalidMetadata(
                "duplicate verified Forge runtime coordinate".into(),
            ));
        }
        let target = canonical_path(&paths.root, &entry.relative_path)?;
        verify_installed_file(&target, entry)?;
    }
    let generated_count = receipt.libraries.iter().filter(|library| {
        matches!(
            library.origin,
            crate::forge_playable::ForgeLibraryOrigin::VerifiedInstallerOrProcessor
        )
    }).count();
    if generated_count != 2 || receipt.libraries.len() <= 2 {
        return Err(LoaderError::InvalidMetadata(
            "Forge generated or Maven inventory incomplete".into(),
        ));
    }
    let client = paths
        .root
        .join("versions")
        .join(&receipt.derived_version_id)
        .join(format!("{}.jar", receipt.derived_version_id));
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

fn verify_installed_file(target: &Path, entry: &VerifiedForgeLibrary) -> Result<(), LoaderError> {
    verify_staged_bytes(target, &entry.official_sha1, entry.size_bytes)
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
    use serde_json::json;
    use crate::forge_playable::{ForgeLibraryOrigin, VerifiedForgeLibrary};
    #[test]
    fn forge_runtime_receipt_requires_real_verified_execution_and_immutable_java8_view() {
        let version = json!({
            "id":"1.16.5-forge-36.2.29",
            "inheritsFrom":"1.16.5",
            "mainClass":"cpw.mods.modlauncher.Launcher",
            "arguments":{"jvm":["--add-exports=java.base/sun.security.util=ALL-UNNAMED"],
                         "game":["--launchTarget","fmlclient"]}
        });
        let java8 = forge_java8_launch_version(&version).unwrap();
        let mut receipt = ForgeRuntimeReceipt {
            schema: "tl-forge-runtime-receipt/v1".into(),
            pack_archive_sha256: "a".repeat(64),
            minecraft_version: "1.16.5".into(),
            forge_version: "36.2.29".into(),
            derived_version_id: "1.16.5-forge-36.2.29".into(),
            required_java_major: 8,
            verified_java_sha1: "0".repeat(40),
            stage_root: PathBuf::from("C:/isolated-processor-stage"),
            processor_count: 4,
            libraries: vec![VerifiedForgeLibrary {
                coordinate: "mock:library:1".into(),
                relative_path: "mock/library/1/library-1.jar".into(),
                path: PathBuf::from("C:/verified/maven.jar"),
                official_sha1: "1".repeat(40),
                size_bytes: 2,
                origin: ForgeLibraryOrigin::OfficialMaven
            }],
            inherited_version: version,
            java8_launch_version: java8.clone(),
            execution_verified: false,
            instance_ready: false,
            game_launched: false,
        };
        assert!(java8_compatible_forge_version(&receipt).is_err());
        receipt.execution_verified = true;
        assert_eq!(java8_compatible_forge_version(&receipt).unwrap(), java8);
        receipt.java8_launch_version["mainClass"] = json!("net.minecraft.client.main.Main");
        assert!(java8_compatible_forge_version(&receipt).is_err());
        receipt.java8_launch_version = java8;
        receipt.instance_ready = true;
        assert!(java8_compatible_forge_version(&receipt).is_err());
    }
}
