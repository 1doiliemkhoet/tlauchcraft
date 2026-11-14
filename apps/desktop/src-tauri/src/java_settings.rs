//! Additive per-instance JVM preferences. Never mutates existing instance/profile JSON or migrations.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{path::{Path, PathBuf}, process::Command};
use tl_storage::{default_data_dir, SqliteStore, InstanceStore};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JavaSettings {
    pub instance_id: String,
    pub xms_mb: u32,
    pub xmx_mb: u32,
    pub java_path: String,
    pub revision: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct MemoryInfo {
    pub total_mb: u64,
    pub available_mb: u64,
    pub reserved_mb: u64,
}
fn memory_info() -> Result<MemoryInfo, String> {
    #[cfg(not(windows))]
    { return Err("Windows memory detection only".into()); }
    #[cfg(windows)]
    {
        let out=Command::new("powershell.exe")
            .args(["-NoProfile","-NonInteractive","-Command",
                "$m=Get-CimInstance Win32_OperatingSystem; Write-Output ($m.TotalVisibleMemorySize.ToString()+' '+$m.FreePhysicalMemory.ToString())"])
            .output().map_err(|e|format!("Windows RAM probe failed: {e}"))?;
        if !out.status.success() { return Err("Windows memory probe failed".into()); }
        let raw=String::from_utf8_lossy(&out.stdout);
        let vals: Vec<u64>=raw.split_whitespace().map(str::parse::<u64>).collect::<Result<_,_>>()
            .map_err(|_|"Windows RAM response invalid")?;
        if vals.len()!=2 { return Err("Windows RAM response incomplete".into()); }
        let total=vals[0]/1024; let available=vals[1]/1024;
        if total<1024 || available>total || available<768 { return Err("Insufficient or invalid available system RAM".into()); }
        Ok(MemoryInfo{total_mb:total,available_mb:available,reserved_mb:1024})
    }
}
fn connect()->Result<Connection,String> {
    let base=default_data_dir().map_err(|e|e.to_string())?;
    // Run preexisting Foundation migration first; this schema never replaces it.
    let _store=SqliteStore::open(&base).map_err(|e|e.to_string())?;
    let db=Connection::open(base.join("launcher.sqlite3")).map_err(|e|e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(10)).map_err(|e|e.to_string())?;
    init_schema(&db)?;
    Ok(db)
}
fn init_schema(db:&Connection)->Result<(),String>{
    db.execute_batch("PRAGMA foreign_keys=ON;
      CREATE TABLE IF NOT EXISTS instance_java_settings (
        instance_id TEXT PRIMARY KEY REFERENCES instances(id) ON DELETE CASCADE,
        xms_mb INTEGER NOT NULL CHECK(xms_mb>=256),
        xmx_mb INTEGER NOT NULL CHECK(xmx_mb>=xms_mb),
        java_path TEXT NOT NULL DEFAULT '',
        revision INTEGER NOT NULL DEFAULT 0
      );").map_err(|e|e.to_string())?;
    Ok(())
}
fn checked_id(s:&str)->Result<Uuid,String> {
    Uuid::parse_str(s).map_err(|_|"Invalid instance UUID".into())
}
fn has_instance(id:Uuid)->Result<(),String>{
    let store=SqliteStore::open(&default_data_dir().map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    if store.get_instance(id).map_err(|e|e.to_string())?.is_none() {
        return Err("Instance is not in SQLite".into());
    }
    Ok(())
}
fn defaults(id:&str,mem:&MemoryInfo)->JavaSettings {
    let ceiling=mem.total_mb.saturating_sub(mem.reserved_mb)
        .min(mem.available_mb.saturating_sub(512))
        .min(4096).max(256);
    let xmx=ceiling.min(2048) as u32;
    JavaSettings{instance_id:id.into(),xms_mb:xmx.min(512),xmx_mb:xmx,java_path:String::new(),revision:0}
}
pub fn read(id:&str)->Result<JavaSettings,String>{
    let parsed=checked_id(id)?;
    has_instance(parsed)?;
    let db=connect()?;
    let found=db.query_row("SELECT xms_mb,xmx_mb,java_path,revision FROM instance_java_settings WHERE instance_id=?1",
        [id],|r|Ok(JavaSettings{instance_id:id.into(),xms_mb:r.get(0)?,xmx_mb:r.get(1)?,java_path:r.get(2)?,revision:r.get(3)?}))
        .optional().map_err(|e|e.to_string())?;
    if let Some(value)=found {return Ok(value)}
    Ok(defaults(id,&memory_info()?))
}
pub fn system_memory()->Result<MemoryInfo,String>{memory_info()}
pub fn validate(xms:u32,xmx:u32,java:&str,mem:&MemoryInfo)->Result<(),String>{
    if xms<256 || xmx<xms || xmx>65536 {return Err("Invalid memory: require 256 MB <= Xms <= Xmx <= 65536 MB".into())}
    if (xmx as u64)>mem.total_mb.saturating_sub(mem.reserved_mb) {
        return Err(format!("Xmx exceeds physical RAM after {} MB system reserve",mem.reserved_mb));
    }
    if (xmx as u64)>mem.available_mb.saturating_sub(512) {
        return Err("Xmx exceeds currently available RAM after 512 MB safety margin".into());
    }
    if java.len()>1024 || java.contains('\0') || java.chars().any(|c|c.is_control()){
        return Err("Invalid Java executable path".into());
    }
    if !java.is_empty(){
        let path=Path::new(java);
        if !path.is_absolute() || !path.is_file() {
            return Err("Java path must be an existing absolute executable path".into());
        }
        if !path.extension().and_then(|x|x.to_str()).is_some_and(|x|x.eq_ignore_ascii_case("exe")) {
            return Err("Java executable must be a Windows .exe file".into());
        }
        let output=Command::new(path).arg("-version").output()
            .map_err(|e|format!("Java probe failed: {e}"))?;
        if !output.status.success() {return Err("Selected Java executable failed -version".into())}
        let s=format!("{}{}",String::from_utf8_lossy(&output.stderr),String::from_utf8_lossy(&output.stdout));
        tl_game::parse_java_major(&s).map_err(|e|format!("Invalid Java version: {e}"))?;
    }
    Ok(())
}
pub fn save(id:&str,xms:u32,xmx:u32,java_path:String,expected_revision:i64)->Result<JavaSettings,String>{
    let parsed=checked_id(id)?;
    has_instance(parsed)?;
    if expected_revision<0 {return Err("Invalid expected revision".into())}
    validate(xms,xmx,&java_path,&memory_info()?)?;
    let mut db=connect()?;
    persist_settings(&mut db,id,xms,xmx,&java_path,expected_revision)?;
    read(id)
}
fn persist_settings(db:&mut Connection,id:&str,xms:u32,xmx:u32,java_path:&str,expected_revision:i64)->Result<i64,String>{
    let tx=db.transaction().map_err(|e|e.to_string())?;
    let current:Option<i64>=tx.query_row("SELECT revision FROM instance_java_settings WHERE instance_id=?1",
        [id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    let new_revision=match current {
        Some(old) if old==expected_revision=>{
            let changed=tx.execute("UPDATE instance_java_settings SET xms_mb=?2,xmx_mb=?3,java_path=?4,revision=revision+1 WHERE instance_id=?1 AND revision=?5",
                params![id,xms,xmx,java_path,old]).map_err(|e|e.to_string())?;
            if changed!=1 {return Err("Settings revision conflict".into())}
            old+1
        },
        None if expected_revision==0=>{
            tx.execute("INSERT INTO instance_java_settings (instance_id,xms_mb,xmx_mb,java_path,revision) VALUES (?1,?2,?3,?4,1)",
                params![id,xms,xmx,java_path]).map_err(|e|format!("Settings insert failed (possibly revision conflict): {e}"))?;
            1
        },
        _=>return Err("Settings revision conflict: reload before saving".into())
    };
    tx.commit().map_err(|e|e.to_string())?;
    Ok(new_revision)
}

pub fn java_executable(s:&JavaSettings)->PathBuf{
    if s.java_path.is_empty(){PathBuf::from("java.exe")}else{PathBuf::from(&s.java_path)}
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test]fn rejects_inverted_and_oversubscribed_ram(){
   let m=MemoryInfo{total_mb:8192,available_mb:4096,reserved_mb:1024};
   assert!(validate(4096,2048,"",&m).is_err());
   assert!(validate(512,4096,"",&m).is_err());
   assert!(validate(512,2048,"",&m).is_ok());
 }
 #[test]fn sqlite_settings_survive_connection_restart_and_enforce_cas(){
   let file=std::env::temp_dir().join(format!("tl-g8-jvm-settings-{}.sqlite3",Uuid::new_v4()));
   let instance=Uuid::new_v4().to_string();
   {
     let mut db=Connection::open(&file).unwrap();
     db.execute_batch("CREATE TABLE instances (id TEXT PRIMARY KEY);").unwrap();
     db.execute("INSERT INTO instances(id) VALUES (?1)",[&instance]).unwrap();
     init_schema(&db).unwrap();
     assert_eq!(persist_settings(&mut db,&instance,768,3072,"",0).unwrap(),1);
     assert!(persist_settings(&mut db,&instance,512,1024,"",0).is_err());
   }
   {
     let mut reopened=Connection::open(&file).unwrap();
     init_schema(&reopened).unwrap();
     let values:(u32,u32,i64)=reopened.query_row(
       "SELECT xms_mb,xmx_mb,revision FROM instance_java_settings WHERE instance_id=?1",
       [&instance], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))
     ).unwrap();
     assert_eq!(values,(768,3072,1));
     assert!(persist_settings(&mut reopened,&instance,512,2048,"",0).is_err());
     assert_eq!(persist_settings(&mut reopened,&instance,1024,4096,"",1).unwrap(),2);
     let values:(u32,u32,i64)=reopened.query_row(
       "SELECT xms_mb,xmx_mb,revision FROM instance_java_settings WHERE instance_id=?1",
       [&instance], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))
     ).unwrap();
     assert_eq!(values,(1024,4096,2));
   }
   let _=std::fs::remove_file(file);
 }
 #[test]fn defaults_are_safe_for_small_available_ram(){
   let m=MemoryInfo{total_mb:8192,available_mb:2500,reserved_mb:1024};
   let d=defaults("example",&m);
   assert_eq!(d.xmx_mb,1988);
   assert!(d.xms_mb<=d.xmx_mb);
 }
}
