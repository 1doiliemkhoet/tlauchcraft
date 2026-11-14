use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use tl_loader::{
    forge_graph::{prepare_forge_1165, BoundValue, ForgeBindings, PrepareState},
    runtime::VerifiedProcessorJar,
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
const PROFILE: &str = include_str!("fixtures/forge_1165_representative.json");
const SHA: &str = "51cff3c1f0bc59f6187e7040cc12a4e9b1eca7aa";
fn root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "tl03-forge-graph-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ))
}
fn version() -> serde_json::Value {
    json!({"id":"1.16.5-forge-36.2.34","inheritsFrom":"1.16.5","mainClass":"cpw.mods.modlauncher.Launcher"})
}
fn artifact(root: &std::path::Path, coordinate: &str) -> VerifiedProcessorJar {
    let path = root.join(format!(
        "input-{}.jar",
        coordinate.split(':').nth(1).unwrap()
    ));
    fs::write(&path, b"fixture").unwrap();
    let relative = tl_loader::maven::artifact_path(coordinate).unwrap();
    VerifiedProcessorJar {
        coordinate: coordinate.to_string(),
        path,
        sha1: SHA.into(),
        source: format!("https://maven.minecraftforge.net/{relative}"),
    }
}
fn setup() -> (PathBuf, ForgeBindings) {
    let root = root();
    fs::create_dir(&root).unwrap();
    let mut artifacts = BTreeMap::new();
    for coordinate in [
        "net.minecraftforge:installertools:1.2.0",
        "net.minecraftforge:jarsplitter:1.1.4",
    ] {
        artifacts.insert(coordinate.into(), artifact(&root, coordinate));
    }
    let mut data = BTreeMap::new();
    data.insert(
        "PATCHED".into(),
        BoundValue::Staged("generated/patched.jar".into()),
    );
    data.insert(
        "MC_SLIM".into(),
        BoundValue::Staged("generated/slim.jar".into()),
    );
    data.insert(
        "MC_EXTRA".into(),
        BoundValue::Staged("generated/extra.jar".into()),
    );
    let mut client = artifact(&root, "net.minecraft:client:1.16.5");
    client.source = format!("https://piston-data.mojang.com/v1/objects/{SHA}/client.jar");
    data.insert("MINECRAFT_JAR".into(), BoundValue::VerifiedExternal(client));
    let mcp_coord = "net.minecraftforge:mcp_config:1.16.5-20210115.111550@zip";
    let mcp_path = root.join("mcp-config.zip");
    fs::write(&mcp_path, b"fixture").unwrap();
    data.insert(format!("[{mcp_coord}]"), BoundValue::VerifiedExternal(
        VerifiedProcessorJar {
            coordinate: mcp_coord.into(),
            path: mcp_path,
            sha1: SHA.into(),
            source: "https://maven.minecraftforge.net/net/minecraftforge/mcp_config/1.16.5-20210115.111550/mcp_config-1.16.5-20210115.111550.zip".into(),
        },
    ));
    (
        root.clone(),
        ForgeBindings {
            minecraft: "1.16.5".into(),
            forge_version: "36.2.34".into(),
            java: root.join("java.exe"),
            stage_root: root.join("new-private-stage"),
            timeout_seconds: 30,
            artifacts,
            data,
        },
    )
}
#[test]
fn prepares_ordered_client_only_graph_without_java_execution() {
    let (root, bindings) = setup();
    let report = prepare_forge_1165(PROFILE, &version(), &bindings);
    assert_eq!(report.state, PrepareState::Verified, "{:?}", report.error);
    let request = report.request.unwrap();
    assert_eq!(request.processors.len(), 2);
    assert_eq!(
        request.processors[0].main_class,
        "net.minecraftforge.installertools.ConsoleTool"
    );
    assert_eq!(
        request.processors[1].main_class,
        "net.minecraftforge.jarsplitter.ConsoleTool"
    );
    assert_eq!(
        request.processors[0].outputs[0].relative_path,
        "generated/patched.jar"
    );
    assert!(!bindings.stage_root.exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn absent_data_binding_returns_needs_action() {
    let (root, mut bindings) = setup();
    bindings.data.remove("MC_EXTRA");
    let report = prepare_forge_1165(PROFILE, &version(), &bindings);
    assert_eq!(report.state, PrepareState::NeedsAction);
    assert!(report.unresolved.iter().any(|x| x.contains("MC_EXTRA")));
    assert!(report.request.is_none());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn checksum_mismatch_refuses_graph() {
    let (root, mut bindings) = setup();
    bindings
        .artifacts
        .get_mut("net.minecraftforge:installertools:1.2.0")
        .unwrap()
        .sha1 = "0000000000000000000000000000000000000000".into();
    let report = prepare_forge_1165(PROFILE, &version(), &bindings);
    // The runtime still refuses invalid JARs before spawn. Graph preparation must
    // also return explicit FAILED rather than report a ready request.
    assert_eq!(report.state, PrepareState::Failed);
    assert!(report.request.is_none());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn unsupported_profile_fails_closed() {
    let (root, bindings) = setup();
    let profile = PROFILE.replace("\"minecraft\": \"1.16.5\"", "\"minecraft\": \"1.19.2\"");
    assert_eq!(
        prepare_forge_1165(&profile, &version(), &bindings).state,
        PrepareState::Failed
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn unapproved_tool_never_gets_arbitrary_main_class() {
    let (root, bindings) = setup();
    let profile = PROFILE.replace(
        "net.minecraftforge:installertools:1.2.0",
        "org.evil:processor:1.0",
    );
    assert_eq!(
        prepare_forge_1165(&profile, &version(), &bindings).state,
        PrepareState::Failed
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn traversal_in_staged_path_refused() {
    let (root, mut bindings) = setup();
    bindings.data.insert(
        "PATCHED".into(),
        BoundValue::Staged("../world/level.dat".into()),
    );
    assert_eq!(
        prepare_forge_1165(PROFILE, &version(), &bindings).state,
        PrepareState::Failed
    );
    assert!(!root.join("world").exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn output_hash_mismatch_is_not_ready() {
    let (root, bindings) = setup();
    let profile = PROFILE.replace(SHA, "123");
    assert_eq!(
        prepare_forge_1165(&profile, &version(), &bindings).state,
        PrepareState::Failed
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn existing_stage_is_rejected_without_mutation() {
    let (root, bindings) = setup();
    fs::create_dir(&bindings.stage_root).unwrap();
    fs::write(bindings.stage_root.join("world.dat"), b"stay").unwrap();
    assert_eq!(
        prepare_forge_1165(PROFILE, &version(), &bindings).state,
        PrepareState::Failed
    );
    assert_eq!(
        fs::read(bindings.stage_root.join("world.dat")).unwrap(),
        b"stay"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resolves_profile_maven_reference_from_verified_artifact_map() {
    let (root, mut bindings) = setup();
    let coordinate = "net.minecraftforge:mcp_config:1.16.5-20210115.111550@zip";
    let key = format!("[{coordinate}]");
    let external = match bindings.data.remove(&key).unwrap() {
        BoundValue::VerifiedExternal(value) => value,
        _ => panic!("fixture must pin Maven ZIP input"),
    };
    bindings.artifacts.insert(coordinate.into(), external);
    let result = prepare_forge_1165(PROFILE, &version(), &bindings);
    assert_eq!(result.state, PrepareState::Verified, "{:?}", result.error);
    let graph = result.request.unwrap();
    let path = root.join("mcp-config.zip");
    assert!(graph.processors[0]
        .arguments
        .iter()
        .any(|arg| arg == path.to_str().unwrap()));
    assert!(!bindings.stage_root.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_maven_reference_to_wrong_artifact_hash() {
    let (root, mut bindings) = setup();
    let coordinate = "net.minecraftforge:mcp_config:1.16.5-20210115.111550@zip";
    let key = format!("[{coordinate}]");
    let external = match bindings.data.remove(&key).unwrap() {
        BoundValue::VerifiedExternal(mut value) => {
            value.sha1 = "0000000000000000000000000000000000000000".into();
            value
        }
        _ => panic!("fixture must pin Maven ZIP input"),
    };
    bindings.artifacts.insert(coordinate.into(), external);
    let result = prepare_forge_1165(PROFILE, &version(), &bindings);
    assert_eq!(result.state, PrepareState::Failed);
    assert!(result.request.is_none());
    fs::remove_dir_all(root).unwrap();
}
