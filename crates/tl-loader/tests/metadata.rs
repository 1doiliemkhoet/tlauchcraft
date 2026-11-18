use tl_loader::{
    fabric::{parse_versions, profile_url, validate_runtime, Fabric},
    maven::{self, Forge, NeoForge},
    LoaderError, LoaderInstaller, LoaderKind, LoaderSpec,
};

fn spec(kind: LoaderKind, game: &str, loader: &str) -> LoaderSpec {
    let _ = game;
    LoaderSpec {
        kind,
        version: Some(loader.into()),
    }
}
fn fabric_fixture() -> &'static str {
    r#"{"id":"fabric-loader-0.16.10-1.21.1","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.16.10","url":"https://maven.fabricmc.net/"},{"name":"org.ow2.asm:asm:9.7","url":"https://repo.maven.apache.org/maven2/"}]}"#
}
#[test]
fn resolves_fabric_plan_without_touching_disk() {
    let p = Fabric
        .plan(
            "1.21.1",
            &spec(LoaderKind::Fabric, "1.21.1", "0.16.10"),
            fabric_fixture(),
            21,
        )
        .unwrap();
    assert_eq!(p.inherits_from, "1.21.1");
    assert_eq!(p.libraries.len(), 2);
    assert_eq!(
        p.libraries[0].relative_path,
        "net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar"
    );
    assert!(p.ready_for_metadata_merge);
    assert!(p.installer.is_none());
    assert!(validate_runtime(&p, 21).is_ok());
    assert_eq!(
        validate_runtime(&p, 17),
        Err(LoaderError::JavaIncompatible {
            required: 21,
            actual: 17
        })
    );
}
#[test]
fn rejects_fabric_version_mismatch() {
    assert_eq!(
        Fabric.plan(
            "1.20.1",
            &spec(LoaderKind::Fabric, "1.20.1", "0.16.10"),
            fabric_fixture(),
            17
        ),
        Err(LoaderError::CompatibilityConflict)
    );
}
#[test]
fn rejects_malicious_paths_and_urls() {
    assert!(profile_url("../1.21", "0.16").is_err());
    assert!(maven::artifact_path("x:y:../../evil").is_err());
    assert!(maven::artifact("http://evil.test", "x:y:1", None).is_err());
    let bad = fabric_fixture().replace("https://maven.fabricmc.net/", "https://evil.example/");
    assert!(Fabric
        .plan(
            "1.21.1",
            &spec(LoaderKind::Fabric, "1.21.1", "0.16.10"),
            &bad,
            21
        )
        .is_err());
}
#[test]
fn parses_fabric_version_catalog() {
    assert_eq!(
        parse_versions(r#"[{"version":"0.16.10"},{"version":"0.16.11"}]"#)
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn forge_installer_is_not_reported_as_installed() {
    let xml = r#"<metadata><versioning><versions><version>1.20.1-47.3.0</version></versions></versioning></metadata>"#;
    let p = Forge
        .plan(
            "1.20.1",
            &spec(LoaderKind::Forge, "1.20.1", "47.3.0"),
            xml,
            17,
        )
        .unwrap();
    assert_eq!(
        p.installer.unwrap().relative_path,
        "net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar"
    );
    assert!(!p.ready_for_metadata_merge);
    assert!(p.main_class.is_none());
}
#[test]
fn forge_rejects_mismatched_versions() {
    let xml = r#"<metadata><versioning><versions><version>1.20.1-47.3.0</version></versions></versioning></metadata>"#;
    assert_eq!(
        Forge.plan(
            "1.21.1",
            &spec(LoaderKind::Forge, "1.21.1", "47.3.0"),
            xml,
            21
        ),
        Err(LoaderError::CompatibilityConflict)
    );
}
#[test]
fn neoforge_only_plans_explicit_candidate_and_never_publishes() {
    let xml = r#"<metadata><versioning><versions><version>21.1.80</version></versions></versioning></metadata>"#;
    let candidate = NeoForge
        .plan(
            "1.21.1",
            &spec(LoaderKind::NeoForge, "1.21.1", "21.1.80"),
            xml,
            21,
        )
        .unwrap();
    assert_eq!(candidate.required_java_major, 21);
    assert!(!candidate.ready_for_metadata_merge);
    assert!(candidate.main_class.is_none());
    assert!(candidate.installer.unwrap().expected_sha1.is_none());
    assert!(matches!(
        NeoForge.plan(
            "1.20.1",
            &spec(LoaderKind::NeoForge, "1.20.1", "21.1.80"),
            xml,
            21,
        ),
        Err(LoaderError::UnsupportedVersion(_))
    ));
}

#[test]
fn cross_generation_forge_plans_have_official_pins_and_java_requirements() {
    for (mc, forge, major, sha) in [
        ("1.12.2", "14.23.5.2860", 8, "5e8a91f71ef3d1f77de3f8d3261aedcc2f551c9d"),
        ("1.18.2", "40.2.21", 17, "5507eebfe3ca62545419e7aa8ee7b5dc559f0e26"),
        ("1.20.1", "47.3.0", 17, "0deb8e547a9e3f098a4c9f082903499703c52f5d"),
    ] {
        let xml = format!("<metadata><versioning><versions><version>{mc}-{forge}</version></versions></versioning></metadata>");
        let plan = Forge.plan(mc, &spec(LoaderKind::Forge, mc, forge), &xml, 8).unwrap();
        assert_eq!(plan.required_java_major, major);
        assert_eq!(plan.installer.unwrap().expected_sha1.as_deref(), Some(sha));
        assert!(!plan.ready_for_metadata_merge);
    }
}
