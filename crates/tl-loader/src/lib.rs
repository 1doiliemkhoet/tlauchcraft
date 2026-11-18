//! Loader metadata adapters. The actual FileInstaller and GameRuntime live in tl-game.
//! No download, installer execution, game launch or user-world mutations happen here.
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use tl_core::{GameInstance, LoaderKind, LoaderSpec};
pub mod fabric;
pub mod forge_1165;
pub mod forge_evidence_cache;
pub mod forge_family;
pub mod forge_graph;
pub mod forge_playable;
pub mod forge_provenance;
pub mod installer;
pub mod loader_runtime;
pub mod loader_matrix;
pub mod maven;
pub mod runtime;
#[cfg(windows)]
mod windows_job;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub relative_path: String,
    pub expected_sha1: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderPlan {
    pub loader: LoaderSpec,
    pub version_id: String,
    pub inherits_from: String,
    pub main_class: Option<String>,
    pub libraries: Vec<Artifact>,
    pub required_java_major: u32,
    pub installer: Option<Artifact>,
    /// Metadata can be merged; NOT proof that files have been installed or verified.
    pub ready_for_metadata_merge: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LoaderError {
    #[error("invalid loader metadata: {0}")]
    InvalidMetadata(String),
    #[error("invalid version or Maven coordinate")]
    InvalidInput,
    #[error("Minecraft version mismatch")]
    CompatibilityConflict,
    #[error("installed Java major {actual} is below required major {required}")]
    JavaIncompatible { required: u32, actual: u32 },
    #[error("installer processor execution and verification are required")]
    NeedsInstallerExecution,
    #[error("unsupported loader metadata: {0}")]
    UnsupportedVersion(String),
}

impl From<LoaderError> for tl_core::CoreError {
    fn from(value: LoaderError) -> Self {
        match value {
            LoaderError::InvalidInput | LoaderError::InvalidMetadata(_) => Self::InvalidInput,
            LoaderError::CompatibilityConflict | LoaderError::JavaIncompatible { .. } => {
                Self::CompatibilityConflict
            }
            LoaderError::UnsupportedVersion(_) => Self::UnsupportedVersion,
            LoaderError::NeedsInstallerExecution => Self::NotVerified,
        }
    }
}

/// Minecraft version is supplied separately because tl-core's LoaderSpec owns only
/// the loader kind/version. No competing type is introduced in this crate.
pub trait LoaderInstaller {
    fn plan(
        &self,
        minecraft_version: &str,
        spec: &LoaderSpec,
        metadata: &str,
        game_java_major: u32,
    ) -> Result<LoaderPlan, LoaderError>;
}

pub fn plan_for_instance(
    instance: &GameInstance,
    metadata: &str,
    game_java_major: u32,
) -> Result<LoaderPlan, LoaderError> {
    instance.validate().map_err(|_| LoaderError::InvalidInput)?;
    match &instance.loader.kind {
        LoaderKind::Fabric => fabric::Fabric.plan(
            &instance.minecraft_version,
            &instance.loader,
            metadata,
            game_java_major,
        ),
        LoaderKind::Forge => maven::Forge.plan(
            &instance.minecraft_version,
            &instance.loader,
            metadata,
            game_java_major,
        ),
        LoaderKind::NeoForge => maven::NeoForge.plan(
            &instance.minecraft_version,
            &instance.loader,
            metadata,
            game_java_major,
        ),
        LoaderKind::Vanilla => Err(LoaderError::InvalidInput),
    }
}

/// Share Chat 2's version inheritance engine. This merges metadata only:
/// the caller must independently verify actual artifact downloads and game launch.
pub fn merge_fabric_version(
    parent: &serde_json::Value,
    profile: &serde_json::Value,
    instance: &GameInstance,
    required_java_major: u32,
) -> Result<serde_json::Value, LoaderError> {
    if instance.loader.kind != LoaderKind::Fabric {
        return Err(LoaderError::InvalidInput);
    }
    let profile_text =
        serde_json::to_string(profile).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let plan = plan_for_instance(instance, &profile_text, required_java_major)?;
    if parent.get("id").and_then(serde_json::Value::as_str)
        != Some(instance.minecraft_version.as_str())
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    if !plan.ready_for_metadata_merge {
        return Err(LoaderError::NeedsInstallerExecution);
    }
    let result = tl_game::inherit(parent, profile)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    if result.get("mainClass").and_then(serde_json::Value::as_str) != plan.main_class.as_deref() {
        return Err(LoaderError::InvalidMetadata(
            "merged mainClass differs".into(),
        ));
    }
    Ok(result)
}

pub fn validate_component(value: &str) -> Result<(), LoaderError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
        || value == "."
        || value == ".."
        || value.contains("..")
    {
        return Err(LoaderError::InvalidInput);
    }
    Ok(())
}

pub fn check_java(required: u32, actual: u32) -> Result<(), LoaderError> {
    if required == 0 || actual == 0 {
        return Err(LoaderError::InvalidInput);
    }
    if actual < required {
        return Err(LoaderError::JavaIncompatible { required, actual });
    }
    Ok(())
}

pub const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";
pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
