use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use tl_core::{GameInstance, LoaderKind, LoaderSpec, OfflineProfile};
use tl_skin::{
    encode_png,
    provider::{
        predicted_game_uuid, CustomSkinLoaderLocal, ProviderContext, ProviderMod, ProviderReason,
        SkinProvider,
    },
    Model, SkinImage, SkinLibrary, Visibility,
};

fn png() -> Vec<u8> {
    encode_png(&SkinImage {
        height: 64,
        rgba: vec![255; 64 * 64 * 4],
    })
    .unwrap()
}
fn fabric(profile: &OfflineProfile) -> GameInstance {
    GameInstance::new(
        "Skin Test",
        "1.21.1",
        LoaderSpec {
            kind: LoaderKind::Fabric,
            version: Some("0.16.10".into()),
        },
        profile.id,
    )
    .unwrap()
}
fn vanilla(profile: &OfflineProfile) -> GameInstance {
    GameInstance::new(
        "Vanilla",
        "1.21.1",
        LoaderSpec {
            kind: LoaderKind::Vanilla,
            version: None,
        },
        profile.id,
    )
    .unwrap()
}
fn mod_jar(dir: &Path) -> String {
    fs::create_dir_all(dir.join("mods")).unwrap();
    let bytes = b"PK\x03\x04verified-fixture-only-not-a-real-mod";
    fs::write(dir.join("mods").join("CustomSkinLoader-test.jar"), bytes).unwrap();
    format!("{:x}", Sha256::digest(bytes))
}
fn ctx<'a>(
    profile: &'a OfflineProfile,
    instance: &'a GameInstance,
    dir: &'a Path,
    consent: bool,
    hash: Option<&'a str>,
) -> ProviderContext<'a> {
    ProviderContext {
        profile,
        instance,
        game_dir: dir,
        consent,
        client_mod: hash.map(|h| ProviderMod {
            mod_id: "customskinloader",
            jar_filename: "CustomSkinLoader-test.jar",
            expected_sha256: h,
        }),
    }
}

#[test]
fn vanilla_fails_safe_even_with_a_local_mod_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = SkinLibrary::open(dir.path().join("catalog")).unwrap();
    let mut p = OfflineProfile::new("LocalPlayer").unwrap();
    let id = library
        .import_png("Classic", &png(), Model::Classic)
        .unwrap()
        .id;
    library
        .assign_foundation_profile(&mut p, id, Model::Classic)
        .unwrap();
    let hash = mod_jar(dir.path());
    let instance = vanilla(&p);
    let context = ctx(&p, &instance, dir.path(), true, Some(&hash));
    let state = CustomSkinLoaderLocal.stage(&library, &context).unwrap();
    assert_eq!(state.status, Visibility::PreviewOnly);
    assert_eq!(state.reason, ProviderReason::UnsupportedVanilla);
    assert!(!dir.path().join("CustomSkinLoader").exists());
}

#[test]
fn missing_mod_consent_and_wrong_digest_remain_preview_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = SkinLibrary::open(dir.path().join("catalog")).unwrap();
    let mut p = OfflineProfile::new("SkinTester").unwrap();
    let id = library
        .import_png("Classic", &png(), Model::Classic)
        .unwrap()
        .id;
    library
        .assign_foundation_profile(&mut p, id, Model::Classic)
        .unwrap();
    let i = fabric(&p);
    let provider = CustomSkinLoaderLocal;
    assert_eq!(
        provider
            .check(&library, &ctx(&p, &i, dir.path(), false, None))
            .reason,
        ProviderReason::ConsentRequired
    );
    assert_eq!(
        provider
            .check(&library, &ctx(&p, &i, dir.path(), true, None))
            .reason,
        ProviderReason::ModMissingOrUntrusted
    );
    let hash = mod_jar(dir.path());
    assert_eq!(
        provider
            .check(
                &library,
                &ctx(&p, &i, dir.path(), true, Some(&"0".repeat(64)))
            )
            .reason,
        ProviderReason::ModMissingOrUntrusted
    );
    assert_eq!(
        provider
            .check(&library, &ctx(&p, &i, dir.path(), true, Some(&hash)))
            .status,
        Visibility::ProviderReadyUnverified
    );
    assert!(!dir.path().join("CustomSkinLoader").exists());
}

