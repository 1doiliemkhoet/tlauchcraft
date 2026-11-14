//! Read-only LAN acceptance preflight. This module does not launch clients,
//! alter profile IDs, edit worlds or claim proof of Minecraft network play.
use crate::{compare_packs, validate_offline_username, CompatibilityReport, LanError, PackFingerprint, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tl_core::{derive_vanilla_offline_identity, vanilla_offline_server_uuid, OfflineGameIdentity, OfflineProfile};

/// A vanilla/offline server identity belongs to the exact case-sensitive username.
/// An application profile ID is never used as a playerdata filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinIdentity {
    pub profile_id: String,
    pub username: String,
    pub effective_game_uuid: String,
    pub offline_server_uuid: String,
}

impl From<OfflineGameIdentity> for JoinIdentity {
    fn from(value: OfflineGameIdentity) -> Self {
        Self {
            profile_id: value.profile_id.to_string(),
            username: value.username,
            effective_game_uuid: value.effective_game_uuid.to_string(),
            offline_server_uuid: value.offline_server_uuid.to_string(),
        }
    }
}

pub fn expected_join_identity(profile: &OfflineProfile) -> Result<JoinIdentity> {
    let identity = derive_vanilla_offline_identity(profile)
        .map_err(|e| LanError::InvalidInput(e.to_string()))?;
    Ok(identity.into())
}

/// Reject a changed username, client UUID mismatch or different observed server UUID.
/// The observed server value must come from runtime evidence, not just the same formula.
pub fn validate_join_identity(
    expected: &JoinIdentity,
    join_username: &str,
    effective_game_uuid: &str,
    observed_server_uuid: &str,
) -> Result<()> {
    validate_offline_username(join_username)?;
    if expected.username != join_username {
        return Err(LanError::CompatibilityConflict(vec![
            "username mismatch: offline inventory identity can change".into(),
        ]));
    }
    if expected.effective_game_uuid != effective_game_uuid
        || expected.offline_server_uuid != observed_server_uuid
    {
        return Err(LanError::CompatibilityConflict(vec![
            "effective game UUID or observed server UUID changed".into(),
        ]));
    }
    Ok(())
}

/// A preflight report is NOT evidence that a real Minecraft protocol handshake succeeded.
pub fn vanilla_1211_preflight(
    server: &PackFingerprint,
    client: &PackFingerprint,
    player: &OfflineProfile,
) -> Result<(CompatibilityReport, JoinIdentity)> {
    if server.minecraft != "1.21.1" || client.minecraft != "1.21.1" {
        return Err(LanError::CompatibilityConflict(vec![
            "Vanilla LAN harness currently targets Minecraft 1.21.1".into(),
        ]));
    }
    if server.loader_kind != "vanilla" || client.loader_kind != "vanilla"
        || !server.loader_version.is_empty() || !client.loader_version.is_empty()
    {
        return Err(LanError::CompatibilityConflict(vec![
            "Vanilla preflight does not cover modded server loaders".into(),
        ]));
    }
    let report = compare_packs(server, client);
    if !report.compatible {
        return Err(LanError::CompatibilityConflict(report.blockers));
    }
    Ok((report, expected_join_identity(player)?))
}

/// Read-only playerdata snapshot for evidence comparisons. Never log raw bytes.
/// NBT can change legitimately during save; a changed hash is not by itself data loss.
#[derive(Debug, Clone)]
pub struct PlayerdataSnapshot {
    pub path: PathBuf,
    bytes: Vec<u8>,
}

impl PlayerdataSnapshot {
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn matches_disk(&self) -> Result<bool> {
        let data = read_checked_playerdata(&self.path)?;
        Ok(data == self.bytes)
    }
}

fn read_checked_playerdata(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err(LanError::InvalidInput(
            "playerdata must be a regular file at most 8 MiB".into(),
        ));
    }
    Ok(fs::read(path)?)
}

/// The caller must independently confirm the server has stopped saving before
/// treating the snapshot as consistent. No data is written or migrated.
pub fn snapshot_stopped_playerdata(
    world: &Path,
    username: &str,
    server_stopped: bool,
) -> Result<PlayerdataSnapshot> {
    if !server_stopped {
        return Err(LanError::NotVerified("server still running".into()));
    }
    let uuid = vanilla_offline_server_uuid(username)
        .map_err(|e| LanError::InvalidInput(e.to_string()))?;
    let root = fs::canonicalize(world)?;
    let data_dir = root.join("playerdata");
    if fs::symlink_metadata(&data_dir)?.file_type().is_symlink() {
        return Err(LanError::InvalidInput("playerdata directory is a symlink".into()));
    }
    let path = data_dir.join(format!("{uuid}.dat"));
    let bytes = read_checked_playerdata(&path)?;
    Ok(PlayerdataSnapshot { path, bytes })
}

/// Pure state-machine test fixture. Observing transitions here is NOT proof of a
/// real connection, Minecraft handshake or persistence of inventory contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    Prepared,
    Joined,
    Disconnected,
    Rejoined,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct SessionTrace {
    pub phase: SessionPhase,
    pub username: String,
    pub offline_server_uuid: String,
}

impl SessionTrace {
    pub fn new(expected: &JoinIdentity) -> Self {
        Self {
            phase: SessionPhase::Prepared,
            username: expected.username.clone(),
            offline_server_uuid: expected.offline_server_uuid.clone(),
        }
    }

