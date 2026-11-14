//! Real Windows Vanilla install/launch bridge; never asserts Minecraft E2E from a process spawn.
use std::{
    collections::HashMap,
    fs, path::{Path, PathBuf},
    process::Child,
    sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}
};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use tauri::{AppHandle,Emitter,State};
use tl_core::{GameInstance,InstanceState,LoaderKind,LoaderSpec,OfflineProfile};
use tl_storage::{default_data_dir,InstanceStore,ProfileStore,SqliteStore};
use uuid::Uuid;
use crate::java_settings;

#[derive(Default)]
pub struct GameState {
    installs: Mutex<HashMap<Uuid,Arc<AtomicBool>>>,
    children: Mutex<HashMap<Uuid,Child>>
}
#[derive(Serialize)]
pub struct GameStatus {
    pub instance_id:String,
    pub install_state:InstanceState,
    pub installing:bool,
    pub pid:Option<u32>,
    pub process_state:String,
    pub last_error:Option<String>,
}
#[derive(Serialize)]
pub struct StartedProcess {
    pub pid:u32,
    pub started_at:String,
    pub log_path:String,
    pub termination_state:&'static str,
}
#[derive(Deserialize)]
pub struct NewVanillaInstance {
    pub name:String,
    pub version:String,
    pub profile_id:String,
}
fn id(raw:&str)->Result<Uuid,String>{
    Uuid::parse_str(raw).map_err(|_|"Invalid UUID".into())
}
fn db()->Result<SqliteStore,String>{
    SqliteStore::open(&default_data_dir().map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}
fn find_instance(uuid:Uuid)->Result<GameInstance,String>{
    db()?.get_instance(uuid).map_err(|e|e.to_string())?
       .ok_or_else(||"Instance does not exist".into())
}
fn root(i:&GameInstance)->Result<PathBuf,String>{
    i.validate().map_err(|e|e.to_string())?;
    let base=default_data_dir().map_err(|e|e.to_string())?;
    if !matches!(&i.loader.kind,LoaderKind::Vanilla){
        return Err("Forge/Fabric/modpack installation is not supported by the Vanilla bridge".into())
    }
    let root=base.join(&i.root_path);
    for part in root.ancestors().take_while(|p|p.starts_with(&base)) {
        if part.exists() && fs::symlink_metadata(part).map_err(|e|e.to_string())?.file_type().is_symlink(){
            return Err("Refusing symlinked instance directory".into())
        }
    }
    Ok(root)
}
fn update_state(uuid:Uuid,next:InstanceState)->Result<(),String>{
    let mut store=db()?;
    let mut record=store.get_instance_versioned(uuid).map_err(|e|e.to_string())?
        .ok_or_else(||"Instance missing from SQLite".to_owned())?;
    if record.value.state==next {return Ok(())}
    record.value.state=next;
    store.update_instance_if_revision(&record.value,record.revision)
        .map_err(|e|format!("Instance revision conflict or storage failure: {e}"))?;
    Ok(())
}
fn error_file(uuid:Uuid)->Result<PathBuf,String>{
    let instance=find_instance(uuid)?;
    Ok(root(&instance)?.join("vanilla-install-error.txt"))
}
fn save_error(uuid:Uuid,error:&str){
    if let Ok(path)=error_file(uuid){
        if let Some(parent)=path.parent(){let _=fs::create_dir_all(parent);}
        // Keep diagnostics local, bounded, and useful across launcher restarts.
        let concise:String=error.chars().take(8192).collect();
        let _=fs::write(path,concise);
    }
}
fn clear_error(uuid:Uuid){
    if let Ok(path)=error_file(uuid){let _=fs::remove_file(path);}
}
fn read_error(uuid:Uuid)->Option<String>{
    let path=error_file(uuid).ok()?;
    if fs::metadata(&path).ok()?.len()>8192 {return None}
    fs::read_to_string(path).ok()
}
fn event(app:&AppHandle,instance:Uuid,phase:&str,key:&str){
    event_detail(app,instance,phase,key,json!({}));
}
fn event_detail(app:&AppHandle,instance:Uuid,phase:&str,key:&str,details:Value){
    // Only actual values reported by tl-game; bytes/percentage stay unknown.
    let _=app.emit("download.progress",json!({
        "contract_version":1,"operation_id":instance.to_string(),
        "instance_id":instance.to_string(),
        "timestamp":chrono::Utc::now().to_rfc3339(),
        "phase":phase,"message_key":key,"details":details,"percent":null
    }));
}
#[tauri::command]
pub fn instance_create_vanilla(req:NewVanillaInstance)->Result<GameInstance,String>{
    let profile=id(&req.profile_id)?;
    if db()?.get_profile(profile).map_err(|e|e.to_string())?.is_none(){
        return Err("Persisted offline profile not found".into())
    }
    let instance=GameInstance::new(req.name,req.version,LoaderSpec{kind:LoaderKind::Vanilla,version:None},profile)
       .map_err(|e|e.to_string())?;
    let mut store=db()?;
    store.create_instance(&instance).map_err(|e|e.to_string())?;
    Ok(instance)
}
fn game_dir_for(root:&Path)->PathBuf {
    // Do not reset or relocate saved worlds across launcher restarts and reinstalls.
    root.join("game")
}
fn count_native_dlls(natives:&Path)->Result<usize,String>{
    let mut count=0;
    for entry in fs::read_dir(natives).map_err(|e|format!("Native extraction directory unreadable: {e}"))? {
        let entry=entry.map_err(|e|format!("Native extraction read failed: {e}"))?;
        let kind=entry.file_type().map_err(|e|e.to_string())?;
        if kind.is_symlink(){return Err("Native extraction contains unexpected symlink".into())}
        if kind.is_file() && entry.path().extension().and_then(|x|x.to_str())
            .is_some_and(|x|x.eq_ignore_ascii_case("dll")) {count+=1;}
    }
    if count==0 {return Err("Native extraction completed without Windows DLLs".into())}
    Ok(count)
}
fn install_work(app:AppHandle,uuid:Uuid,cancel:Arc<AtomicBool>)->Result<(),String>{
    let i=find_instance(uuid)?;
    let dir=root(&i)?;
    if cancel.load(Ordering::Relaxed){return Err("Install cancelled".into())}
    event(&app,uuid,"resolving","Resolving official Mojang Vanilla metadata");
    let version=tl_game::vanilla::resolve_official_version(&i.minecraft_version).map_err(|e|e.to_string())?;
    if cancel.load(Ordering::Relaxed){return Err("Install cancelled".into())}
    update_state(uuid,InstanceState::Downloading)?;
    event(&app,uuid,"downloading","Downloading and verifying official Mojang files");
    let installed=tl_game::vanilla::install_vanilla(&version,&dir,&cancel).map_err(|e|e.to_string())?;
    event_detail(&app,uuid,"downloading","Official Mojang files downloaded with SHA-1 verification",
        json!({"downloaded_files":installed.downloaded_files,"native_archives":installed.native_archives.len()}));
    if cancel.load(Ordering::Relaxed){return Err("Install cancelled".into())}
    update_state(uuid,InstanceState::Verifying)?;
    event(&app,uuid,"verifying","Extracting verified native DLLs");
    // Unique native path prevents overwriting any existing game files/worlds.
    let natives=dir.join(format!("natives-{}",Uuid::new_v4()));
    tl_game::vanilla::extract_natives(&installed.native_archives,&natives).map_err(|e|e.to_string())?;
    let native_dll_count=count_native_dlls(&natives)?;
    if cancel.load(Ordering::Relaxed){return Err("Install cancelled".into())}
    event(&app,uuid,"verifying","Independently rehashing installed Mojang client, libraries, natives and assets");
    let evidence_dir=dir.join("evidence");
    fs::create_dir_all(&evidence_dir).map_err(|e|e.to_string())?;
    let attest=evidence_dir.join(format!("install-checksums-{}.json",Uuid::new_v4()));
    let verified_files=tl_game::vanilla_evidence::write_install_checksums(&version,&dir,&attest)
        .map_err(|e|format!("Installed checksum verification failed: {e}"))?;
    if verified_files != installed.downloaded_files {
        return Err(format!("Verified-file count mismatch: expected {}, got {}",installed.downloaded_files,verified_files))
    }
    event_detail(&app,uuid,"verifying","Independent SHA-1 file attestation complete",
        json!({"verified_files":verified_files,"native_dll_count":native_dll_count}));
    if cancel.load(Ordering::Relaxed){return Err("Install cancelled".into())}
    // Never remove or reinitialize the game directory; it owns persistent saves.
    fs::create_dir_all(game_dir_for(&dir)).map_err(|e|e.to_string())?;
    let metadata=json!({"version":version,"natives_dir":natives,"native_dll_count":native_dll_count,"verified_files":verified_files,"checksum_evidence":attest});
    let tmp=dir.join(format!(".vanilla-install-{}.tmp",Uuid::new_v4()));
    fs::write(&tmp,serde_json::to_vec(&metadata).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    fs::rename(&tmp,dir.join("vanilla-install.json")).map_err(|e|e.to_string())?;
    update_state(uuid,InstanceState::Ready)?;
    clear_error(uuid);
    event(&app,uuid,"ready","Vanilla files verified; launch prerequisites still checked on Play");
    Ok(())
}
#[tauri::command]
pub fn game_install(app:AppHandle,state:State<'_,Arc<GameState>>,instance_id:String)->Result<String,String>{
    let uuid=id(&instance_id)?;
    let i=find_instance(uuid)?;
    root(&i)?;
    if !matches!(&i.state,InstanceState::Empty|InstanceState::Failed|InstanceState::Cancelled|InstanceState::NeedsAction){
        return Err(format!("Cannot install instance in state {:?}",i.state))
    }
    let mut installs=state.installs.lock().map_err(|_|"Install lock poisoned")?;
    if installs.contains_key(&uuid){return Err("Install already in progress".into())}
    update_state(uuid,InstanceState::Planned)?;
    clear_error(uuid);
    let cancelled=Arc::new(AtomicBool::new(false));
    installs.insert(uuid,cancelled.clone());
    drop(installs);
    let worker=state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let outcome=install_work(app.clone(),uuid,cancelled.clone());
        if let Err(ref e)=outcome {
            let status=if cancelled.load(Ordering::Relaxed) || e=="Install cancelled"{
                InstanceState::Cancelled
            }else{InstanceState::Failed};
            let is_cancelled=matches!(&status,InstanceState::Cancelled);
            save_error(uuid,e);
            let _=update_state(uuid,status);
            event(&app,uuid,if is_cancelled{"cancelled"}else{"failed"},e);
        }
        if let Ok(mut tasks)=worker.installs.lock(){tasks.remove(&uuid);}
    });
    Ok(instance_id)
}
#[tauri::command]
pub fn game_cancel(state:State<'_,Arc<GameState>>,instance_id:String)->Result<(),String>{
    let uuid=id(&instance_id)?;
    let map=state.installs.lock().map_err(|_|"Install lock poisoned")?;
    let cancel=map.get(&uuid).ok_or("No cancellable install running")?;
    cancel.store(true,Ordering::Relaxed);
    Ok(())
}
#[tauri::command]
pub fn game_status(state:State<'_,Arc<GameState>>,instance_id:String)->Result<GameStatus,String>{
    let uuid=id(&instance_id)?;
    let mut i=find_instance(uuid)?;
    let installing=state.installs.lock().map_err(|_|"Install lock poisoned")?.contains_key(&uuid);
    if !installing && matches!(&i.state,InstanceState::Planned|InstanceState::Downloading|InstanceState::Verifying) {
        let reason="Install worker was interrupted by a launcher restart; retry install to resume verification";
        save_error(uuid,reason);
        update_state(uuid,InstanceState::Failed)?;
        i=find_instance(uuid)?;
    }
    let mut children=state.children.lock().map_err(|_|"Process lock poisoned")?;
    let mut pid=None;
    let mut process_state="not_running".to_owned();
    if let Some(child)=children.get_mut(&uuid) {
        match child.try_wait() {
            Ok(Some(result))=>{
                process_state=if result.success(){"exited"}else{"crashed"}.into();
                children.remove(&uuid);
            },
            Ok(None)=>{pid=Some(child.id());process_state="running".into();},
            Err(e)=>return Err(format!("Process status failed: {e}"))
        }
    }
    Ok(GameStatus{instance_id,install_state:i.state,installing,pid,process_state,last_error:read_error(uuid)})
}
fn installed_metadata(dir:&Path)->Result<(Value,PathBuf),String>{
    let meta=dir.join("vanilla-install.json");
    let bytes=fs::read(&meta).map_err(|_|"Verified install metadata missing; run Vanilla Install".to_owned())?;
    if bytes.len()>32*1024*1024{return Err("Install metadata is too large".into())}
    let data:Value=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
    let version=data.get("version").cloned().ok_or("Version metadata missing")?;
    let native_str=data.get("natives_dir").and_then(Value::as_str).ok_or("Natives path missing")?;
    let natives=PathBuf::from(native_str);
    if !natives.is_dir() || !natives.starts_with(dir) {
       return Err("Unsafe or missing natives directory".into())
    }
    count_native_dlls(&natives)?;
    // For new installations, require the SHA-1 manifest generated by the merged G2 verifier.
    if let Some(path)=data.get("checksum_evidence").and_then(Value::as_str) {
        let evidence=PathBuf::from(path);
        if !evidence.starts_with(dir) || !evidence.is_file(){
            return Err("Installed checksum evidence missing or outside instance directory".into())
        }
    }
    Ok((version,natives))
}

