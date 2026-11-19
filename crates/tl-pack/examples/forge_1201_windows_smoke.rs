//! Disposable Windows CI smoke: actual reviewed Forge processors, verified
//! private assembly and launch-command preparation. Never starts Minecraft.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, path::PathBuf, sync::atomic::AtomicBool};
    use tl_game::vanilla::{build_vanilla_launch, VanillaIdentity, VanillaPaths};
    use tl_pack::forge_official::prepare_official_windows_1165;

    let args = env::args().collect::<Vec<_>>();
    if args.len() != 4 || args[3] != "--approve-official-processors" {
        return Err(
            "usage: forge_1201_windows_smoke <root> <java.exe> --approve-official-processors".into(),
        );
    }
    let root = PathBuf::from(&args[1]);
    let java = PathBuf::from(&args[2]);
    if !root.is_absolute() || root.exists() || !java.is_absolute() {
        return Err("fresh absolute smoke root and absolute Java executable required".into());
    }
    fs::create_dir(&root)?;
    let cancel = AtomicBool::new(false);
    let prepared = prepare_official_windows_1165(
        &"a".repeat(64),
        "1.20.1",
        "47.4.0",
        "ci-forge-1201-smoke",
        &root,
        &java,
        true,
        &cancel,
    )?;
    let evidence = prepared.approved();
    let runtime = tl_loader::forge_playable::verify_forge_runtime_for_pack(
        evidence.selection,
        evidence.profile_json,
        evidence.derived,
        evidence.mojang_parent,
        evidence.processor_execution.request(),
        evidence.processor_execution.report(),
        evidence.java,
        evidence.maven,
        evidence.installer_or_generated,
    )?;
    assert_eq!(runtime.processor_count, 6);
    assert_eq!(runtime.required_java_major, 17);
    assert!(runtime.execution_verified);
    assert!(!runtime.instance_ready);
    assert!(!runtime.game_launched);
    println!("REAL_PROCESSORS_VERIFIED count=6 required_java=17");
    let stage = root.join("smoke.staging");
    fs::create_dir(&stage)?;
    let assembly = tl_pack::forge_publish::assemble_from_approved_execution(
        &stage,
        &root.join("cache").join("mojang"),
        &evidence,
        &cancel,
    )?;
    println!("PRIVATE_ASSEMBLY_VERIFIED {assembly:?}");
    let paths = VanillaPaths {
        root: stage.clone(),
        game_dir: stage.clone(),
        natives_dir: stage.join("natives"),
        java: evidence.java.executable.clone(),
    };
    let identity = VanillaIdentity::from_offline_username("SmokePlayer")?;
    let launch = build_vanilla_launch(&runtime.java8_launch_version, &paths, &identity, 512, 2048)?;
    assert_eq!(launch.main_class, "cpw.mods.bootstraplauncher.BootstrapLauncher");
    let modules = launch
        .jvm_args
        .windows(2)
        .find(|pair| pair[0] == "-p")
        .ok_or("missing Forge module path")?;
    assert!(std::env::split_paths(&modules[1]).all(|path| path.is_file()));
    assert!(launch.jvm_args.iter().any(|arg| arg == "--add-modules"));
    assert!(launch
        .game_args
        .windows(2)
        .any(|pair| pair[0] == "--launchTarget" && pair[1] == "forgeclient"));
    let command = tl_game::build_command(&launch)?;
    assert!(!command
        .get_args()
        .any(|arg| arg.to_string_lossy().contains("${")));
    println!("FORGE_1201_LAUNCH_COMMAND_VERIFIED_GAME_NOT_STARTED");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Forge 1.20.1 smoke requires Windows");
    std::process::exit(1);
}