    pub fn transition(
        &mut self,
        next: SessionPhase,
        username: &str,
        server_uuid: &str,
    ) -> Result<()> {
        let valid = matches!(
            (self.phase, next),
            (SessionPhase::Prepared, SessionPhase::Joined)
                | (SessionPhase::Joined, SessionPhase::Disconnected)
                | (SessionPhase::Disconnected, SessionPhase::Rejoined)
                | (SessionPhase::Rejoined, SessionPhase::Disconnected)
                | (SessionPhase::Disconnected, SessionPhase::Stopped)
        );
        if !valid || self.username != username || self.offline_server_uuid != server_uuid {
            return Err(LanError::CompatibilityConflict(vec![
                "unexpected session transition or changed offline identity".into(),
            ]));
        }
        self.phase = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackFingerprint;

    fn vanilla() -> PackFingerprint {
        PackFingerprint {
            minecraft: "1.21.1".into(),
            loader_kind: "vanilla".into(),
            loader_version: String::new(),
            files: vec![],
        }
    }

    #[test]
    fn canonical_identity_stable_across_rejoins() {
        let profile = OfflineProfile::new("TestPlayer").unwrap();
        let first = expected_join_identity(&profile).unwrap();
        let second = expected_join_identity(&profile).unwrap();
        assert_eq!(first, second);
        assert_ne!(first.profile_id, first.offline_server_uuid);
        validate_join_identity(
            &first,
            "TestPlayer",
            &first.effective_game_uuid,
            &first.offline_server_uuid,
        )
        .unwrap();
    }

    #[test]
    fn rejects_username_case_or_uuid_mismatch() {
        let profile = OfflineProfile::new("TestPlayer").unwrap();
        let expected = expected_join_identity(&profile).unwrap();
        assert!(validate_join_identity(
            &expected,
            "testplayer",
            &expected.effective_game_uuid,
            &expected.offline_server_uuid
        )
        .is_err());
        assert!(validate_join_identity(
            &expected,
            "TestPlayer",
            &expected.profile_id,
            &expected.offline_server_uuid
        )
        .is_err());
        assert!(validate_join_identity(
            &expected,
            "TestPlayer",
            &expected.effective_game_uuid,
            &expected.profile_id
        )
        .is_err());
    }

    #[test]
    fn rejects_mismatched_vanilla_version_and_loader() {
        let profile = OfflineProfile::new("TestPlayer").unwrap();
        let server = vanilla();
        let mut client = vanilla();
        client.minecraft = "1.20.1".into();
        assert!(vanilla_1211_preflight(&server, &client, &profile).is_err());
        client = vanilla();
        client.loader_kind = "fabric".into();
        assert!(vanilla_1211_preflight(&server, &client, &profile).is_err());
    }

    #[test]
    fn vanilla_preflight_matches() {
        let profile = OfflineProfile::new("TestPlayer").unwrap();
        let (report, identity) = vanilla_1211_preflight(&vanilla(), &vanilla(), &profile).unwrap();
        assert!(report.compatible);
        assert_eq!(identity.username, "TestPlayer");
    }

    #[test]
    fn reconnect_trace_requires_unchanged_identity() {
        let profile = OfflineProfile::new("TestPlayer").unwrap();
        let identity = expected_join_identity(&profile).unwrap();
        let mut trace = SessionTrace::new(&identity);
        assert!(trace
            .transition(
                SessionPhase::Rejoined,
                &identity.username,
                &identity.offline_server_uuid
            )
            .is_err());
        for phase in [
            SessionPhase::Joined,
            SessionPhase::Disconnected,
            SessionPhase::Rejoined,
            SessionPhase::Disconnected,
            SessionPhase::Stopped,
        ] {
            trace
                .transition(phase, &identity.username, &identity.offline_server_uuid)
                .unwrap();
        }
        assert!(trace
            .transition(SessionPhase::Joined, "OtherUser", &identity.offline_server_uuid)
            .is_err());
    }

    #[test]
    fn playerdata_snapshot_detects_changes_without_writing_world() {
        let profile = OfflineProfile::new("TestPlayer").unwrap();
        let expected = expected_join_identity(&profile).unwrap();
        let root = std::env::temp_dir().join(format!("tl-lan-e2e-{}", std::process::id()));
        let world = root.join("world");
        fs::create_dir_all(world.join("playerdata")).unwrap();
        let playerdata = world
            .join("playerdata")
            .join(format!("{}.dat", expected.offline_server_uuid));
        fs::write(&playerdata, b"inventory before").unwrap();
        assert!(snapshot_stopped_playerdata(&world, "TestPlayer", false).is_err());
        let snapshot = snapshot_stopped_playerdata(&world, "TestPlayer", true).unwrap();
        assert_eq!(snapshot.len(), b"inventory before".len());
        assert!(snapshot.matches_disk().unwrap());
        fs::write(&playerdata, b"inventory after").unwrap();
        assert!(!snapshot.matches_disk().unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn username_change_does_not_resolve_old_playerdata() {
        let profile = OfflineProfile::new("TestPlayer").unwrap();
        let expected = expected_join_identity(&profile).unwrap();
        let root = std::env::temp_dir().join(format!("tl-lan-rename-{}", std::process::id()));
        let world = root.join("world");
        fs::create_dir_all(world.join("playerdata")).unwrap();
        fs::write(
            world
                .join("playerdata")
                .join(format!("{}.dat", expected.offline_server_uuid)),
            b"inventory",
        )
        .unwrap();
        assert!(snapshot_stopped_playerdata(&world, "NewPlayer", true).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
