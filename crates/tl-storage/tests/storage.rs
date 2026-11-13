use tl_core::{CoreError, GameInstance, InstanceState, LoaderKind, LoaderSpec, OfflineProfile};
use tl_storage::{InstanceStore, ProfileStore, SqliteStore};
#[test]
fn profile_and_instance_persist_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(dir.path()).unwrap();
    let mut profile = OfflineProfile::new("UserOne").unwrap();
    store.create_profile(&profile).unwrap();
    let mut instance = GameInstance::new(
        "Testing",
        "1.21.1",
        LoaderSpec {
            kind: LoaderKind::Fabric,
            version: Some("0.16.10".into()),
        },
        profile.id,
    )
    .unwrap();
    store.create_instance(&instance).unwrap();
    instance.state = InstanceState::Ready;
    store.update_instance(&instance).unwrap();
    profile.rename("UserTwo").unwrap();
    store.update_profile(&profile).unwrap();
    drop(store);
    let store = SqliteStore::open(dir.path()).unwrap();
    assert_eq!(store.get_profile(profile.id).unwrap(), Some(profile));
    assert_eq!(store.get_instance(instance.id).unwrap(), Some(instance));
    assert_eq!(store.schema_version().unwrap(), 2);
}
#[test]
fn username_unique_case_insensitive_and_reference_restricted() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SqliteStore::open(dir.path()).unwrap();
    let p = OfflineProfile::new("Steve").unwrap();
    db.create_profile(&p).unwrap();
    assert_eq!(
        db.create_profile(&OfflineProfile::new("steve").unwrap()),
        Err(CoreError::AlreadyExists)
    );
    let i = GameInstance::new(
        "Vanilla",
        "1.20.1",
        LoaderSpec {
            kind: LoaderKind::Vanilla,
            version: None,
        },
        p.id,
    )
    .unwrap();
    db.create_instance(&i).unwrap();
    assert_eq!(
        db.delete_profile(p.id),
        Err(CoreError::CompatibilityConflict)
    );
    db.delete_instance_record(i.id).unwrap();
    db.delete_profile(p.id).unwrap();
}
#[test]
fn snapshot_backup_is_restorable() {
    let root = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let mut db = SqliteStore::open(root.path()).unwrap();
    let p = OfflineProfile::new("BackupUser").unwrap();
    db.create_profile(&p).unwrap();
    db.backup_to(&backups.path().join("launcher.sqlite3"))
        .unwrap();
    let restored = SqliteStore::open(backups.path()).unwrap();
    assert_eq!(restored.get_profile(p.id).unwrap(), Some(p));
}
#[test]
fn concurrent_writes_keep_a_single_stable_id() {
    let root = tempfile::tempdir().unwrap();
    let mut db = SqliteStore::open(root.path()).unwrap();
    let p = OfflineProfile::new("Concurrent").unwrap();
    db.create_profile(&p).unwrap();
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let path = root.path().to_path_buf();
            let id = p.id;
            std::thread::spawn(move || {
                let mut db = SqliteStore::open(&path).unwrap();
                let mut profile = db.get_profile(id).unwrap().unwrap();
                profile.rename(format!("Player_{i}")).unwrap();
                db.update_profile(&profile).unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let db = SqliteStore::open(root.path()).unwrap();
    assert_eq!(db.list_profiles().unwrap().len(), 1);
    assert_eq!(db.get_profile(p.id).unwrap().unwrap().id, p.id);
}
#[test]
fn invalid_instance_path_is_rejected() {
    let p = OfflineProfile::new("SomeUser").unwrap();
    let mut i = GameInstance::new(
        "World",
        "1.21.1",
        LoaderSpec {
            kind: LoaderKind::Vanilla,
            version: None,
        },
        p.id,
    )
    .unwrap();
    i.root_path = "../../Users/saves".into();
    assert!(i.validate().is_err());
}

#[test]
fn backup_refuses_to_overwrite_existing_backup() {
    let root = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let db = SqliteStore::open(root.path()).unwrap();
    let target = backups.path().join("existing.sqlite3");
    std::fs::write(&target, b"retain original backup").unwrap();
    assert_eq!(db.backup_to(&target), Err(CoreError::AlreadyExists));
    assert_eq!(std::fs::read(target).unwrap(), b"retain original backup");
}

#[test]
fn newer_database_schema_is_rejected_without_modification() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = rusqlite::Connection::open(dir.path().join("launcher.sqlite3")).unwrap();
        db.execute_batch("PRAGMA user_version = 999;").unwrap();
    }
    assert!(matches!(
        SqliteStore::open(dir.path()),
        Err(CoreError::UnsupportedVersion)
    ));
    let db = rusqlite::Connection::open(dir.path().join("launcher.sqlite3")).unwrap();
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 999);
}

#[test]
fn deleting_instance_metadata_preserves_world_directory() {
    let root = tempfile::tempdir().unwrap();
    let mut db = SqliteStore::open(root.path()).unwrap();
    let p = OfflineProfile::new("SafePlayer").unwrap();
    db.create_profile(&p).unwrap();
    let instance = GameInstance::new(
        "Protected World",
        "1.20.1",
        LoaderSpec {
            kind: LoaderKind::Vanilla,
            version: None,
        },
        p.id,
    )
    .unwrap();
    db.create_instance(&instance).unwrap();
    let save = root
        .path()
        .join(&instance.root_path)
        .join(".minecraft")
        .join("saves")
        .join("WorldOne");
    std::fs::create_dir_all(&save).unwrap();
    std::fs::write(save.join("level.dat"), b"world-fixture-preserve").unwrap();
    db.delete_instance_record(instance.id).unwrap();
    assert!(db.get_instance(instance.id).unwrap().is_none());
    assert_eq!(
        std::fs::read(save.join("level.dat")).unwrap(),
        b"world-fixture-preserve"
    );
}
