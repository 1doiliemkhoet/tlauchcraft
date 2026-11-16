//! Read-only bridge between Chat 2's actual launched Minecraft processes,
//! Chat 4's installed modpack receipts, and Chat 1's canonical offline IDs.
//! This module NEVER launches untrusted code, changes firewall rules, or mints G6 PASS.
use crate::{
    direct_connect_address,
    forge_handoff::{verify_two_installed_clients, InstalledPackReceipt},
    validate_offline_username, LanError, Result,
};
use std::{fs, net::Ipv4Addr, path::Path};
use tl_core::{
    derive_vanilla_offline_identity, GameInstance, InstanceState, LaunchProcess, LaunchRequest,
    OfflineProfile, TerminationState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanLaunchPreflight {
    pub run_id: String,
    pub host_username: String,
    pub peer_username: String,
    pub host_offline_uuid: String,
    pub peer_offline_uuid: String,
    pub minecraft_version: String,
    pub forge_version: String,
    pub archive_sha256: String,
    pub host_pid: u32,
    pub peer_pid: u32,
}

pub struct RunningClient<'a> {
    pub profile: &'a OfflineProfile,
    pub instance: &'a GameInstance,
    pub launch_request: &'a LaunchRequest,
    pub launch_process: &'a LaunchProcess,
    pub installed_root: &'a Path,
    pub receipt: &'a InstalledPackReceipt,
}

fn check_client(client: &RunningClient<'_>) -> Result<String> {
    validate_offline_username(&client.profile.username)?;
    if client.instance.profile_id != client.profile.id
        || client.launch_request.profile_id != client.profile.id
        || client.launch_request.instance_id != client.instance.id
        || client.instance.id.to_string() != client.receipt.instance_id
    {
        return Err(LanError::CompatibilityConflict(vec![
            "launcher profile, instance, request or pack receipt identity mismatch".into(),
        ]));
    }
    if client.instance.state != InstanceState::Ready
        || client.instance.minecraft_version != client.receipt.minecraft_version
        || client.instance.loader.kind != tl_core::LoaderKind::Forge
        || client.instance.loader.version.as_deref() != Some(client.receipt.loader_version.as_str())
    {
        return Err(LanError::NotVerified(
            "verified Forge 1.16.5 READY instance not bound to launch".into(),
        ));
    }
    if client.launch_process.pid == 0
        || client.launch_process.termination_state != TerminationState::Running
        || client.launch_process.log_path.trim().is_empty()
    {
        return Err(LanError::NotVerified(
            "runtime has not provided running Minecraft PID and log path".into(),
        ));
    }
    // The caller must supply the actual game-process PID obtained from Chat 2.
    // An integer supplied by untrusted JSON alone is NOT OS process attestation.
    let computed = derive_vanilla_offline_identity(client.profile)
        .map_err(|e| LanError::InvalidInput(e.to_string()))?;
    if computed.profile_id == computed.offline_server_uuid {
        return Err(LanError::CompatibilityConflict(vec![
            "internal profile UUID must not be used as offline server UUID".into(),
        ]));
    }
    Ok(computed.offline_server_uuid.to_string())
}

/// Must be invoked by the integrated launcher AFTER each real Minecraft spawn,
/// with its actual returned LaunchProcess, not with an invented PID or bool.
/// File hashes are recomputed from the published directories on both machines.
pub fn preflight_running_clients(
    host: &RunningClient<'_>,
    peer: &RunningClient<'_>,
) -> Result<LanLaunchPreflight> {
    let host_uuid = check_client(host)?;
    let peer_uuid = check_client(peer)?;
    if host.profile.id == peer.profile.id
        || host.instance.id == peer.instance.id
        || host.launch_process.pid == peer.launch_process.pid
        || host.profile.username.eq_ignore_ascii_case(&peer.profile.username)
        || host_uuid == peer_uuid
    {
        return Err(LanError::CompatibilityConflict(vec![
            "LAN clients need separate profiles, instances, running processes and usernames".into(),
        ]));
    }
    verify_two_installed_clients(
        host.installed_root,
        host.receipt,
        peer.installed_root,
        peer.receipt,
    )?;
    Ok(LanLaunchPreflight {
        run_id: host.receipt.run_id.clone(),
        host_username: host.profile.username.clone(),
        peer_username: peer.profile.username.clone(),
        host_offline_uuid: host_uuid,
        peer_offline_uuid: peer_uuid,
        minecraft_version: host.receipt.minecraft_version.clone(),
        forge_version: host.receipt.loader_version.clone(),
        archive_sha256: host.receipt.archive_sha256.clone(),
        host_pid: host.launch_process.pid,
        peer_pid: peer.launch_process.pid,
    })
}

