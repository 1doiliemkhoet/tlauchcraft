//! Windows-only manual G2/G3/G4 harness. No automated menu/world success claim.
use std::{env, error::Error, fs, path::PathBuf, process::Command, sync::atomic::AtomicBool};
use tl_game::{
    launch,
    vanilla::{
        build_vanilla_launch, extract_natives, install_vanilla, resolve_official_version,
        write_launch_evidence, VanillaIdentity, VanillaPaths,
    },
    vanilla_evidence::{write_install_checksums, write_world_snapshot},
};

fn main() -> Result<(), Box<dyn Error>> {
    if !cfg!(windows) {
        return Err("Windows-only E2E harness".into());
    }
    let args: Vec<String> = env::args().collect();
    if args.len() != 5 {
        eprintln!(
            "Usage: vanilla_e2e <official-release-id> <fresh-launcher-root> <java.exe> <offline-username>"
        );
        return Err("expected four CLI arguments".into());
    }
    // Do not accept access tokens in CLI arguments or log files.
    // Local offline profiles do not require a Microsoft login. Do not log tokens.
    let token = env::var("TLAUCHCRAFT_ACCESS_TOKEN").unwrap_or_else(|_| "0".to_owned());
    let default_user_type = if token == "0" { "legacy" } else { "msa" };
    let version_id = &args[1];
    let root = PathBuf::from(&args[2]);
    let java = PathBuf::from(&args[3]);
    let username = &args[4];

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
    eprintln!("Installed {} planned files", installed.downloaded_files);
    let count = write_install_checksums(
        &version,
        &root,
        &evidence.join(format!("install-checksums-{native_nonce}.json")),
    )?;
    eprintln!("Rehashed {count} installed files for G2 evidence");
    eprintln!(
        "[vanilla-native] verified_native_archives={} extraction_target={}",
        installed.native_archives.len(),
        natives_dir.display()
    );
    extract_natives(&installed.native_archives, &natives_dir)?;
    let dll_count = fs::read_dir(&natives_dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        })
        .count();
    eprintln!("[vanilla-native] extracted_dll_count={dll_count}");
    if dll_count == 0 {
        return Err("native archive extraction completed without Windows DLLs".into());
    }
    let paths = VanillaPaths {
        root: root.clone(),
        game_dir,
        natives_dir,
        java,
    };
    let mut player = VanillaIdentity::from_offline_username(username)?;
    // The game UUID always follows tl-core's offline contract, never a CLI supplied
    // arbitrary UUID or the persisted internal profile UUID.
    player.access_token = token;
    player.user_type =
        env::var("TLAUCHCRAFT_USER_TYPE").unwrap_or_else(|_| default_user_type.to_owned());
    player.client_id = env::var("TLAUCHCRAFT_CLIENT_ID").unwrap_or_default();
    player.xuid = env::var("TLAUCHCRAFT_XUID").unwrap_or_default();
    let launch_config = build_vanilla_launch(&version, &paths, &player, 512, 4096)?;
    let stdout = evidence.join(format!("client-{native_nonce}-stdout.log"));
    let stderr = evidence.join(format!("client-{native_nonce}-stderr.log"));
    let java_version = Command::new(&paths.java).arg("-version").output()?;
    let java_detail = format!(
        "{}{}",
        String::from_utf8_lossy(&java_version.stderr),
        String::from_utf8_lossy(&java_version.stdout)
    );
    let windows_version = Command::new("cmd.exe").args(["/C", "ver"]).output()?;
    let windows_detail = String::from_utf8_lossy(&windows_version.stdout)
        .trim()
        .to_owned();
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
    let actual_log = paths.game_dir.join("logs").join("latest.log");
    let runtime_report = serde_json::json!({
        "minecraft_version": version_id,
        "windows_version": windows_detail,
        "java_version": java_detail.trim(),
        "java_major_expected": tl_game::required_java(&version)?,
        "pid": pid,
        "exit_code": result.code(),
        "stdout": stdout,
        "stderr": stderr,
        "game_log_path": actual_log,
        "game_log_present": actual_log.is_file(),
        "main_menu_verified": false,
        "world_reopened_verified": false
    });
    let runtime_path = evidence.join(format!("runtime-{native_nonce}.json"));
    let mut runtime_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&runtime_path)?;
    use std::io::Write;
    runtime_file.write_all(serde_json::to_string_pretty(&runtime_report)?.as_bytes())?;
    runtime_file.sync_all()?;
    println!("Runtime evidence: {}", runtime_path.display());
    let worlds = write_world_snapshot(
        &paths.game_dir,
        &evidence.join(format!("world-snapshot-{native_nonce}.json")),
    )?;
    println!("Captured {worlds} saved world directories (not G4 proof)");
    println!("Exit alone does not establish G3 or G4.");
    Ok(())
}
