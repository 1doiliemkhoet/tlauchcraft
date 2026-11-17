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
#[cfg(windows)]
#[repr(C)]
struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page_file: u64,
    avail_page_file: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended_virtual: u64,
}
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
}
fn memory_info() -> Result<MemoryInfo, String> {
    #[cfg(not(windows))]
    { return Err("Windows memory detection only".into()); }
    #[cfg(windows)]
    {
        // Native Windows kernel32 API, no 300-1000 ms PowerShell/WMI process on tab entry.
        let mut status = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0, total_phys: 0, avail_phys: 0,
            total_page_file: 0, avail_page_file: 0,
            total_virtual: 0, avail_virtual: 0, avail_extended_virtual: 0
        };
        if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
            return Err(format!("Windows RAM probe failed: {}", std::io::Error::last_os_error()));
        }
        let total = status.total_phys / (1024 * 1024);
        let available = status.avail_phys / (1024 * 1024);
        if total < 1024 || available > total || available < 768 {
            return Err("Insufficient or invalid available system RAM".into());
        }
        Ok(MemoryInfo {total_mb:total,available_mb:available,reserved_mb:1024})
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
        let mut probe=Command::new(path);
        probe.arg("-version");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            probe.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let output=probe.output().map_err(|e|format!("Java probe failed: {e}"))?;
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


#[derive(Clone, Debug, Serialize)]
pub struct JavaDetection {
    pub path: String,
    pub major: u32,
    pub source: String,
}

/// Discover local Java 8 installations without a CurseForge key, network
/// request, PowerShell, or installer execution. Only verified -version output
/// (and not a directory name) determines compatibility.
#[cfg(windows)]
pub fn detect_java8() -> Result<Option<JavaDetection>, String> {
    use std::{
        collections::HashSet,
        process::Stdio,
        time::{Duration, Instant},
    };

    fn add_candidate(
        found: &mut Vec<(PathBuf, String)>,
        seen: &mut HashSet<String>,
        path: PathBuf,
        source: &str,
    ) {
        if found.len() >= 160 || !path.is_absolute() || !path.is_file() {
            return;
        }
        let key = path.to_string_lossy().to_ascii_lowercase();
        if seen.insert(key) {
            found.push((path, source.to_owned()));
        }
    }

    fn add_home(
        found: &mut Vec<(PathBuf, String)>,
        seen: &mut HashSet<String>,
        home: &Path,
        source: &str,
    ) {
        add_candidate(found, seen, home.join("bin").join("java.exe"), source);
        add_candidate(found, seen, home.join("jre").join("bin").join("java.exe"), source);
        add_candidate(found, seen, home.join("java.exe"), source);
    }

    fn scan_vendor(
        found: &mut Vec<(PathBuf, String)>,
        seen: &mut HashSet<String>,
        dir: &Path,
        label: &str,
        levels: u32,
    ) {
        if found.len() >= 160 || levels == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return; };
        for item in entries.take(64).flatten() {
            if found.len() >= 160 { break; }
            if !item.file_type().is_ok_and(|kind| kind.is_dir()) { continue; }
            let child = item.path();
            add_home(found, seen, &child, label);
            if levels > 1 { scan_vendor(found, seen, &child, label, levels - 1); }
        }
    }

    fn read_version(path: &Path) -> Option<u32> {
        use std::os::windows::process::CommandExt;
        let mut child = Command::new(path)
            .arg("-version")
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
        let begin = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    let output = child.wait_with_output().ok()?;
                    if !output.status.success() { return None; }
                    let lines = format!(
                        "{}{}", String::from_utf8_lossy(&output.stderr),
                        String::from_utf8_lossy(&output.stdout)
                    );
                    return tl_game::parse_java_major(&lines).ok();
                }
                Ok(None) if begin.elapsed() < Duration::from_millis(1500) => {
                    std::thread::sleep(Duration::from_millis(40));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        }
    }

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for key in ["JAVA_HOME", "JRE_HOME", "JDK_HOME"] {
        if let Some(home) = std::env::var_os(key) {
            add_home(&mut candidates, &mut seen, &PathBuf::from(home), key);
        }
    }
    if let Some(value) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&value).take(100) {
            add_candidate(&mut candidates, &mut seen, dir.join("java.exe"), "PATH");
        }
    }
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(dir) = std::env::var_os(key) {
            let base = PathBuf::from(dir);
            for vendor in [
                "Java", "Eclipse Adoptium", "Adoptium", "Microsoft",
                "Amazon Corretto", "Zulu", "BellSoft", "Liberica", "OpenJDK",
                "Semeru", "IBM",
            ] {
                scan_vendor(
                    &mut candidates, &mut seen, &base.join(vendor),
                    "Installed Java", 2,
                );
            }
        }
    }
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(dir).join("Programs");
        for vendor in ["Java", "Eclipse Adoptium", "Adoptium", "Microsoft", "Zulu"] {
            scan_vendor(&mut candidates, &mut seen, &base.join(vendor), "User Java", 2);
        }
    }
    // CurseForge App may manage a separate Minecraft installation and legacy JRE.
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let runtime = PathBuf::from(home).join("curseforge").join("minecraft")
            .join("Install").join("runtime");
        for family in ["jre-legacy", "java-runtime-alpha"] {
            for arch in ["windows-x64", "windows-x86"] {
                add_home(
                    &mut candidates, &mut seen,
                    &runtime.join(family).join(arch).join(family),
                    "CurseForge-managed Minecraft runtime",
                );
            }
        }
    }
    // Official Minecraft Launcher jre-legacy (Java 8) runtime layout.
    if let Some(dir) = std::env::var_os("APPDATA") {
        let runtime = PathBuf::from(dir).join(".minecraft").join("runtime");
        for family in ["jre-legacy", "java-runtime-alpha", "java-runtime-beta"] {
            for arch in ["windows-x64", "windows-x86"] {
                add_home(
                    &mut candidates, &mut seen,
                    &runtime.join(family).join(arch).join(family),
                    "Minecraft Launcher runtime",
                );
            }
        }
    }
    // Probing an arbitrary Java executable means starting a local process.
    // Only discover from the user's system environment or standard runtime
    // folders; never search inside or execute JARs from an imported modpack.
    let deadline = Instant::now() + Duration::from_secs(14);
    for (path, source) in candidates {
        if Instant::now() >= deadline { break; }
        if read_version(&path) == Some(8) {
            return Ok(Some(JavaDetection {
                path: path.to_string_lossy().into_owned(),
                major: 8,
                source,
            }));
        }
    }
    Ok(None)
}

#[cfg(not(windows))]
pub fn detect_java8() -> Result<Option<JavaDetection>, String> {
    Ok(None)
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