#[derive(Serialize)]
pub struct LaunchPreflight {
    pub instance_id:String,
    pub java_path:String,
    pub required_java:u32,
    pub xms_mb:u32,
    pub xmx_mb:u32
}
/// No process is created here. Checking readiness never claims G3.
#[tauri::command]
pub fn game_preflight(instance_id:String,profile_id:String)->Result<LaunchPreflight,String>{
    let uuid=id(&instance_id)?;
    let profile_uuid=id(&profile_id)?;
    let i=find_instance(uuid)?;
    if !matches!(&i.state,InstanceState::Ready){
        return Err("Vanilla files must be installed and READY".into())
    }
    let profile=db()?.get_profile(profile_uuid).map_err(|e|e.to_string())?
        .ok_or("Offline profile not in SQLite")?;
    let req=tl_core::LaunchRequest{
        instance_id:uuid,profile_id:profile_uuid,java_path:None,
        min_ram_mb:None,max_ram_mb:None,extra_jvm_args:None
    };
    tl_game::foundation::validate_launch_context(&i,&profile,&req).map_err(|e|e.to_string())?;
    let dir=root(&i)?;
    let (version,natives)=installed_metadata(&dir)?;
    if version.get("id").and_then(Value::as_str)!=Some(i.minecraft_version.as_str()){
        return Err("Installed version metadata mismatch".into())
    }
    let prefs=java_settings::read(&instance_id)?;
    java_settings::validate(prefs.xms_mb,prefs.xmx_mb,&prefs.java_path,
        &java_settings::system_memory()?)?;
    let required=tl_game::required_java(&version).map_err(|e|e.to_string())?;
    let identity=tl_game::vanilla::VanillaIdentity::from_offline_profile(&profile)
        .map_err(|e|e.to_string())?;
    let paths=tl_game::vanilla::VanillaPaths{
        root:dir.clone(),game_dir:game_dir_for(&dir),natives_dir:natives,
        java:java_settings::java_executable(&prefs)
    };
    let _command=tl_game::vanilla::build_vanilla_launch(&version,&paths,&identity,prefs.xms_mb,prefs.xmx_mb)
        .map_err(|e|e.to_string())?;
    Ok(LaunchPreflight{
        instance_id,java_path:paths.java.to_string_lossy().into_owned(),
        required_java:required,xms_mb:prefs.xms_mb,xmx_mb:prefs.xmx_mb
    })
}

