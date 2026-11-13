use std::{fs::File, io::Write, path::Path};
use tempfile::tempdir;
use tl_pack::{inspect_archive, initial_state, safe_path, stage_overrides, ImportState, PackSource};
use zip::{write::SimpleFileOptions, ZipWriter};

fn create_zip(path: &Path, files: &[(&str, &str)]) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    for &(name, contents) in files {
        writer.start_file(name, SimpleFileOptions::default()).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}
#[test]
fn curseforge_requires_authorized_resolution() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("curseforge.zip");
    create_zip(&input, &[("manifest.json", r#"{"name":"Example","minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.3.0","primary":true}]},"files":[{"projectID":3,"fileID":9,"required":true}],"overrides":"overrides"}"#),
      ("overrides/config/a.toml", "safe")]);
    let metadata = inspect_archive(&input).unwrap();
    assert_eq!(metadata.source, PackSource::CurseforgeZip);
    assert_eq!(metadata.files[0].project_id, Some(3));
    assert_eq!(initial_state(&metadata), ImportState::NeedsAction);
    let stage = temp.path().join("new-stage");
    stage_overrides(&input, &metadata, &stage).unwrap();
    assert_eq!(std::fs::read_to_string(stage.join("config/a.toml")).unwrap(), "safe");
}
#[test]
fn modrinth_hashes_and_side_are_parsed() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("pack.mrpack");
    create_zip(&input, &[("modrinth.index.json", r#"{"name":"Test MR","dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.11"},"files":[{"path":"mods/example.jar","hashes":{"sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"downloads":["https://cdn.modrinth.com/data/safe"],"env":{"client":"required","server":"unsupported"}}]}"#),
      ("overrides/config/test.txt", "one")]);
    let data = inspect_archive(&input).unwrap();
    assert_eq!(data.loader.kind, "fabric");
    assert_eq!(data.files[0].sha1.as_ref().unwrap().len(), 40);
    assert_eq!(data.overrides, vec!["config/test.txt"]);
}
#[test]
fn prism_component_inspection() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("prism.zip");
    create_zip(&input, &[("mmc-pack.json", r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.15.11"}]}"#),("instance.cfg","name=Demo"),("minecraft/mods/a.jar","fixture")]);
    let data = inspect_archive(&input).unwrap();
    assert_eq!(data.source, PackSource::PrismZip);
    assert_eq!(data.loader.kind, "fabric");
    assert_eq!(data.overrides, vec!["mods/a.jar"]);
}
#[test]
fn rejects_zip_slip_and_windows_aliases() {
    for s in ["../evil", "/absolute", "C:/windows/a", "mods\\escape.jar", "mods/CON", "mods/../outside", "mods/a."] {
        assert!(safe_path(s).is_err(), "{s}");
    }
}
#[test]
fn rejects_case_insensitive_duplicate_paths() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("duplicate.zip");
    create_zip(&input, &[("manifest.json", "{}"), ("Mods/A.jar", "1"), ("mods/a.jar", "2")]);
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
    create_zip(&input, &[("modrinth.index.json", r#"{"name":"No downloads","dependencies":{"minecraft":"1.20.1"},"files":[]}"#)]);
    let data = inspect_archive(&input).unwrap();
    assert!(stage_overrides(&input, &data, temp.path()).is_err());
}
