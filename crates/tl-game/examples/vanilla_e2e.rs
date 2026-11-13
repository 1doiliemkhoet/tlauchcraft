//! Windows-only manual G2/G3/G4 harness. No automated menu/world success claim.
use std::{env, error::Error, fs, path::PathBuf, sync::atomic::AtomicBool};
use tl_game::{
    launch,
    vanilla::{
        build_vanilla_launch, extract_natives, install_vanilla, resolve_official_version,
        write_launch_evidence, VanillaIdentity, VanillaPaths,
    },
};

fn main() -> Result<(), Box<dyn Error>> {
    if !cfg!(windows) {
        return Err("Windows-only E2E harness".into());
    }
    let args: Vec<String> = env::args().collect();
    if args.len() != 6 {
        eprintln!(
            "Usage: vanilla_e2e <official-release-id> <fresh-launcher-root> <java.exe> <username> <game-uuid>"
        );
        return Err("expected five CLI arguments".into());
    }
    // Do not accept access tokens in CLI arguments or log files.
    let token = env::var("TLAUCHCRAFT_ACCESS_TOKEN")
        .map_err(|_| "TLAUCHCRAFT_ACCESS_TOKEN must be provided through environment")?;
    let version_id = &args[1];
    let root = PathBuf::from(&args[2]);
    let java = PathBuf::from(&args[3]);
    let username = &args[4];
    let uuid = &args[5];

    if !root.is_absolute() {
        return Err("use an absolute launcher-owned test root".into());
    }
    fs::create_dir_all(&root)?;
    let game_dir = root.join("instance");
    let native_nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let natives_dir = root.join(format!("natives-{native_nonce}"));
    let evidence = root.join("evidence");
    fs::create_dir_all(&evidence)?;
    eprintln!("Resolving verified official release metadata for {version_id}...");
    let version = resolve_official_version(version_id)?;
    let cancelled = AtomicBool::new(false);
    let installed = install_vanilla(&version, &root, &cancelled)?;
    eprintln!("Verified {} planned files", installed.downloaded_files);
    extract_natives(&installed.native_archives, &natives_dir)?;
    let paths = VanillaPaths {
        root: root.clone(),
        game_dir,
        natives_dir,
        java,
    };
    let player = VanillaIdentity {
        game_uuid: uuid.clone(),
        username: username.clone(),
        access_token: token,
        user_type: env::var("TLAUCHCRAFT_USER_TYPE").unwrap_or_else(|_| "msa".to_owned()),
        client_id: env::var("TLAUCHCRAFT_CLIENT_ID").unwrap_or_default(),
        xuid: env::var("TLAUCHCRAFT_XUID").unwrap_or_default(),
    };
    let launch_config = build_vanilla_launch(&version, &paths, &player, 512, 4096)?;
    let stdout = evidence.join(format!("client-{native_nonce}-stdout.log"));
    let stderr = evidence.join(format!("client-{native_nonce}-stderr.log"));
    let mut child = launch(&launch_config, &stdout, &stderr)?;
    let pid = child.id();
    write_launch_evidence(
        &evidence.join(format!("launch-{native_nonce}.json")),
        version_id,
        &paths.java,
        pid,
        &stdout,
        &stderr,
    )?;
    println!("Minecraft process spawned: PID={pid}");
    println!("Observe main menu manually; take a screenshot for G3.");
    println!("Create a world, save/quit, fully relaunch and reopen it for G4.");
    println!("Evidence directory: {}", evidence.display());
    let result = child.wait()?;
    println!("Game process exited: {result}");
    println!("Exit alone does not establish G3 or G4.");
    Ok(())
}
