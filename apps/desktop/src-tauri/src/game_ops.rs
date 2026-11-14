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
    if !matches!(i.loader.kind,LoaderKind::Vanilla){
        return Err("Forge/Fabric/modpack installation is not supported by the Vanilla bridge".into())
    }
    Ok(base.join(&i.root_path))
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
fn event(app:&AppHandle,instance:Uuid,phase:&str,key:&str){
    // Progress percentage is null until tl-game supports trustworthy byte counters.
    let _=app.emit("download.progress",json!({
        "contract_version":1,"operation_id":instance.to_string(),
        "instance_id":instance.to_string(),
        "timestamp":chrono::Utc::now().to_rfc3339(),
        "phase":phase,"message_key":key,"details":{},"percent":null
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
    if cancel.load(Ordering::Relaxed){return Err("Install cancelled".into())}
    update_state(uuid,InstanceState::Verifying)?;
    event(&app,uuid,"verifying","Extracting verified native DLLs");
    // Unique native path prevents overwriting any existing game files/worlds.
    let natives=dir.join(format!("natives-{}",Uuid::new_v4()));
    tl_game::vanilla::extract_natives(&installed.native_archives,&natives).map_err(|e|e.to_string())?;
    if cancel.load(Ordering::Relaxed){return Err("Install cancelled".into())}
    fs::create_dir_all(dir.join("game")).map_err(|e|e.to_string())?;
    let metadata=json!({"version":version,"natives_dir":natives});
    let tmp=dir.join(format!(".vanilla-install-{}.tmp",Uuid::new_v4()));
    fs::write(&tmp,serde_json::to_vec(&metadata).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    fs::rename(&tmp,dir.join("vanilla-install.json")).map_err(|e|e.to_string())?;
    update_state(uuid,InstanceState::Ready)?;
    event(&app,uuid,"ready","Vanilla files verified; launch prerequisites still checked on Play");
    Ok(())
}
#[tauri::command]
pub fn game_install(app:AppHandle,state:State<'_,Arc<GameState>>,instance_id:String)->Result<String,String>{
    let uuid=id(&instance_id)?;
    let i=find_instance(uuid)?;
    root(&i)?;
    if !matches!(i.state,InstanceState::Empty|InstanceState::Failed|InstanceState::Cancelled|InstanceState::NeedsAction){
        return Err(format!("Cannot install instance in state {:?}",i.state))
    }
    let mut installs=state.installs.lock().map_err(|_|"Install lock poisoned")?;
    if installs.contains_key(&uuid){return Err("Install already in progress".into())}
    update_state(uuid,InstanceState::Planned)?;
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
            let is_cancelled=matches!(status,InstanceState::Cancelled);
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
    let i=find_instance(uuid)?;
    let installing=state.installs.lock().map_err(|_|"Install lock poisoned")?.contains_key(&uuid);
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
    Ok(GameStatus{instance_id,install_state:i.state,installing,pid,process_state})
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
    Ok((version,natives))
}
#[tauri::command]
pub fn game_launch(app:AppHandle,state:State<'_,Arc<GameState>>,request:tl_core::LaunchRequest)->Result<StartedProcess,String>{
    let i=find_instance(request.instance_id)?;
    if i.state!=InstanceState::Ready{return Err("Instance not READY; complete verified Vanilla install".into())}
    let profile:OfflineProfile=db()?.get_profile(request.profile_id).map_err(|e|e.to_string())?
        .ok_or("Persisted profile missing")?;
    tl_game::foundation::validate_launch_context(&i,&profile,&request).map_err(|e|e.to_string())?;
    let dir=root(&i)?;
    let prefs=java_settings::read(&i.id.to_string())?;
    let selected_xms=request.min_ram_mb.unwrap_or(prefs.xms_mb);
    let selected_xmx=request.max_ram_mb.unwrap_or(prefs.xmx_mb);
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
    let paths=tl_game::vanilla::VanillaPaths{root:dir.clone(),game_dir:dir.join("game"),natives_dir:natives,java:java_settings::java_executable(&prefs)};
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
    let stdout=log_dir.join("minecraft-stdout.log");
    let stderr=log_dir.join("minecraft-stderr.log");
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
pub fn game_stop(state:State<'_,Arc<GameState>>,instance_id:String)->Result<(),String>{
    let uuid=id(&instance_id)?;
    let mut children=state.children.lock().map_err(|_|"Process lock poisoned")?;
    let mut child=children.remove(&uuid).ok_or("No launcher-owned Minecraft process")?;
    if child.try_wait().map_err(|e|e.to_string())?.is_none(){
        child.kill().map_err(|e|e.to_string())?;
        child.wait().map_err(|e|e.to_string())?;
    }
    Ok(())
}
