use std::{fs::File, io::Write, path::Path};
use tempfile::tempdir;
use tl_pack::{
    initial_state, inspect_archive, safe_path, stage_overrides, ImportState, PackSource,
};
use zip::{write::SimpleFileOptions, ZipWriter};

fn create_zip(path: &Path, files: &[(&str, &str)]) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    for &(name, contents) in files {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}
#[test]
fn curseforge_requires_authorized_resolution() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("curseforge.zip");
    create_zip(
        &input,
        &[
            (
                "manifest.json",
                r#"{"name":"Example","minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.3.0","primary":true}]},"files":[{"projectID":3,"fileID":9,"required":true}],"overrides":"overrides"}"#,
            ),
            ("overrides/config/a.toml", "safe"),
        ],
    );
    let metadata = inspect_archive(&input).unwrap();
    assert_eq!(metadata.source, PackSource::CurseforgeZip);
    assert_eq!(metadata.loader.kind, "forge");
    assert_eq!(metadata.loader.version.as_deref(), Some("47.3.0"));
    assert_eq!(metadata.files[0].project_id, Some(3));
    assert_eq!(initial_state(&metadata), ImportState::NeedsAction);
    let stage = temp.path().join("new-stage");
    stage_overrides(&input, &metadata, &stage).unwrap();
    assert_eq!(
        std::fs::read_to_string(stage.join("config/a.toml")).unwrap(),
        "safe"
    );
}
#[test]
fn curseforge_rejects_same_project_with_two_file_versions() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("duplicate-project.zip");
    create_zip(
        &path,
        &[(
            "manifest.json",
            r#"{"name":"Bad","minecraft":{"version":"1.16.5","modLoaders":[{"id":"forge-36.2.34","primary":true}]},"files":[{"projectID":101,"fileID":201},{"projectID":101,"fileID":202}]}"#,
        )],
    );
    assert!(inspect_archive(&path).is_err());
}

#[test]
fn modrinth_hashes_and_side_are_parsed() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("pack.mrpack");
    create_zip(
        &input,
        &[
            (
                "modrinth.index.json",
                r#"{"name":"Test MR","dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.11"},"files":[{"path":"mods/example.jar","hashes":{"sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"downloads":["https://cdn.modrinth.com/data/safe"],"env":{"client":"required","server":"unsupported"}}]}"#,
            ),
            ("overrides/config/test.txt", "one"),
        ],
    );
    let data = inspect_archive(&input).unwrap();
    assert_eq!(data.loader.kind, "fabric");
    assert_eq!(data.files[0].sha1.as_ref().unwrap().len(), 40);
    assert_eq!(data.overrides, vec!["config/test.txt"]);
}
#[test]
fn prism_component_inspection() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("prism.zip");
    create_zip(
        &input,
        &[
            (
                "mmc-pack.json",
                r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.15.11"}]}"#,
            ),
            ("instance.cfg", "name=Demo"),
            ("minecraft/mods/a.jar", "fixture"),
        ],
    );
    let data = inspect_archive(&input).unwrap();
    assert_eq!(data.source, PackSource::PrismZip);
    assert_eq!(data.loader.kind, "fabric");
    assert_eq!(data.overrides, vec!["mods/a.jar"]);
}
#[test]
fn rejects_zip_slip_and_windows_aliases() {
    for s in [
        "../evil",
        "/absolute",
        "C:/windows/a",
        "mods\\escape.jar",
        "mods/CON",
        "mods/../outside",
        "mods/a.",
    ] {
        assert!(safe_path(s).is_err(), "{s}");
    }
}
#[test]
fn rejects_case_insensitive_duplicate_paths() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("duplicate.zip");
    create_zip(
        &input,
        &[
            ("manifest.json", "{}"),
            ("Mods/A.jar", "1"),
            ("mods/a.jar", "2"),
        ],
    );
    assert!(inspect_archive(&input).is_err());
}
#[test]
fn rejects_generic_archives() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("generic.zip");
    create_zip(&input, &[("mods/a.jar", "fixture")]);
    assert!(inspect_archive(&input).is_err());
}
#[test]
fn staging_refuses_preexisting_directory() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("modrinth.mrpack");
    create_zip(
        &input,
        &[(
            "modrinth.index.json",
            r#"{"name":"No downloads","dependencies":{"minecraft":"1.20.1"},"files":[]}"#,
        )],
    );
    let data = inspect_archive(&input).unwrap();
    assert!(stage_overrides(&input, &data, temp.path()).is_err());
}

