//! Transactional local SQLite metadata. Does not touch game worlds or imported data.
use rusqlite::{params, Connection, ErrorCode, OptionalExtension, TransactionBehavior};
use std::{
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
        Ok(Self { conn })
    }
    pub fn schema_version(&self) -> Result<i64> {
        self.conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(db_err)
    }
    /// Consistent backup including WAL pages; refuses overwrite.
    pub fn backup_to(&self, target: &Path) -> Result<()> {
        if target.exists() {
            return Err(CoreError::AlreadyExists);
        }
        let mut dest = Connection::open(target).map_err(db_err)?;
        let backup = rusqlite::backup::Backup::new(&self.conn, &mut dest).map_err(db_err)?;
        backup
            .run_to_completion(64, Duration::from_millis(10), None)
            .map_err(db_err)
    }
}
fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_err)?;
    if version > 1 {
        return Err(CoreError::UnsupportedVersion);
    }
    if version == 1 {
        return Ok(());
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_err)?;
    tx.execute_batch(
        "CREATE TABLE profiles (
 id TEXT PRIMARY KEY,username TEXT NOT NULL COLLATE NOCASE UNIQUE,payload TEXT NOT NULL
 );
 CREATE TABLE instances (
 id TEXT PRIMARY KEY,profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
 payload TEXT NOT NULL
 );
 CREATE INDEX instances_profile_idx ON instances(profile_id);
 PRAGMA user_version = 1;",
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
        Ok(())
    }
    fn get_profile(&self, id: Uuid) -> Result<Option<OfflineProfile>> {
        let v: Option<String> = self
            .conn
            .query_row(
                "SELECT payload FROM profiles WHERE id=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        v.map(decode).transpose()
    }
    fn list_profiles(&self) -> Result<Vec<OfflineProfile>> {
        let mut stmt = self
            .conn
            .prepare("SELECT payload FROM profiles ORDER BY username")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_err)?;
        rows.map(|v| decode(v.map_err(db_err)?)).collect()
    }
    fn update_profile(&mut self, p: &OfflineProfile) -> Result<()> {
        tl_core::validate_username(&p.username)?;
        let affected = self
            .conn
            .execute(
                "UPDATE profiles SET username=?2,payload=?3 WHERE id=?1",
                params![p.id.to_string(), p.username, encode(p)?],
            )
            .map_err(db_err)?;
        if affected == 0 {
            return Err(CoreError::NotFound);
        }
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
        Ok(())
    }
    fn get_instance(&self, id: Uuid) -> Result<Option<GameInstance>> {
        let v: Option<String> = self
            .conn
            .query_row(
                "SELECT payload FROM instances WHERE id=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        v.map(decode).transpose()
    }
    fn list_instances(&self) -> Result<Vec<GameInstance>> {
        let mut stmt = self
            .conn
            .prepare("SELECT payload FROM instances ORDER BY id")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_err)?;
        rows.map(|v| decode(v.map_err(db_err)?)).collect()
    }
    fn update_instance(&mut self, i: &GameInstance) -> Result<()> {
        i.validate()?;
        let affected = self
            .conn
            .execute(
                "UPDATE instances SET profile_id=?2,payload=?3 WHERE id=?1",
                params![i.id.to_string(), i.profile_id.to_string(), encode(i)?],
            )
            .map_err(db_err)?;
        if affected == 0 {
            return Err(CoreError::NotFound);
        }
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
