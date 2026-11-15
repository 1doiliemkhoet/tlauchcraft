//! Real, sanitized official Forge 36.2.29 installer metadata, extracted by Control
//! from SHA-1 7d247279cf06eed6ffcf609f36d11e34c70350f3.
//! NO Java process, artifact download or user Minecraft world is touched.
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf};
use tl_loader::{
    forge_1165::forge_1165_client_blueprint,
    forge_graph::{prepare_forge_1165, BoundValue, ForgeBindings, PrepareState},
};

const PROFILE: &str = include_str!("fixtures/forge_36229_official_profile.json");
const VERSION: &str = include_str!("fixtures/forge_36229_official_version.json");

fn version() -> Value {
    serde_json::from_str(VERSION).unwrap()
}

fn bindings() -> ForgeBindings {
    let stage_root = std::env::temp_dir().join(format!(
        "forge-real-36229-{}-uncreated-private-stage",
        std::process::id()
    ));
    let mut data = BTreeMap::new();
    for (token, relative) in [
        ("MAPPINGS", "generated/mappings.txt"),
        ("MC_SLIM", "generated/client-slim.jar"),
        ("MC_EXTRA", "generated/client-extra.jar"),
        ("MC_SRG", "generated/client-srg.jar"),
        ("PATCHED", "generated/client-patched.jar"),
    ] {
        data.insert(token.into(), BoundValue::Staged(relative.into()));
    }
    ForgeBindings {
        minecraft: "1.16.5".into(),
        forge_version: "36.2.29".into(),
        java: PathBuf::from("C:/approved/java8/java.exe"),
        stage_root,
        timeout_seconds: 120,
        artifacts: BTreeMap::new(),
        data,
    }
}

#[test]
fn official_graph_exact_order_classpaths_and_outputs() {
    let profile: Value = serde_json::from_str(PROFILE).unwrap();
    let ver = version();
    assert_eq!(profile["libraries"].as_array().unwrap().len(), 39);
    assert_eq!(ver["libraries"].as_array().unwrap().len(), 26);
    assert_eq!(ver["inheritsFrom"], "1.16.5");
    assert_eq!(ver["mainClass"], "cpw.mods.modlauncher.Launcher");
    let graph = forge_1165_client_blueprint(PROFILE, &ver, "36.2.29").unwrap();
    let expected = [
        ("net.minecraftforge:installertools:1.2.6", 17, 0),
        ("net.minecraftforge:jarsplitter:1.1.4", 2, 2),
        ("net.md-5:SpecialSource:1.8.5", 7, 0),
        ("net.minecraftforge:binarypatcher:1.0.12", 11, 1),
    ];
    assert_eq!(graph.steps.len(), 4);
    for (step, (coordinate, cp, hashes)) in graph.steps.iter().zip(expected) {
        assert_eq!(step.jar, coordinate);
        assert_eq!(step.classpath.len(), cp);
        assert_eq!(step.outputs.len(), hashes);
    }
    assert_eq!(profile["data"]["MC_SLIM_SHA"]["client"], "'045d6edac8fe1b004e159c49704b5b729f551079'");
    assert_eq!(profile["data"]["MC_EXTRA_SHA"]["client"], "'4aef4168f94d97bc73bf510213b2518942f931d1'");
    assert_eq!(profile["data"]["PATCHED_SHA"]["client"], "'42710a50971139d0f218e438a211a0dbe9bbb3d2'");
    assert_eq!(profile["data"]["BINPATCH"]["client"], "/data/client.lzma");
    assert!(profile["data"].get("MINECRAFT_JAR").is_none());
    assert!(ver["arguments"]["game"].as_array().unwrap().len() > 4);
}

#[test]
fn real_graph_with_no_verified_binaries_needs_action_not_false_ready() {
    let input = bindings();
    let result = prepare_forge_1165(PROFILE, &version(), &input);
    assert_eq!(result.state, PrepareState::NeedsAction, "{:?}", result.error);
    assert!(result.request.is_none());
    for required in [
        "installertools:1.2.6",
        "jarsplitter:1.1.4",
        "SpecialSource:1.8.5",
        "binarypatcher:1.0.12",
        "mcp_config:1.16.5-20210115.111550@zip",
        "MINECRAFT_JAR",
        "BINPATCH",
    ] {
        assert!(result.unresolved.iter().any(|x| x.contains(required)), "missing NeedsAction for {required}: {:?}", result.unresolved);
    }
    assert!(!input.stage_root.exists());
}

#[test]
fn official_graph_rejects_forward_reference() {
    let mut profile: Value = serde_json::from_str(PROFILE).unwrap();
    let steps = profile["processors"].as_array_mut().unwrap();
    steps.swap(1, 2); // SpecialSource cannot consume MC_SLIM before jarsplitter
    let r = prepare_forge_1165(&profile.to_string(), &version(), &bindings());
    assert_eq!(r.state, PrepareState::Failed, "{:?}", r.unresolved);
    assert!(r.request.is_none());
}

#[test]
fn official_graph_disallows_unapproved_specialsource_version() {
    let mut profile: Value = serde_json::from_str(PROFILE).unwrap();
    profile["processors"][2]["jar"] = json!("net.md-5:SpecialSource:1.8.6");
    let r = prepare_forge_1165(&profile.to_string(), &version(), &bindings());
    assert_eq!(r.state, PrepareState::Failed);
}

#[test]
fn official_graph_rejects_invalid_declared_sha1() {
    let mut profile: Value = serde_json::from_str(PROFILE).unwrap();
    profile["data"]["MC_SLIM_SHA"]["client"] = json!("'fake'");
    let r = prepare_forge_1165(&profile.to_string(), &version(), &bindings());
    assert_eq!(r.state, PrepareState::Failed);
}

#[test]
fn arbitrary_binpatch_path_must_never_pass_as_a_literal() {
    let mut input = bindings();
    input.data.insert("BINPATCH".into(), BoundValue::Literal("../../user/world.dat".into()));
    let result = prepare_forge_1165(PROFILE, &version(), &input);
    assert_eq!(result.state, PrepareState::Failed);
}