/// Read the actual local server's Open-to-LAN log after the user opens the world.
/// Reject a stale/logless or ambiguous port; do not treat a TCP probe as a login.
/// The caller confirms the game log belongs to the intended live host process.
pub fn observed_open_to_lan_port(log: &Path) -> Result<u16> {
    let metadata = fs::symlink_metadata(log)?;
    if !metadata.file_type().is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err(LanError::NotVerified("invalid or oversized Minecraft log".into()));
    }
    let log_data = fs::read_to_string(log)?;
    let mut last_port = None;
    for line in log_data.lines() {
        // Vanilla 1.16.x integrated server prints 'Local game hosted on port N'
        // on successful LAN world sharing. Require this marker, not a bare port.
        let Some((_, tail)) = line.split_once("Local game hosted on port ") else {
            continue;
        };
        let port_text = tail.trim();
        if !port_text.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(port) = port_text.parse::<u16>() {
            if port != 0 {
                last_port = Some(port);
            }
        }
    }
    last_port.ok_or_else(|| LanError::NotVerified(
        "no confirmed Minecraft Open-to-LAN port in host log".into(),
    ))
}

/// Use the port shown by Minecraft's Open-to-LAN menu if a log line is not
/// available. The operator must independently confirm it belongs to this run.
pub fn direct_connect_from_displayed_port(
    host_private_ip: Ipv4Addr,
    displayed_port: u16,
) -> Result<String> {
    direct_connect_address(host_private_ip, displayed_port)
}

