//! Desktop bridge for local, opt-in cosmetic assets. No network or authentication.
use std::{fs, io::Read, path::{Path, PathBuf}, sync::Mutex};
use tl_core::{GameInstance, OfflineProfile};
use tl_skin::{Model, SkinLibrary, SkinRecord, Visibility, provider::{CustomSkinLoaderLocal, ProviderContext, ProviderMod, SkinProvider, configure_forge_local}};
use tl_storage::ProfileStore;
use uuid::Uuid;

static SKIN_LOCK: Mutex<()> = Mutex::new(());

fn open_library() -> Result<SkinLibrary,String> {
    let root=tl_storage::default_data_dir().map_err(|e|e.to_string())?.join("skins");
    let root=tl_storage::isolated_data_dir(&root).map_err(|e|e.to_string())?;
    SkinLibrary::open(root).map_err(|e|e.to_string())
}

#[tauri::command]
pub fn skin_list() -> Result<Vec<SkinRecord>,String> {
    let _guard=SKIN_LOCK.lock().map_err(|_|"Skin lock unavailable")?;
    Ok(open_library()?.list())
}

#[tauri::command]
pub fn skin_import(path:String,name:String,model:Model) -> Result<SkinRecord,String> {
    let _guard=SKIN_LOCK.lock().map_err(|_|"Skin lock unavailable")?;
    let path=PathBuf::from(path);
    if !path.is_absolute() || !path.is_file() {
        return Err("Select an existing PNG".into());
    }
    let mut bytes=Vec::new();
    fs::File::open(path).map_err(|e|e.to_string())?.take(2*1024*1024+1)
        .read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    open_library()?.import_png(&name,&bytes,model).map_err(|e|e.to_string())
}

#[tauri::command]
pub fn skin_preview(skin_id:Uuid) -> Result<Vec<u8>,String> {
    let _guard=SKIN_LOCK.lock().map_err(|_|"Skin lock unavailable")?;
    open_library()?.export_png(skin_id).map_err(|e|e.to_string())
}

#[tauri::command]
pub fn skin_select(profile_id:Uuid,skin_id:Uuid,model:Model,instance_id:Option<Uuid>) -> Result<(),String> {
    let _guard=SKIN_LOCK.lock().map_err(|_|"Skin lock unavailable")?;
    let mut store=super::open_store().map_err(|e|e.to_string())?;
    let mut profile=store.get_profile(profile_id).map_err(|e|e.to_string())?.ok_or("Profile not found")?;
    let mut library=open_library()?;
    if let Some(instance_id)=instance_id {
        use tl_storage::InstanceStore;
        let instance=store.get_instance(instance_id).map_err(|e|e.to_string())?.ok_or("Instance not found")?;
        if instance.profile_id!=profile_id { return Err("Instance/profile mismatch".into()); }
        return library.assign_instance_override(instance_id,tl_core::SkinSelection{
            profile_id,skin_id,model,cape_id:None
        }).map_err(|e|e.to_string());
    }
    library.assign_foundation_profile(&mut profile,skin_id,model).map_err(|e|e.to_string())?;
    profile.updated_at=chrono::Utc::now();
    store.update_profile(&profile).map_err(|e|format!("Skin selection needs reconciliation; reselect: {e}"))
}

pub fn prepare(profile:&OfflineProfile,instance:&GameInstance,dir:&Path,spec:tl_pack::skin_compat::Candidate) -> Result<String,String> {
    let _guard=SKIN_LOCK.lock().map_err(|_|"Skin lock unavailable")?;
    let library=open_library()?;
    library.validate_foundation_profile(profile).map_err(|e|e.to_string())?;
    let Some(selection)=library.selection(profile.id,Some(instance.id)) else {
        configure_forge_local(dir,&Model::Classic).map_err(|e|e.to_string())?;
        return Ok("PreviewOnly: no skin selected".into());
    };
    let ctx=ProviderContext {
        profile,instance,game_dir:dir,consent:true,
        client_mod:Some(ProviderMod{mod_id:"customskinloader",jar_filename:spec.filename,expected_sha256:spec.sha256})
    };
    let adapter=CustomSkinLoaderLocal;
    let report=adapter.stage(&library,&ctx).map_err(|e|e.to_string())?;
    if report.status!=Visibility::ProviderReadyUnverified {
        return Err(format!("Skin provider not ready: {:?}",report.reason));
    }
    configure_forge_local(dir,&selection.model).map_err(|e|e.to_string())?;
    Ok("ProviderReadyUnverified: local PNG and provider prepared; inspect F5 in game".into())
}