#[test]
fn stage_then_restart_is_idempotent_and_external_edits_are_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let library_dir = dir.path().join("library");
    let mut library = SkinLibrary::open(&library_dir).unwrap();
    let mut p = OfflineProfile::new("SkinTester").unwrap();
    let id = library
        .import_png("Skin", &png(), Model::Classic)
        .unwrap()
        .id;
    library
        .assign_foundation_profile(&mut p, id, Model::Classic)
        .unwrap();
    let i = fabric(&p);
    let hash = mod_jar(dir.path());
    let request = ctx(&p, &i, dir.path(), true, Some(&hash));
    let adapter = CustomSkinLoaderLocal;
    let first = adapter.stage(&library, &request).unwrap();
    assert_eq!(first.status, Visibility::ProviderReadyUnverified);
    assert_eq!(first.model, Some(Model::Classic));
    let staged = first.target.unwrap();
    assert!(staged.ends_with("CustomSkinLoader/LocalSkin/skins/SkinTester.png"));
    assert_eq!(fs::read(&staged).unwrap(), library.export_png(id).unwrap());
    assert_eq!(
        adapter.stage(&library, &request).unwrap().status,
        Visibility::ProviderReadyUnverified
    );
    drop(library);
    let reopened = SkinLibrary::open(&library_dir).unwrap();
    assert_eq!(
        adapter.stage(&reopened, &request).unwrap().status,
        Visibility::ProviderReadyUnverified
    );
    fs::write(&staged, b"external-edit").unwrap();
    let conflict = adapter.check(&reopened, &request);
    assert_eq!(conflict.reason, ProviderReason::AssetConflict);
    assert_eq!(conflict.status, Visibility::PreviewOnly);
    assert!(adapter.stage(&reopened, &request).unwrap().target.is_none());
    assert_eq!(fs::read(&staged).unwrap(), b"external-edit");
}

#[test]
fn slim_override_and_mismatched_instance_never_stage() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = SkinLibrary::open(dir.path().join("catalog")).unwrap();
    let mut p = OfflineProfile::new("SkinTester").unwrap();
    let id = library
        .import_png("Skin", &png(), Model::Classic)
        .unwrap()
        .id;
    library
        .assign_foundation_profile(&mut p, id, Model::Classic)
        .unwrap();
    let instance = fabric(&p);
    library
        .assign_instance_override(
            instance.id,
            tl_core::SkinSelection {
                profile_id: p.id,
                skin_id: id,
                model: Model::Slim,
                cape_id: None,
            },
        )
        .unwrap();
    let hash = mod_jar(dir.path());
    let provider = CustomSkinLoaderLocal;
    assert_eq!(
        provider
            .check(&library, &ctx(&p, &instance, dir.path(), true, Some(&hash)))
            .reason,
        ProviderReason::SlimModelNotConfirmed
    );
    let mut foreign = fabric(&p);
    foreign.profile_id = uuid::Uuid::new_v4();
    assert_eq!(
        provider
            .check(&library, &ctx(&p, &foreign, dir.path(), true, Some(&hash)))
            .reason,
        ProviderReason::ProfileOrInstanceMismatch
    );
    assert!(!dir.path().join("CustomSkinLoader").exists());
}

#[test]
fn profile_switch_distinct_usernames_and_managed_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = SkinLibrary::open(dir.path().join("library")).unwrap();
    let hash = mod_jar(dir.path());
    let adapter = CustomSkinLoaderLocal;
    for username in ["FirstUser", "SecondUser"] {
        let mut profile = OfflineProfile::new(username).unwrap();
        let skin = library
            .import_png(username, &png(), Model::Classic)
            .unwrap();
        library
            .assign_foundation_profile(&mut profile, skin.id, Model::Classic)
            .unwrap();
        let instance = fabric(&profile);
        let request = ctx(&profile, &instance, dir.path(), true, Some(&hash));
        let result = adapter.stage(&library, &request).unwrap();
        assert_eq!(result.status, Visibility::ProviderReadyUnverified);
        assert!(result.target.unwrap().exists());
    }
    assert!(dir
        .path()
        .join("CustomSkinLoader/LocalSkin/skins/FirstUser.png")
        .exists());
    assert!(dir
        .path()
        .join("CustomSkinLoader/LocalSkin/skins/SecondUser.png")
        .exists());
    assert_ne!(
        predicted_game_uuid(&OfflineProfile::new("FirstUser").unwrap()).unwrap(),
        predicted_game_uuid(&OfflineProfile::new("SecondUser").unwrap()).unwrap()
    );
}

#[test]
fn unmanaged_target_file_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = SkinLibrary::open(dir.path().join("library")).unwrap();
    let mut p = OfflineProfile::new("SkinTester").unwrap();
    let skin = library.import_png("Test", &png(), Model::Classic).unwrap();
    library
        .assign_foundation_profile(&mut p, skin.id, Model::Classic)
        .unwrap();
    let hash = mod_jar(dir.path());
    let path = dir.path().join("CustomSkinLoader/LocalSkin/skins");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SkinTester.png"), b"user-owned").unwrap();
    let instance = fabric(&p);
    let request = ctx(&p, &instance, dir.path(), true, Some(&hash));
    assert_eq!(
        CustomSkinLoaderLocal.check(&library, &request).reason,
        ProviderReason::AssetConflict
    );
    assert_eq!(
        CustomSkinLoaderLocal
            .stage(&library, &request)
            .unwrap()
            .status,
        Visibility::PreviewOnly
    );
    assert_eq!(
        fs::read(path.join("SkinTester.png")).unwrap(),
        b"user-owned"
    );
}