#[test]
fn modrinth_client_overrides_win_and_stay_isolated() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("client.mrpack");
    create_zip(
        &input,
        &[
            (
                "modrinth.index.json",
                r#"{"name":"Client pack","dependencies":{"minecraft":"1.20.1"},"files":[]}"#,
            ),
            ("overrides/config/choice.txt", "shared"),
            ("client-overrides/config/choice.txt", "client"),
            ("client-overrides/mods/local.jar", "fixture-only"),
        ],
    );
    let data = inspect_archive(&input).unwrap();
    assert_eq!(data.source, PackSource::ModrinthMrpack);
    assert_eq!(data.overrides.len(), 3);
    let stage = temp.path().join("instance-staging");
    stage_overrides(&input, &data, &stage).unwrap();
    assert_eq!(
        std::fs::read_to_string(stage.join("config/choice.txt")).unwrap(),
        "client"
    );
    assert_eq!(
        std::fs::read_to_string(stage.join("mods/local.jar")).unwrap(),
        "fixture-only"
    );
    assert!(!temp.path().join("mods/local.jar").exists());
}

#[test]
fn rejects_corrupted_archive_without_staging() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("corrupt.zip");
    std::fs::write(&input, b"not a zip").unwrap();
    assert!(inspect_archive(&input).is_err());
    assert!(!temp.path().join("stage").exists());
}

#[test]
fn rejects_zip_bomb_by_compression_ratio() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("bomb.zip");
    let mut writer = ZipWriter::new(File::create(&input).unwrap());
    writer
        .start_file("manifest.json", SimpleFileOptions::default())
        .unwrap();
    writer
        .write_all(
            br#"{"name":"Bomb","minecraft":{"version":"1.20.1","modLoaders":[]},"files":[]}"#,
        )
        .unwrap();
    writer
        .start_file(
            "overrides/config/huge.txt",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
    writer.write_all(&vec![b'A'; 2 * 1024 * 1024]).unwrap();
    writer.finish().unwrap();
    assert!(inspect_archive(&input).is_err());
}

#[test]
fn changed_archive_cannot_be_staged() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("modrinth.mrpack");
    create_zip(
        &input,
        &[(
            "modrinth.index.json",
            r#"{"name":"Initial","dependencies":{"minecraft":"1.20.1"},"files":[]}"#,
        )],
    );
    let inspected = inspect_archive(&input).unwrap();
    create_zip(
        &input,
        &[(
            "modrinth.index.json",
            r#"{"name":"Changed","dependencies":{"minecraft":"1.20.1"},"files":[]}"#,
        )],
    );
    let stage = temp.path().join("stage");
    assert!(stage_overrides(&input, &inspected, &stage).is_err());
    assert!(!stage.exists());
}

#[test]
fn rollback_removes_partial_staging_on_unsupported_prism_patch() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("prism.zip");
    create_zip(
        &input,
        &[
            (
                "mmc-pack.json",
                r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"}]}"#,
            ),
            ("instance.cfg", "name=Patch"),
            ("minecraft/config/safe.txt", "safe"),
            ("minecraft/patches/legacy.json", "{}"),
        ],
    );
    let inspected = inspect_archive(&input).unwrap();
    let stage = temp.path().join("stage");
    assert!(stage_overrides(&input, &inspected, &stage).is_err());
    assert!(!stage.exists(), "failure must not publish partial instance");
}

#[test]
fn rejects_unsafe_case_folded_duplicate_and_reserved_filename() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("reserved.zip");
    create_zip(
        &input,
        &[("manifest.json", "{}"), ("overrides/config/AUX.txt", "bad")],
    );
    assert!(inspect_archive(&input).is_err());
}
