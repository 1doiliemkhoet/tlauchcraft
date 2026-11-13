use tl_skin::*;
use uuid::Uuid;

fn image(height: u32) -> SkinImage {
    SkinImage {
        height,
        rgba: vec![255; (64 * height * 4) as usize],
    }
}
#[test]
fn png_64_and_legacy_roundtrip() {
    for h in [32, 64] {
        let source = image(h);
        let png = encode_png(&source).unwrap();
        let decoded = decode_skin(&png).unwrap();
        assert_eq!(decoded.height, h);
        assert_eq!(decoded.rgba, source.rgba);
    }
}
#[test]
fn invalid_dimensions_corruption_and_size_rejected() {
    assert!(decode_skin(b"not a png").is_err());
    assert!(decode_skin(&vec![0; 2_097_153]).is_err());
    let mut bytes = encode_png(&image(64)).unwrap();
    bytes[16] = 1; // invalid IHDR width (and CRC)
    assert!(decode_skin(&bytes).is_err());
}
#[test]
fn library_roundtrip_persistence_and_instance_override() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = Uuid::new_v4();
    let instance = Uuid::new_v4();
    let (original, override_id);
    {
        let mut db = SkinLibrary::open(tmp.path()).unwrap();
        let a = db
            .import_png("First", &encode_png(&image(64)).unwrap(), Model::Classic)
            .unwrap();
        original = a.id;
        let b = db.duplicate(a.id, "Second").unwrap();
        override_id = b.id;
        db.assign_profile(SkinSelection {
            profile_id: profile,
            skin_id: a.id,
            model: Model::Classic,
            cape_id: None,
        })
        .unwrap();
        db.assign_instance_override(
            instance,
            SkinSelection {
                profile_id: profile,
                skin_id: b.id,
                model: Model::Slim,
                cape_id: None,
            },
        )
        .unwrap();
        assert_eq!(db.selection(profile, Some(instance)).unwrap().skin_id, b.id);
    }
    let mut db = SkinLibrary::open(tmp.path()).unwrap();
    assert_eq!(db.selection(profile, None).unwrap().skin_id, original);
    assert_eq!(
        db.selection(profile, Some(instance)).unwrap().model,
        Model::Slim
    );
    assert_eq!(
        decode_skin(&db.export_png(original).unwrap())
            .unwrap()
            .height,
        64
    );
    assert!(db.delete_confirmed(original, false).is_err());
    assert!(db.export_png(original).is_ok());
    db.delete_confirmed(original, true).unwrap();
    assert_eq!(db.selection(profile, None), None);
    assert_eq!(
        db.selection(profile, Some(instance)).unwrap().skin_id,
        override_id
    );
}
#[test]
fn pixel_editor_undo_redo_and_bounds() {
    let mut editor = SkinEditor::new(image(64));
    assert!(editor.paint(64, 0, [0, 0, 0, 0]).is_err());
    editor.paint(3, 10, [10, 20, 30, 40]).unwrap();
    assert_eq!(editor.sample(3, 10).unwrap(), [10, 20, 30, 40]);
    assert!(editor.undo());
    assert_eq!(editor.sample(3, 10).unwrap(), [255; 4]);
    assert!(editor.redo());
    assert_eq!(editor.sample(3, 10).unwrap(), [10, 20, 30, 40]);
}
#[test]
fn preview_does_not_equal_real_visibility_and_opt_in_is_enforced() {
    let image = image(64);
    assert_eq!(lan_share_payload(false, &image).unwrap(), None);
    assert!(lan_share_payload(true, &image).unwrap().is_some());
    let mut s = AppearanceStatus::default();
    assert_eq!(s.local, Visibility::PreviewOnly);
    assert!(s.verify_local("screenshot".into()).is_err());
    s.provider_prepared(&ProviderCapabilities {
        provider_id: "manual-test".into(),
        minecraft_version: "test".into(),
        loader: "Fabric".into(),
        local_skin: true,
        peer_skin: true,
        custom_cape: false,
    });
    assert!(s.verify_peer("a".into(), "b".into()).is_err());
    s.verify_local("real-local-evidence".into()).unwrap();
    s.verify_peer("real-local-evidence".into(), "real-friend-evidence".into())
        .unwrap();
    assert_eq!(s.peer, Visibility::PeerVerified);
}

#[test]
fn fill_mirror_palette_and_undo() {
    let mut e = SkinEditor::new(image(64));
    assert_eq!(e.palette(0).len(), 0);
    assert_eq!(e.flood_fill(0, 0, [5, 6, 7, 8]).unwrap(), 4096);
    assert_eq!(e.sample(63, 63).unwrap(), [5, 6, 7, 8]);
    assert!(e.undo());
    assert_eq!(e.sample(63, 63).unwrap(), [255; 4]);
    e.paint(0, 0, [1, 2, 3, 4]).unwrap();
    e.paint(1, 0, [7, 8, 9, 10]).unwrap();
    e.mirror_region(0, 0, 2, 1).unwrap();
    assert_eq!(e.sample(0, 0).unwrap(), [7, 8, 9, 10]);
    assert!(e.mirror_region(63, 63, 2, 2).is_err());
    assert_eq!(atlas_regions().len(), 12);
}

#[test]
fn foundation_identity_and_skin_selection_stay_aligned() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SkinLibrary::open(dir.path()).unwrap();
    let mut profile = tl_core::OfflineProfile::new("SkinPlayer").unwrap();
    let stable_id = profile.id;
    assert!(db.validate_foundation_profile(&profile).is_ok());
    let skin = db
        .import_png("Local", &encode_png(&image(64)).unwrap(), Model::Slim)
        .unwrap();
    db.assign_foundation_profile(&mut profile, skin.id, Model::Slim)
        .unwrap();
    assert_eq!(profile.id, stable_id);
    assert_eq!(profile.skin_id, Some(skin.id));
    assert_eq!(db.selection(profile.id, None).unwrap().model, Model::Slim);
    assert!(db.validate_foundation_profile(&profile).is_ok());
    let mut stale = profile.clone();
    stale.skin_id = None;
    assert!(db.validate_foundation_profile(&stale).is_err());
    let reopened = SkinLibrary::open(dir.path()).unwrap();
    assert_eq!(
        reopened.selection(stable_id, None).unwrap().skin_id,
        skin.id
    );
}
