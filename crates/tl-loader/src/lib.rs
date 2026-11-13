//! Loader-only metadata resolution. No game process spawning or writes to player data.
//! Chat 1/2 provide transport, verified artifact acquisition, and launch execution.
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod fabric;
pub mod maven;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind { Fabric, Forge, NeoForge }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderSpec {
    pub kind: LoaderKind,
    pub minecraft_version: String,
    pub loader_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Canonical HTTPS artifact URL: never a mirror from untrusted input.
    pub url: String,
    /// Maven-relative path. FileInstaller must validate it under its cache root.
    pub relative_path: String,
    /// Optional SHA-1 from upstream metadata (not a substitute for SHA-256 evidence).
    pub expected_sha1: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderPlan {
    pub loader: LoaderSpec,
    /// Derived Minecraft version metadata ID, never a caller-supplied shell string.
    pub version_id: String,
    pub inherits_from: String,
    pub main_class: Option<String>,
    pub libraries: Vec<Artifact>,
    pub required_java_major: u32,
    pub installer: Option<Artifact>,
    /// No process must be launched unless this is true and installation was verified.
    pub ready_for_metadata_merge: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LoaderError {
    #[error("invalid loader metadata: {0}")] InvalidMetadata(String),
    #[error("invalid version or Maven coordinate")] InvalidInput,
    #[error("Minecraft version mismatch")] CompatibilityConflict,
    #[error("installed Java major {actual} is below required major {required}")]
    JavaIncompatible { required: u32, actual: u32 },
    #[error("installer processor execution and verification are required")]
    NeedsInstallerExecution,
    #[error("unsupported loader metadata: {0}")] UnsupportedVersion(String),
}

pub trait LoaderInstaller {
    fn plan(&self, spec: &LoaderSpec, metadata: &str, game_java_major: u32)
        -> Result<LoaderPlan, LoaderError>;
}

pub fn validate_component(v: &str) -> Result<(), LoaderError> {
    if v.is_empty() || v.len() > 128
        || !v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
        || v == "." || v == ".." || v.contains("..")
    { return Err(LoaderError::InvalidInput); }
    Ok(())
}

pub fn check_java(required: u32, actual: u32) -> Result<(), LoaderError> {
    if required == 0 || actual == 0 { return Err(LoaderError::InvalidInput); }
    if actual < required {
        return Err(LoaderError::JavaIncompatible { required, actual });
    }
    Ok(())
}

pub const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";
pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