#[tauri::command]
pub fn game_launch(app:AppHandle,state:State<'_,Arc<GameState>>,request:tl_core::LaunchRequest)->Result<StartedProcess,String>{
    let i=find_instance(request.instance_id)?;
    if !matches!(&i.state,InstanceState::Ready){return Err("Instance not READY; complete verified Vanilla install".into())}
    let profile:OfflineProfile=db()?.get_profile(request.profile_id).map_err(|e|e.to_string())?
        .ok_or("Persisted profile missing")?;
    tl_game::foundation::validate_launch_context(&i,&profile,&request).map_err(|e|e.to_string())?;
    let dir=root(&i)?;
    let prefs=java_settings::read(&i.id.to_string())?;
    if request.min_ram_mb.is_some_and(|v|v!=prefs.xms_mb) || request.max_ram_mb.is_some_and(|v|v!=prefs.xmx_mb) {
        return Err("RAM overrides must be saved per instance with revision/CAS first".into())
    }
    let selected_xms=prefs.xms_mb;
    let selected_xmx=prefs.xmx_mb;
    // Prevent requests from circumventing persisted Java safety constraints.
    java_settings::validate(selected_xms,selected_xmx,&prefs.java_path,&java_settings::system_memory()?)?;
    if request.java_path.as_ref().is_some_and(|path|path!=&prefs.java_path){
       return Err("Java overrides must be saved in Settings before launch".into())
    }
    if request.extra_jvm_args.as_ref().is_some_and(|extra|!extra.is_empty()){
       return Err("Custom JVM args are not permitted by the Desktop bridge".into())
    }
    let (version,natives)=installed_metadata(&dir)?;
    if version.get("id").and_then(Value::as_str)!=Some(i.minecraft_version.as_str()){
        return Err("Installed version does not match instance".into())
    }
    let identity=tl_game::vanilla::VanillaIdentity::from_offline_profile(&profile)
        .map_err(|e|e.to_string())?;
    let paths=tl_game::vanilla::VanillaPaths{root:dir.clone(),game_dir:game_dir_for(&dir),natives_dir:natives,java:java_settings::java_executable(&prefs)};
    let launch=tl_game::vanilla::build_vanilla_launch(&version,&paths,&identity,selected_xms,selected_xmx)
        .map_err(|e|e.to_string())?;
    let mut children=state.children.lock().map_err(|_|"Process lock poisoned")?;
    if let Some(existing)=children.get_mut(&i.id){
        if existing.try_wait().map_err(|e|e.to_string())?.is_none(){
            return Err("Minecraft is already running for this instance".into())
        }
        children.remove(&i.id);
    }
    let log_dir=dir.join("logs");
    fs::create_dir_all(&log_dir).map_err(|e|e.to_string())?;
    let launch_id=Uuid::new_v4();
    let stdout=log_dir.join(format!("minecraft-{launch_id}-stdout.log"));
    let stderr=log_dir.join(format!("minecraft-{launch_id}-stderr.log"));
    let child=tl_game::launch(&launch,&stdout,&stderr).map_err(|e|e.to_string())?;
    let pid=child.id();
    children.insert(i.id,child);
    let _=app.emit("game.started",json!({
        "contract_version":1,"operation_id":i.id.to_string(),
        "instance_id":i.id.to_string(),"timestamp":chrono::Utc::now().to_rfc3339(),
        "phase":"started","message_key":"Java process created; Minecraft menu not verified",
        "details":{"pid":pid},"percent":null
    }));
    Ok(StartedProcess{pid,started_at:chrono::Utc::now().to_rfc3339(),
        log_path:stdout.to_string_lossy().into_owned(),termination_state:"running"})
}
#[tauri::command]
pub fn game_stop(app:AppHandle,state:State<'_,Arc<GameState>>,instance_id:String)->Result<(),String>{
    let uuid=id(&instance_id)?;
    let mut children=state.children.lock().map_err(|_|"Process lock poisoned")?;
    let mut child=children.remove(&uuid).ok_or("No launcher-owned Minecraft process")?;
    if child.try_wait().map_err(|e|e.to_string())?.is_none(){
        child.kill().map_err(|e|e.to_string())?;
        child.wait().map_err(|e|e.to_string())?;
    }
    let _=app.emit("game.exited",json!({
      "contract_version":1,"operation_id":instance_id,
      "instance_id":uuid.to_string(),"timestamp":chrono::Utc::now().to_rfc3339(),
      "phase":"stopped","message_key":"Launcher-owned Java process terminated",
      "details":{},"percent":null
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_command_contains_configured_real_jvm_heap_flags() {
        // Exercise exact tl-game executable command builder, not a mocked UI string.
        let config=tl_game::LaunchConfig{
            java:PathBuf::from("java.exe"),
            classpath:vec![PathBuf::from("client.jar")],
            natives_dir:PathBuf::from("natives"),
            game_dir:PathBuf::from("game"),
            main_class:"net.minecraft.client.main.Main".to_owned(),
            jvm_args:vec![],
            game_args:vec![],
            min_ram_mb:768,
            max_ram_mb:3072,
        };
        let command=tl_game::build_command(&config).expect("valid launch");
        let args:Vec<String>=command.get_args().map(|a|a.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&"-Xms768M".to_owned()));
        assert!(args.contains(&"-Xmx3072M".to_owned()));
    }
    #[test]
    fn saved_world_path_stays_constant_across_native_reinstalls(){
        let root=PathBuf::from(r"C:\Users\Tester\AppData\Local\tlauchcraft\instances\123");
        let first=game_dir_for(&root).join("saves").join("OldWorld").join("level.dat");
        let second=game_dir_for(&root).join("saves").join("OldWorld").join("level.dat");
        assert_eq!(first,second);
        assert!(!first.to_string_lossy().contains("natives-"));
    }
    #[test]
    fn native_verifier_rejects_empty_folder(){
        let path=std::env::temp_dir().join(format!("tl-native-check-{}",Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        assert!(count_native_dlls(&path).is_err());
        fs::write(path.join("lwjgl.dll"),b"test dll").unwrap();
        assert_eq!(count_native_dlls(&path).unwrap(),1);
        fs::remove_dir_all(&path).unwrap();
    }
    #[test]
    fn rejects_jvm_memory_inversion_before_process_spawn() {
        let config=tl_game::LaunchConfig{
            java:PathBuf::from("java.exe"),
            classpath:vec![PathBuf::from("client.jar")],
            natives_dir:PathBuf::from("natives"),
            game_dir:PathBuf::from("game"),
            main_class:"Example".into(),
            jvm_args:vec![],game_args:vec![],
            min_ram_mb:4096,max_ram_mb:512
        };
        assert!(tl_game::build_command(&config).is_err());
    }
}
