//! Transactional local SQLite metadata. Does not touch game worlds or imported data.
use rusqlite::{params, Connection, ErrorCode, OptionalExtension, TransactionBehavior};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tl_core::{CoreError, GameInstance, OfflineProfile};
use uuid::Uuid;
type Result<T> = std::result::Result<T, CoreError>;

pub trait ProfileStore {
    fn create_profile(&mut self, p: &OfflineProfile) -> Result<()>;
    fn get_profile(&self, id: Uuid) -> Result<Option<OfflineProfile>>;
    fn list_profiles(&self) -> Result<Vec<OfflineProfile>>;
    fn update_profile(&mut self, p: &OfflineProfile) -> Result<()>;
    fn delete_profile(&mut self, id: Uuid) -> Result<()>;
}
pub trait InstanceStore {
    fn create_instance(&mut self, i: &GameInstance) -> Result<()>;
    fn get_instance(&self, id: Uuid) -> Result<Option<GameInstance>>;
    fn list_instances(&self) -> Result<Vec<GameInstance>>;
    fn update_instance(&mut self, i: &GameInstance) -> Result<()>;
    /// Metadata only; never deletes disk directories or game saves.
    fn delete_instance_record(&mut self, id: Uuid) -> Result<()>;
}
fn db_err(e: rusqlite::Error) -> CoreError {
    match e {
        rusqlite::Error::SqliteFailure(x, _) if x.code == ErrorCode::ConstraintViolation => {
            CoreError::AlreadyExists
        }
        rusqlite::Error::SqliteFailure(x, _) if x.code == ErrorCode::DiskFull => {
            CoreError::DiskFull
        }
        _ => CoreError::StorageFailure,
    }
}
pub fn isolated_data_dir(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(CoreError::InvalidInput);
    }
    for ancestor in path.ancestors() {
        if ancestor.exists()
            && fs::symlink_metadata(ancestor)
                .map_err(|_| CoreError::StorageFailure)?
                .file_type()
                .is_symlink()
        {
            return Err(CoreError::InvalidInput);
        }
    }
    fs::create_dir_all(path).map_err(|_| CoreError::StorageFailure)?;
    Ok(path.to_path_buf())
}
pub fn default_data_dir() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .ok_or(CoreError::InvalidInput)?;
    isolated_data_dir(&PathBuf::from(base).join("tlauchcraft"))
}
pub struct SqliteStore {
    conn: Connection,
    profile_revisions: RefCell<HashMap<Uuid, i64>>,
    instance_revisions: RefCell<HashMap<Uuid, i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    pub value: T,
    pub revision: i64,
}
impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self> {
        let path = isolated_data_dir(path)?;
        let mut conn = Connection::open(path.join("launcher.sqlite3")).map_err(db_err)?;
        conn.busy_timeout(Duration::from_secs(10)).map_err(db_err)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(db_err)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_err)?;
        migrate(&mut conn)?;
        Ok(Self {
            conn,
            profile_revisions: RefCell::new(HashMap::new()),
            instance_revisions: RefCell::new(HashMap::new()),
        })
    }
    pub fn schema_version(&self) -> Result<i64> {
        self.conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(db_err)
    }
    /// Creates a SQLite-consistent backup and atomically publishes it without clobbering.
    /// Temporary files are cleaned up on failure; directories are never created implicitly.
    pub fn backup_to(&self, target: &Path) -> Result<()> {
        let parent = target.parent().ok_or(CoreError::InvalidInput)?;
        if !parent.is_absolute() || !parent.is_dir() || target.file_name().is_none() {
            return Err(CoreError::InvalidInput);
        }
        for ancestor in parent.ancestors() {
            if ancestor.exists()
                && fs::symlink_metadata(ancestor)
                    .map_err(|_| CoreError::StorageFailure)?
                    .file_type()
                    .is_symlink()
            {
                return Err(CoreError::InvalidInput);
            }
        }
        if fs::symlink_metadata(target).is_ok() {
            return Err(CoreError::AlreadyExists);
        }
        let temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|_| CoreError::StorageFailure)?;
        {
            let mut dest = Connection::open(temporary.path()).map_err(db_err)?;
            let backup = rusqlite::backup::Backup::new(&self.conn, &mut dest).map_err(db_err)?;
            backup
                .run_to_completion(64, Duration::from_millis(10), None)
                .map_err(db_err)?;
        }
        temporary.persist_noclobber(target).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                CoreError::AlreadyExists
            } else {
                CoreError::StorageFailure
            }
        })?;
        Ok(())
    }

    pub fn get_profile_versioned(&self, id: Uuid) -> Result<Option<Versioned<OfflineProfile>>> {
        let item: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT payload,revision FROM profiles WHERE id=?1",
                [id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_err)?;
        let decoded = item
            .map(|(json, revision)| {
                Ok(Versioned {
                    value: decode(json)?,
                    revision,
                })
            })
            .transpose()?;
        if let Some(ref found) = decoded {
            self.profile_revisions
                .borrow_mut()
                .insert(id, found.revision);
        }
        Ok(decoded)
    }

    pub fn get_instance_versioned(&self, id: Uuid) -> Result<Option<Versioned<GameInstance>>> {
        let item: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT payload,revision FROM instances WHERE id=?1",
                [id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_err)?;
        let decoded = item
            .map(|(json, revision)| {
                Ok(Versioned {
                    value: decode(json)?,
                    revision,
                })
            })
            .transpose()?;
        if let Some(ref found) = decoded {
            self.instance_revisions
                .borrow_mut()
                .insert(id, found.revision);
        }
        Ok(decoded)
    }

    /// Atomic compare-and-swap: stale writer cannot overwrite a newer revision.
    pub fn update_profile_if_revision(
        &mut self,
        profile: &OfflineProfile,
        expected: i64,
    ) -> Result<i64> {
        tl_core::validate_username(&profile.username)?;
        let next = self.conn.execute(
            "UPDATE profiles SET username=?2,payload=?3,revision=revision+1 WHERE id=?1 AND revision=?4",
            params![profile.id.to_string(),profile.username,encode(profile)?,expected]
        ).map_err(db_err)?;
        if next == 0 {
            return Err(if self.get_profile_versioned(profile.id)?.is_some() {
                CoreError::CompatibilityConflict
            } else {
                CoreError::NotFound
            });
        }
        let revision = expected + 1;
        self.profile_revisions
            .borrow_mut()
            .insert(profile.id, revision);
        Ok(revision)
    }

    pub fn update_instance_if_revision(
        &mut self,
        instance: &GameInstance,
        expected: i64,
    ) -> Result<i64> {
        instance.validate()?;
        let next = self.conn.execute(
            "UPDATE instances SET profile_id=?2,payload=?3,revision=revision+1 WHERE id=?1 AND revision=?4",
            params![instance.id.to_string(),instance.profile_id.to_string(),encode(instance)?,expected]
        ).map_err(db_err)?;
        if next == 0 {
            return Err(if self.get_instance_versioned(instance.id)?.is_some() {
                CoreError::CompatibilityConflict
            } else {
                CoreError::NotFound
            });
        }
        let revision = expected + 1;
        self.instance_revisions
            .borrow_mut()
            .insert(instance.id, revision);
        Ok(revision)
    }
}
fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_err)?;
    if version > 2 {
        return Err(CoreError::UnsupportedVersion);
    }
    if version == 2 {
        return Ok(());
    }
    if version == 1 {
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_err)?;
        tx.execute_batch("ALTER TABLE profiles ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;\nALTER TABLE instances ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;\nPRAGMA user_version = 2;").map_err(db_err)?;
        return tx.commit().map_err(db_err);
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_err)?;
    tx.execute_batch(
        "CREATE TABLE profiles (
 id TEXT PRIMARY KEY,username TEXT NOT NULL COLLATE NOCASE UNIQUE,payload TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 0
 );
 CREATE TABLE instances (
 id TEXT PRIMARY KEY,profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
 payload TEXT NOT NULL
 );
 CREATE INDEX instances_profile_idx ON instances(profile_id);
 PRAGMA user_version = 2;",
    )
    .map_err(db_err)?;
    tx.commit().map_err(db_err)
}
fn encode<T: serde::Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|_| CoreError::StorageFailure)
}
fn decode<T: serde::de::DeserializeOwned>(v: String) -> Result<T> {
    serde_json::from_str(&v).map_err(|_| CoreError::StorageFailure)
}
impl ProfileStore for SqliteStore {
    fn create_profile(&mut self, p: &OfflineProfile) -> Result<()> {
        tl_core::validate_username(&p.username)?;
        self.conn
            .execute(
                "INSERT INTO profiles(id,username,payload) VALUES (?1,?2,?3)",
                params![p.id.to_string(), p.username, encode(p)?],
            )
            .map_err(db_err)?;
        self.profile_revisions.borrow_mut().insert(p.id, 0);
        Ok(())
    }
    fn get_profile(&self, id: Uuid) -> Result<Option<OfflineProfile>> {
        Ok(self.get_profile_versioned(id)?.map(|v| v.value))
    }
    fn list_profiles(&self) -> Result<Vec<OfflineProfile>> {
        let mut stmt = self
            .conn
            .prepare("SELECT payload FROM profiles ORDER BY username")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_err)?;
        let profiles: Vec<OfflineProfile> = rows
            .map(|v| decode(v.map_err(db_err)?))
            .collect::<Result<_>>()?;
        for p in &profiles {
            self.get_profile_versioned(p.id)?;
        }
        Ok(profiles)
    }
    fn update_profile(&mut self, p: &OfflineProfile) -> Result<()> {
        let expected = self
            .profile_revisions
            .borrow()
            .get(&p.id)
            .copied()
            .ok_or(CoreError::CompatibilityConflict)?;
        self.update_profile_if_revision(p, expected)?;
        Ok(())
    }
    fn delete_profile(&mut self, id: Uuid) -> Result<()> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM instances WHERE profile_id=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        if n > 0 {
            return Err(CoreError::CompatibilityConflict);
        }
        let affected = self
            .conn
            .execute("DELETE FROM profiles WHERE id=?1", [id.to_string()])
            .map_err(db_err)?;
        if affected == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }
}
impl InstanceStore for SqliteStore {
    fn create_instance(&mut self, i: &GameInstance) -> Result<()> {
        i.validate()?;
        self.conn
            .execute(
                "INSERT INTO instances(id,profile_id,payload) VALUES (?1,?2,?3)",
                params![i.id.to_string(), i.profile_id.to_string(), encode(i)?],
            )
            .map_err(db_err)?;
        self.instance_revisions.borrow_mut().insert(i.id, 0);
        Ok(())
    }
    fn get_instance(&self, id: Uuid) -> Result<Option<GameInstance>> {
        Ok(self.get_instance_versioned(id)?.map(|v| v.value))
    }
    fn list_instances(&self) -> Result<Vec<GameInstance>> {
        let mut stmt = self
            .conn
            .prepare("SELECT payload FROM instances ORDER BY id")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_err)?;
        let instances: Vec<GameInstance> = rows
            .map(|v| decode(v.map_err(db_err)?))
            .collect::<Result<_>>()?;
        for i in &instances {
            self.get_instance_versioned(i.id)?;
        }
        Ok(instances)
    }
    fn update_instance(&mut self, i: &GameInstance) -> Result<()> {
        let expected = self
            .instance_revisions
            .borrow()
            .get(&i.id)
            .copied()
            .ok_or(CoreError::CompatibilityConflict)?;
        self.update_instance_if_revision(i, expected)?;
        Ok(())
    }
    fn delete_instance_record(&mut self, id: Uuid) -> Result<()> {
        let affected = self
            .conn
            .execute("DELETE FROM instances WHERE id=?1", [id.to_string()])
            .map_err(db_err)?;
        if affected == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }
}
