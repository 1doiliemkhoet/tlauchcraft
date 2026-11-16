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
    assert_eq!(store.schema_version().unwrap(), 3);
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
fn concurrent_writes_detect_stale_revision_on_two_connections() {
    let root = tempfile::tempdir().unwrap();
    let mut initial = SqliteStore::open(root.path()).unwrap();
    let p = OfflineProfile::new("Concurrent").unwrap();
    initial.create_profile(&p).unwrap();
    let mut one = SqliteStore::open(root.path()).unwrap();
    let mut two = SqliteStore::open(root.path()).unwrap();
    let mut first = one.get_profile_versioned(p.id).unwrap().unwrap();
    let mut second = two.get_profile_versioned(p.id).unwrap().unwrap();
    assert_eq!(first.revision, 0);
    first.value.rename("Winner").unwrap();
    second.value.rename("Stale").unwrap();
    assert_eq!(
        one.update_profile_if_revision(&first.value, first.revision),
        Ok(1)
    );
    assert_eq!(
        two.update_profile_if_revision(&second.value, second.revision),
        Err(CoreError::CompatibilityConflict)
    );
    assert_eq!(two.get_profile(p.id).unwrap().unwrap().username, "Winner");
    assert_eq!(
        two.get_profile_versioned(p.id).unwrap().unwrap().revision,
        1
    );
}

#[test]
fn legacy_update_path_also_rejects_stale_profile_and_instance() {
    let root = tempfile::tempdir().unwrap();
    let mut one = SqliteStore::open(root.path()).unwrap();
    let mut two = SqliteStore::open(root.path()).unwrap();
    let p = OfflineProfile::new("Legacy").unwrap();
    one.create_profile(&p).unwrap();
    let mut a = one.get_profile(p.id).unwrap().unwrap();
    let mut b = two.get_profile(p.id).unwrap().unwrap();
    a.rename("Newest").unwrap();
    b.rename("Outdated").unwrap();
    one.update_profile(&a).unwrap();
    assert_eq!(
        two.update_profile(&b),
        Err(CoreError::CompatibilityConflict)
    );
    let i = GameInstance::new(
        "Instance",
        "1.20.1",
        LoaderSpec {
            kind: LoaderKind::Vanilla,
            version: None,
        },
        p.id,
    )
    .unwrap();
    one.create_instance(&i).unwrap();
    let mut ia = one.get_instance(i.id).unwrap().unwrap();
    let mut ib = two.get_instance(i.id).unwrap().unwrap();
    ia.state = InstanceState::Ready;
    ib.state = InstanceState::Failed;
    one.update_instance(&ia).unwrap();
    assert_eq!(
        two.update_instance(&ib),
        Err(CoreError::CompatibilityConflict)
    );
}

#[test]
fn migrated_v1_profiles_and_instances_retain_data() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("launcher.sqlite3");
    let p = OfflineProfile::new("Migrated").unwrap();
    let i = GameInstance::new(
        "OldInstance",
        "1.20.1",
        LoaderSpec {
            kind: LoaderKind::Vanilla,
            version: None,
        },
        p.id,
    )
    .unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE profiles(id TEXT PRIMARY KEY, username TEXT NOT NULL COLLATE NOCASE UNIQUE, payload TEXT NOT NULL); CREATE TABLE instances(id TEXT PRIMARY KEY,profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,payload TEXT NOT NULL); CREATE INDEX instances_profile_idx ON instances(profile_id); PRAGMA user_version=1;").unwrap();
        conn.execute(
            "INSERT INTO profiles VALUES (?1,?2,?3)",
            rusqlite::params![
                p.id.to_string(),
                p.username,
                serde_json::to_string(&p).unwrap()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO instances VALUES (?1,?2,?3)",
            rusqlite::params![
                i.id.to_string(),
                p.id.to_string(),
                serde_json::to_string(&i).unwrap()
            ],
        )
        .unwrap();
    }
    let store = SqliteStore::open(root.path()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 3);
    assert_eq!(store.get_profile_versioned(p.id).unwrap().unwrap().value, p);
    assert_eq!(
        store.get_instance_versioned(i.id).unwrap().unwrap().value,
        i
    );
    assert_eq!(
        store.get_profile_versioned(p.id).unwrap().unwrap().revision,
        0
    );
    drop(store);
    assert_eq!(
        SqliteStore::open(root.path())
            .unwrap()
            .schema_version()
            .unwrap(),
        2
    );
}

#[test]
fn backup_symlink_is_not_overwritten_and_no_temp_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let db = SqliteStore::open(root.path()).unwrap();
    let existing = backups.path().join("original.sqlite3");
    std::fs::write(&existing, b"must stay").unwrap();
    let target = backups.path().join("backup.sqlite3");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&existing, &target).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&existing, &target).unwrap();
    assert_eq!(db.backup_to(&target), Err(CoreError::AlreadyExists));
    assert_eq!(std::fs::read(&existing).unwrap(), b"must stay");
    assert_eq!(std::fs::read_dir(backups.path()).unwrap().count(), 2);
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

#[test]
fn forge_receipt_anchor_is_immutable_and_requires_verifying_intent() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(dir.path()).unwrap();
    let profile = OfflineProfile::new("AnchorUser").unwrap();
    store.create_profile(&profile).unwrap();
    let mut forge = GameInstance::new(
        "Forge",
        "1.16.5",
        LoaderSpec {
            kind: LoaderKind::Forge,
            version: Some("36.2.34".into()),
        },
        profile.id,
    )
    .unwrap();
    store.create_instance(&forge).unwrap();
    let sha = "a".repeat(64);
    assert_eq!(
        store.insert_forge_receipt_anchor(forge.id, &sha),
        Err(CoreError::CompatibilityConflict)
    );
    forge.state = InstanceState::Verifying;
    store.update_instance(&forge).unwrap();
    assert_eq!(
        store.insert_forge_receipt_anchor(forge.id, "not-a-digest"),
        Err(CoreError::InvalidInput)
    );
    store.insert_forge_receipt_anchor(forge.id, &sha).unwrap();
    assert_eq!(
        store.get_forge_receipt_anchor(forge.id).unwrap(),
        Some(sha)
    );
    assert_eq!(
        store.insert_forge_receipt_anchor(forge.id, &"b".repeat(64)),
        Err(CoreError::AlreadyExists)
    );
    let mut forged = store.get_instance(forge.id).unwrap().unwrap();
    forged.state = InstanceState::Ready;
    store.update_instance(&forged).unwrap();
    assert_eq!(
        store.insert_forge_receipt_anchor(forge.id, &"c".repeat(64)),
        Err(CoreError::CompatibilityConflict)
    );
}
