//! Read-only operator gate: cargo run --locked -p tl-lan --example forge_lan_gate -- gate.json
//! The machine-readable gate JSON must be issued by trusted Chat 4 installation.
use serde::Deserialize;
use std::{env, fs, net::Ipv4Addr, path::PathBuf, process};
use tl_core::vanilla_offline_server_uuid;
use tl_lan::{
    forge_handoff::{verify_two_installed_clients, InstalledPackReceipt},
    require_stable_identity, validate_server_spec, LanServerSpec,
};

#[derive(Deserialize)]
struct ClientInput {
    root: PathBuf,
    receipt: InstalledPackReceipt,
    profile_id: String,
    original_username: String,
    join_username: String,
    effective_game_uuid: String,
    offline_server_uuid: String,
}

#[derive(Deserialize)]
struct GateInput {
    host: ClientInput,
    peer: ClientInput,
    host_private_ipv4: Ipv4Addr,
    port: u16,
    acknowledged_offline_risk: bool,
    confirmed_private_network: bool,
}

fn checked_identity(input: &ClientInput) -> Result<(), String> {
    require_stable_identity(&input.original_username, &input.join_username)
        .map_err(|e| format!("username stability: {e:?}"))?;
    let expected = vanilla_offline_server_uuid(&input.join_username)
        .map_err(|e| e.to_string())?
        .to_string();
    if input.profile_id.is_empty()
        || input.profile_id == expected
        || input.effective_game_uuid != expected
        || input.offline_server_uuid != expected
    {
        return Err("offline identity claim differs from Chat 1 canonical UUID".into());
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 2 {
        return Err("usage: forge_lan_gate <chat4-verified-gate.json>".into());
    }
    let metadata = fs::metadata(&args[1]).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 4 * 1024 * 1024 {
        return Err("gate manifest must be a regular JSON file <=4 MiB".into());
    }
    let input: GateInput = serde_json::from_slice(&fs::read(&args[1]).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let spec = LanServerSpec {
        server_root: input.host.root.clone(),
        bind: input.host_private_ipv4,
        port: input.port,
        max_players: 2,
        whitelist: true,
        acknowledged_offline_risk: input.acknowledged_offline_risk,
        confirmed_private_network: input.confirmed_private_network,
    };
    validate_server_spec(&spec).map_err(|e| format!("LAN safety: {e:?}"))?;
    checked_identity(&input.host)?;
    checked_identity(&input.peer)?;
    if input.host.profile_id == input.peer.profile_id
        || input
            .host
            .join_username
            .eq_ignore_ascii_case(&input.peer.join_username)
        || input.host.offline_server_uuid == input.peer.offline_server_uuid
    {
        return Err("two clients must have distinct profiles, nicknames and server UUIDs".into());
    }
    verify_two_installed_clients(
        &input.host.root,
        &input.host.receipt,
        &input.peer.root,
        &input.peer.receipt,
    )
    .map_err(|e| format!("pack parity: {e:?}"))?;
    println!("LAN_PREFLIGHT_PASS: exact Forge/client files and offline UUID claims matched");
    println!("G6_NOT_VERIFIED: no Minecraft handshake or gameplay has been tested");
    Ok(())
}

fn main() {
    if let Err(reason) = run(&env::args().collect::<Vec<_>>()) {
        eprintln!("LAN_PREFLIGHT_BLOCKED: {reason}");
        process::exit(2);
    }
}