/// Build a Direct Connect destination only for an operator-approved private IP.
/// No socket is opened and no Windows Firewall/router configuration is touched.
pub fn direct_connect_from_observed_log(
    host_private_ip: Ipv4Addr,
    host_log: &Path,
) -> Result<String> {
    direct_connect_address(host_private_ip, observed_open_to_lan_port(host_log)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    use tl_core::{LoaderKind, LoaderSpec};
    use crate::forge_handoff::VerifiedPackFile;

    fn receipt(id: String, hash: String) -> InstalledPackReceipt {
        InstalledPackReceipt {
            run_id: "single-run".into(), instance_id: id,
            archive_sha256: "a".repeat(64), minecraft_version: "1.16.5".into(),
            loader_kind: "forge".into(), loader_version: "36.2.29".into(),
            installed_and_verified: true, loader_execution_verified: true,
            files: vec![VerifiedPackFile {
                relative_path: "mods/verified.jar".into(), sha256: hash,
                side: "both".into(), gameplay: true,
            }],
        }
    }
    fn launch_fixture() -> (OfflineProfile, GameInstance, LaunchRequest, LaunchProcess) {
        let profile = OfflineProfile::new("PlayerA").unwrap();
        let mut instance = GameInstance::new(
            "Modded", "1.16.5",
            LoaderSpec { kind: LoaderKind::Forge, version: Some("36.2.29".into()) },
            profile.id,
        ).unwrap();
        instance.state = InstanceState::Ready;
        let request = LaunchRequest {
            instance_id: instance.id, profile_id: profile.id,
            java_path: None, min_ram_mb: None, max_ram_mb: None, extra_jvm_args: None,
        };
        let process = serde_json::from_value(serde_json::json!({
            "pid": 12345, "started_at": "2026-11-16T02:00:00Z",
            "log_path": "logs/latest.log", "termination_state": "running"
        })).unwrap();
        (profile, instance, request, process)
    }
    #[test]
    fn rejects_profile_instance_or_launch_mismatch() {
        let (profile, instance, mut request, process) = launch_fixture();
        let root = tempfile::tempdir().unwrap();
        let receipt = receipt(instance.id.to_string(), "a".repeat(64));
        request.profile_id = OfflineProfile::new("Another").unwrap().id;
        let client = RunningClient {
            profile: &profile, instance: &instance, launch_request: &request,
            launch_process: &process, installed_root: root.path(), receipt: &receipt,
        };
        assert!(check_client(&client).is_err());
    }
    #[test]
    fn rejects_not_ready_or_exited_process() {
        let (profile, mut instance, request, mut process) = launch_fixture();
        let root = tempfile::tempdir().unwrap();
        let receipt = receipt(instance.id.to_string(), "a".repeat(64));
        instance.state = InstanceState::NeedsAction;
        let c = RunningClient {
            profile: &profile, instance: &instance, launch_request: &request,
            launch_process: &process, installed_root: root.path(), receipt: &receipt,
        };
        assert!(check_client(&c).is_err());
        instance.state = InstanceState::Ready;
        process.termination_state = TerminationState::Exited;
        let c = RunningClient {
            profile: &profile, instance: &instance, launch_request: &request,
            launch_process: &process, installed_root: root.path(), receipt: &receipt,
        };
        assert!(check_client(&c).is_err());
    }
    #[test]
    fn verifies_two_actual_install_roots_before_lan_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let (a_root, b_root) = (directory.path().join("a"), directory.path().join("b"));
        for root in [&a_root, &b_root] {
            fs::create_dir_all(root.join("mods")).unwrap();
            fs::write(root.join("mods/verified.jar"), b"fixture").unwrap();
        }
        let hash = format!("{:x}", Sha256::digest(b"fixture"));
        let (a_profile, a_instance, a_request, a_process) = launch_fixture();
        let b_profile = OfflineProfile::new("PlayerB").unwrap();
        let mut b_instance = GameInstance::new(
            "Peer", "1.16.5",
            LoaderSpec { kind: LoaderKind::Forge, version: Some("36.2.29".into()) },
            b_profile.id,
        ).unwrap();
        b_instance.state = InstanceState::Ready;
        let b_request = LaunchRequest {
            instance_id: b_instance.id, profile_id: b_profile.id,
            java_path: None, min_ram_mb: None, max_ram_mb: None, extra_jvm_args: None,
        };
        let mut b_process = a_process.clone();
        b_process.pid = 12346;
        let a_receipt = receipt(a_instance.id.to_string(), hash.clone());
        let b_receipt = receipt(b_instance.id.to_string(), hash);
        let a = RunningClient {
            profile: &a_profile, instance: &a_instance, launch_request: &a_request,
            launch_process: &a_process, installed_root: &a_root, receipt: &a_receipt,
        };
        let b = RunningClient {
            profile: &b_profile, instance: &b_instance, launch_request: &b_request,
            launch_process: &b_process, installed_root: &b_root, receipt: &b_receipt,
        };
        let report = preflight_running_clients(&a, &b).unwrap();
        assert_ne!(report.host_offline_uuid, report.peer_offline_uuid);
        assert_ne!(report.host_pid, report.peer_pid);
        fs::write(b_root.join("mods/verified.jar"), b"tampered").unwrap();
        assert!(preflight_running_clients(&a, &b).is_err());
    }
    #[test]
    fn log_port_requires_real_marker_and_private_ip() {
        let dir = tempfile::tempdir().unwrap();
        let log: PathBuf = dir.path().join("latest.log");
        fs::write(&log, b"Listening on port 25565\n").unwrap();
        assert!(observed_open_to_lan_port(&log).is_err());
        fs::write(&log, b"[Server thread/INFO]: Local game hosted on port 51234\n").unwrap();
        assert_eq!(observed_open_to_lan_port(&log).unwrap(), 51234);
        assert!(direct_connect_from_observed_log(Ipv4Addr::new(8, 8, 8, 8), &log).is_err());
        assert_eq!(
            direct_connect_from_observed_log(Ipv4Addr::new(192, 168, 1, 10), &log).unwrap(),
            "192.168.1.10:51234",
        );
        assert_eq!(
            direct_connect_from_displayed_port(Ipv4Addr::new(192, 168, 1, 10), 51234).unwrap(),
            "192.168.1.10:51234",
        );
    }
}
