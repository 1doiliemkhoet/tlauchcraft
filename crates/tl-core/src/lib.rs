//! Shared v1 typed contract. Local profiles are not online authentication.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};
use uuid::Uuid;

pub const CONTRACT_VERSION: u32 = 1;
pub mod identity;
pub use identity::{
    derive_vanilla_offline_identity, preview_rename_impact, vanilla_offline_server_uuid,
    OfflineGameIdentity, RenameImpact,
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflineProfile {
    pub id: Uuid,
    pub username: String,
    pub skin_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl OfflineProfile {
    pub fn new(username: impl Into<String>) -> Result<Self, CoreError> {
        let username = username.into();
        validate_username(&username)?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            username,
            skin_id: None,
            created_at: now,
            updated_at: now,
        })
    }
    pub fn rename(&mut self, username: impl Into<String>) -> Result<bool, CoreError> {
        let username = username.into();
        validate_username(&username)?;
        if self.username == username {
            return Ok(false);
        }
        self.username = username;
        self.updated_at = Utc::now();
        Ok(true)
    }
}
pub fn validate_username(s: &str) -> Result<(), CoreError> {
    if !(3..=16).contains(&s.len()) || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderSpec {
    pub kind: LoaderKind,
    pub version: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    Empty,
    Inspecting,
    Planned,
    Downloading,
    Verifying,
    Ready,
    NeedsAction,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameInstance {
    pub id: Uuid,
    pub display_name: String,
    pub root_path: String,
    pub minecraft_version: String,
    pub loader: LoaderSpec,
    pub profile_id: Uuid,
    pub state: InstanceState,
}
fn version_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
}
impl GameInstance {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        loader: LoaderSpec,
        profile_id: Uuid,
    ) -> Result<Self, CoreError> {
        let id = Uuid::new_v4();
        let i = Self {
            id,
            display_name: name.into(),
            root_path: format!("instances/{id}"),
            minecraft_version: version.into(),
            loader,
            profile_id,
            state: InstanceState::Empty,
        };
        i.validate()?;
        Ok(i)
    }
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.root_path != format!("instances/{}", self.id)
            || self.display_name.trim().is_empty()
            || self.display_name.len() > 128
            || self.display_name.chars().any(char::is_control)
            || !version_ok(&self.minecraft_version)
            || self.loader.version.as_ref().is_some_and(|v| !version_ok(v))
            || matches!(self.loader.kind, LoaderKind::Vanilla) && self.loader.version.is_some()
        {
            return Err(CoreError::InvalidInput);
        }
        validate_safe_relative_path(&self.root_path)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModpackSourceKind {
    CurseForgeZip,
    ModrinthMrpack,
    PrismZip,
    GenericZip,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModpackSource {
    pub r#type: ModpackSourceKind,
    pub archive_sha256: String,
    pub source_metadata: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSide {
    Client,
    Server,
    Both,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedFile {
    pub path: String,
    pub expected_sha1: Option<String>,
    pub expected_sha512: Option<String>,
    pub actual_sha256: String,
    pub origin: String,
    pub side: FileSide,
    pub required: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallReport {
    pub instance_id: Uuid,
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
    pub blocked: Vec<String>,
    pub failed: Vec<String>,
    pub bytes_downloaded: u64,
    pub evidence_path: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRequest {
    pub instance_id: Uuid,
    pub profile_id: Uuid,
    pub java_path: Option<String>,
    pub min_ram_mb: Option<u32>,
    pub max_ram_mb: Option<u32>,
    pub extra_jvm_args: Option<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationState {
    Running,
    Exited,
    Crashed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchProcess {
    pub pid: u32,
    pub started_at: DateTime<Utc>,
    pub log_path: String,
    pub termination_state: TerminationState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanServerSpec {
    pub instance_id: Uuid,
    pub trusted_bind: String,
    pub port: u16,
    pub whitelist: bool,
    pub online_mode: bool,
    pub server_root: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinModel {
    Classic,
    Slim,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinSelection {
    pub profile_id: Uuid,
    pub skin_id: Uuid,
    pub model: SkinModel,
    pub cape_id: Option<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub contract_version: u32,
    pub operation_id: Uuid,
    pub instance_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub phase: String,
    pub message_key: String,
    pub details: serde_json::Value,
    pub percent: Option<f32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cancellation {
    pub operation_id: Uuid,
    pub requested: bool,
}
pub const EVENT_NAMES: &[&str] = &[
    "profile.changed",
    "instance.changed",
    "download.progress",
    "download.verified",
    "pack.inspect",
    "pack.needs_action",
    "pack.ready",
    "loader.installed",
    "java.resolved",
    "game.started",
    "game.exited",
    "lan.server_started",
    "lan.peer_joined",
    "skin.selected",
    "skin.local_verified",
    "skin.peer_verified",
];
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "PascalCase")]
pub enum CoreError {
    #[error("invalid input")]
    InvalidInput,
    #[error("unsupported version")]
    UnsupportedVersion,
    #[error("network failure")]
    NetworkFailure,
    #[error("integrity mismatch")]
    IntegrityMismatch,
    #[error("unauthorized source")]
    UnauthorizedSource,
    #[error("manual download required")]
    NeedsManualDownload,
    #[error("compatibility conflict")]
    CompatibilityConflict,
    #[error("disk full")]
    DiskFull,
    #[error("cancelled")]
    Cancelled,
    #[error("launch failed")]
    LaunchFailed,
    #[error("unsafe archive")]
    UnsafeArchive,
    #[error("not verified")]
    NotVerified,
    #[error("record not found")]
    NotFound,
    #[error("already exists")]
    AlreadyExists,
    #[error("storage failed")]
    StorageFailure,
}
/// Validate a portable archive path, then independently check symlinks before IO.
pub fn validate_safe_relative_path(s: &str) -> Result<(), CoreError> {
    if s.is_empty() || s.contains('\\') || s.contains(':') || s.chars().any(char::is_control) {
        return Err(CoreError::InvalidInput);
    }
    let p = Path::new(s);
    if p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(CoreError::InvalidInput);
    }
    for segment in s.split('/') {
        let stem = segment.split('.').next().unwrap_or("").to_ascii_lowercase();
        if segment.is_empty()
            || segment.ends_with('.')
            || segment.ends_with(' ')
            || segment.chars().any(|c| "<>\"|?*".contains(c))
            || matches!(
                stem.as_str(),
                "con"
                    | "prn"
                    | "aux"
                    | "nul"
                    | "com1"
                    | "com2"
                    | "com3"
                    | "com4"
                    | "com5"
                    | "com6"
                    | "com7"
                    | "com8"
                    | "com9"
                    | "lpt1"
                    | "lpt2"
                    | "lpt3"
                    | "lpt4"
                    | "lpt5"
                    | "lpt6"
                    | "lpt7"
                    | "lpt8"
                    | "lpt9"
            )
        {
            return Err(CoreError::InvalidInput);
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn username_validation() {
        assert!(OfflineProfile::new("Steve_123").is_ok());
        for bad in [
            "a",
            "contains space",
            "éeee",
            "abc/def",
            "0123456789abcdef0",
        ] {
            assert!(OfflineProfile::new(bad).is_err());
        }
    }
    #[test]
    fn path_validation() {
        for bad in [
            "../world", "C:/Users", "/root", "a\\b", "a//b", "CON", "COM1.txt",
        ] {
            assert!(validate_safe_relative_path(bad).is_err(), "{bad}");
        }
        assert!(validate_safe_relative_path("instances/abc/mods/a.jar").is_ok());
    }
    #[test]
    fn identity_persists_on_rename() {
        let mut p = OfflineProfile::new("Original").unwrap();
        let id = p.id;
        p.rename("Renamed").unwrap();
        assert_eq!(id, p.id);
    }
    #[test]
    fn instance_roundtrip() {
        let p = OfflineProfile::new("Player").unwrap();
        let instance = GameInstance::new(
            "Test",
            "1.21.1",
            LoaderSpec {
                kind: LoaderKind::Fabric,
                version: Some("0.16.10".into()),
            },
            p.id,
        )
        .unwrap();
        let copied: GameInstance =
            serde_json::from_str(&serde_json::to_string(&instance).unwrap()).unwrap();
        assert_eq!(instance, copied);
    }
}
