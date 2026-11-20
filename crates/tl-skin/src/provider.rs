//! Explicit client-side skin provider boundary. Preparing PNG bytes is never proof of rendering.
//! No game JAR modification, mod installation, network requests, world writes or LAN transport.
use crate::{decode_skin, digest, Model, SkinError, SkinLibrary, Visibility};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tl_core::{derive_vanilla_offline_identity, GameInstance, LoaderKind, OfflineProfile};
use uuid::Uuid;

const MOD_ID: &str = "customskinloader";
const MAX_MOD_BYTES: u64 = 64 * 1024 * 1024;

/// A mod artifact independently obtained and approved by the user.
/// A matching JAR hash proves local artifact integrity, NOT that Minecraft loaded the mod.
#[derive(Debug, Clone)]
pub struct ProviderMod<'a> {
    pub mod_id: &'a str,
    pub jar_filename: &'a str,
    pub expected_sha256: &'a str,
}
#[derive(Debug)]
pub struct ProviderContext<'a> {
    pub profile: &'a OfflineProfile,
    pub instance: &'a GameInstance,
    /// Actual isolated game working directory, NOT shared .minecraft or the world directory.
    pub game_dir: &'a Path,
    pub consent: bool,
    pub client_mod: Option<ProviderMod<'a>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderReason {
    ConsentRequired,
    UnsupportedVanilla,
    UnsupportedVersionOrLoader,
    SlimModelNotConfirmed,
    ProfileOrInstanceMismatch,
    SelectionMissing,
    ModMissingOrUntrusted,
    UnsafeGameDirectory,
    AssetConflict,
    ReadyUnverified,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReport {
    pub status: Visibility,
    pub reason: ProviderReason,
    pub provider_id: String,
    pub profile_id: Uuid,
    pub instance_id: Uuid,
    pub skin_id: Option<Uuid>,
    pub model: Option<Model>,
    pub target: Option<PathBuf>,
    pub image_sha256: Option<String>,
}
impl ProviderReport {
    fn preview(ctx: &ProviderContext<'_>, reason: ProviderReason) -> Self {
        Self {
            status: Visibility::PreviewOnly,
            reason,
            provider_id: MOD_ID.to_owned(),
            profile_id: ctx.profile.id,
            instance_id: ctx.instance.id,
            skin_id: None,
            model: None,
            target: None,
            image_sha256: None,
        }
    }
}

/// A plan/check is read-only; stage writes a selected PNG ONLY after explicit consent.
/// Neither operation may elevate status to LocalVerified or PeerVerified.
pub trait SkinProvider {
    fn check(&self, library: &SkinLibrary, ctx: &ProviderContext<'_>) -> ProviderReport;
    fn stage(
        &self,
        library: &SkinLibrary,
        ctx: &ProviderContext<'_>,
    ) -> crate::Result<ProviderReport>;
}
#[derive(Debug, Default, Clone, Copy)]
pub struct CustomSkinLoaderLocal;

#[derive(Debug, Serialize, Deserialize)]
struct Receipt {
    profile_id: Uuid,
    instance_id: Uuid,
    username: String,
    previous_sha256: String,
}
fn safe_component(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'.' | b'_' | b'-'))
        && name != "."
        && name != ".."
        && !name.contains("..")
}
fn no_symlink_ancestors(path: &Path) -> bool {
    path.ancestors()
        .all(|ancestor| match fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return false;
                    }
                }
                !meta.file_type().is_symlink()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => false,
        })
}
fn safe_username_file(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ![
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&upper.as_str())
}

fn acceptable_game_dir(ctx: &ProviderContext<'_>) -> bool {
    ctx.game_dir.is_absolute() && ctx.game_dir.is_dir() && no_symlink_ancestors(ctx.game_dir)
}
fn verified_mod(ctx: &ProviderContext<'_>) -> bool {
    let Some(ref spec) = ctx.client_mod else {
        return false;
    };
    if spec.mod_id != MOD_ID
        || !safe_component(spec.jar_filename)
        || !spec.jar_filename.to_ascii_lowercase().ends_with(".jar")
        || spec.expected_sha256.len() != 64
        || !spec
            .expected_sha256
            .bytes()
            .all(|ch| ch.is_ascii_hexdigit())
    {
        return false;
    }
    let path = ctx.game_dir.join("mods").join(spec.jar_filename);
    if !no_symlink_ancestors(&path) || !path.is_file() {
        return false;
    }
    let Ok(meta) = fs::metadata(&path) else {
        return false;
    };
    if !(4..=MAX_MOD_BYTES).contains(&meta.len()) {
        return false;
    }
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    // ZIP/JAR signature is a sanity check only, not full mod identity verification.
    bytes.starts_with(b"PK\x03\x04") && digest(&bytes).eq_ignore_ascii_case(spec.expected_sha256)
}
fn target(ctx: &ProviderContext<'_>) -> PathBuf {
    ctx.game_dir
        .join("CustomSkinLoader")
        .join("LocalSkin")
        .join("skins")
        .join(format!("{}.png", ctx.profile.username))
}
fn receipt_path(ctx: &ProviderContext<'_>) -> PathBuf {
    ctx.game_dir
        .join("CustomSkinLoader")
        .join("LocalSkin")
        .join(".tlauchcraft")
        .join(format!("{}-{}.json", ctx.instance.id, ctx.profile.id))
}
fn checked_receipt(
    ctx: &ProviderContext<'_>,
    destination: &Path,
) -> crate::Result<Option<Receipt>> {
    let path = receipt_path(ctx);
    if !no_symlink_ancestors(&path) || !no_symlink_ancestors(destination) {
        return Err(SkinError::InvalidInput(
            "unsafe symlink in cosmetic path".into(),
        ));
    }
    if !path.exists() {
        if destination.exists() {
            return Err(SkinError::InvalidInput(
                "existing unmanaged local skin; refusing overwrite".into(),
            ));
        }
        return Ok(None);
    }
    if fs::metadata(&path)?.len() > 16 * 1024 {
        return Err(SkinError::InvalidInput("oversized skin receipt".into()));
    }
    let receipt: Receipt = serde_json::from_slice(&fs::read(path)?)?;
    if receipt.profile_id != ctx.profile.id
        || receipt.instance_id != ctx.instance.id
        || receipt.username != ctx.profile.username
        || !destination.is_file()
        || fs::metadata(destination)?.len() > 2 * 1024 * 1024
        || digest(&fs::read(destination)?) != receipt.previous_sha256
    {
        return Err(SkinError::InvalidInput(
            "managed skin changed externally or profile renamed".into(),
        ));
    }
    Ok(Some(receipt))
}
fn write_new(path: &Path, contents: &[u8]) -> crate::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| SkinError::InvalidInput("missing parent".into()))?;
    if !no_symlink_ancestors(parent) || !no_symlink_ancestors(path) {
        return Err(SkinError::InvalidInput("unsafe cosmetic path".into()));
    }
    fs::create_dir_all(parent)?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    if let Err(e) = file.write_all(contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(e.into());
    }
    Ok(())
}
impl SkinProvider for CustomSkinLoaderLocal {
    fn check(&self, library: &SkinLibrary, ctx: &ProviderContext<'_>) -> ProviderReport {
        let fail = |reason| ProviderReport::preview(ctx, reason);
        if !ctx.consent {
            return fail(ProviderReason::ConsentRequired);
        }
        if ctx.instance.profile_id != ctx.profile.id
            || ctx.instance.validate().is_err()
            || library.validate_foundation_profile(ctx.profile).is_err()
        {
            return fail(ProviderReason::ProfileOrInstanceMismatch);
        }
        if ctx.instance.loader.kind == LoaderKind::Vanilla {
            return fail(ProviderReason::UnsupportedVanilla);
        }
        let fabric = ctx.instance.loader.kind == LoaderKind::Fabric
            && ctx.instance.minecraft_version == "1.21.1";
        let forge = ctx.instance.loader.kind == LoaderKind::Forge
            && matches!(ctx.instance.minecraft_version.as_str(), "1.16.5" | "1.20.1");
        if !fabric && !forge {
            return fail(ProviderReason::UnsupportedVersionOrLoader);
        }
        if !acceptable_game_dir(ctx) || !safe_username_file(&ctx.profile.username) {
            return fail(ProviderReason::UnsafeGameDirectory);
        }
        let Some(selection) = library.selection(ctx.profile.id, Some(ctx.instance.id)) else {
            return fail(ProviderReason::SelectionMissing);
        };
        // Forge's explicit local-only Legacy config selects default/slim for the
        // active profile; Fabric keeps the original conservative Classic gate.
        if fabric && selection.model == Model::Slim {
            return fail(ProviderReason::SlimModelNotConfirmed);
        }
        let Ok(bytes) = library.export_png(selection.skin_id) else {
            return fail(ProviderReason::SelectionMissing);
        };
        let Ok(image) = decode_skin(&bytes) else {
            return fail(ProviderReason::SelectionMissing);
        };
        if selection.model == Model::Slim && image.height != 64 {
            return fail(ProviderReason::SlimModelNotConfirmed);
        }
        if !verified_mod(ctx) {
            return fail(ProviderReason::ModMissingOrUntrusted);
        }
        let destination = target(ctx);
        if checked_receipt(ctx, &destination).is_err() {
            return fail(ProviderReason::AssetConflict);
        }
        ProviderReport {
            status: Visibility::ProviderReadyUnverified,
            reason: ProviderReason::ReadyUnverified,
            provider_id: MOD_ID.to_owned(),
            profile_id: ctx.profile.id,
            instance_id: ctx.instance.id,
            skin_id: Some(selection.skin_id),
            model: Some(selection.model.clone()),
            target: Some(destination),
            image_sha256: Some(digest(&bytes)),
        }
    }

    fn stage(
        &self,
        library: &SkinLibrary,
        ctx: &ProviderContext<'_>,
    ) -> crate::Result<ProviderReport> {
        let report = self.check(library, ctx);
        if report.status != Visibility::ProviderReadyUnverified {
            return Ok(report);
        }
        let dst = report
            .target
            .as_ref()
            .ok_or_else(|| SkinError::InvalidInput("missing target".into()))?;
        let selected = library
            .selection(ctx.profile.id, Some(ctx.instance.id))
            .ok_or(SkinError::NotFound)?;
        let bytes = library.export_png(selected.skin_id)?;
        let original = checked_receipt(ctx, dst)?;
        let new_sha = digest(&bytes);
        if original
            .as_ref()
            .is_some_and(|r| r.previous_sha256 == new_sha)
        {
            return Ok(report);
        }
        // Never overwrite a user's pre-existing texture. Updates are limited to files
        // whose last staged hash matches our own receipt, with rollback on write failure.
        let nonce = Uuid::new_v4();
        let tmp = dst.with_extension(format!("{nonce}.tmp"));
        write_new(&tmp, &bytes)?;
        let backup = dst.with_extension(format!("{nonce}.bak"));
        if original.is_some() {
            if let Err(e) = fs::rename(dst, &backup) {
                let _ = fs::remove_file(&tmp);
                return Err(e.into());
            }
        }
        if let Err(e) = fs::rename(&tmp, dst) {
            if original.is_some() {
                let _ = fs::rename(&backup, dst);
            }
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        let receipt = Receipt {
            profile_id: ctx.profile.id,
            instance_id: ctx.instance.id,
            username: ctx.profile.username.clone(),
            previous_sha256: new_sha,
        };
        let receipt_file = receipt_path(ctx);
        // Stage receipts without clobbering an unknown existing file.
        let receipt_tmp = receipt_file.with_extension(format!("{nonce}.tmp"));
        let receipt_bytes = serde_json::to_vec_pretty(&receipt)?;
        let save_result = (|| -> crate::Result<()> {
            write_new(&receipt_tmp, &receipt_bytes)?;
            if original.is_some() {
                let receipt_backup = receipt_file.with_extension(format!("{nonce}.bak"));
                fs::rename(&receipt_file, &receipt_backup)?;
                if let Err(e) = fs::rename(&receipt_tmp, &receipt_file) {
                    let _ = fs::rename(&receipt_backup, &receipt_file);
                    return Err(e.into());
                }
                let _ = fs::remove_file(receipt_backup);
            } else {
                fs::rename(&receipt_tmp, &receipt_file)?;
            }
            Ok(())
        })();
        if let Err(e) = save_result {
            let _ = fs::remove_file(&receipt_tmp);
            let _ = fs::remove_file(dst);
            if original.is_some() {
                let _ = fs::rename(&backup, dst);
            }
            return Err(e);
        }
        if original.is_some() {
            let _ = fs::remove_file(backup);
        }
        Ok(report)
    }
}

/// Typed, read-only predicted identity for diagnostic display. No game login claim.
pub fn predicted_game_uuid(profile: &OfflineProfile) -> crate::Result<Uuid> {
    derive_vanilla_offline_identity(profile)
        .map(|identity| identity.offline_server_uuid)
        .map_err(|_| SkinError::InvalidInput("invalid Foundation username".into()))
}

const CONFIG_NAME: &str = "TLaunchCraftLocal";
const CONFIG_SKIN: &str = "LocalSkin/skins/{USERNAME}.png";

/// Called only after the Forge provider has passed the trusted installed receipt gate.
/// CSL rewrites its config on boot; accept only our single local source, not a byte hash.
pub fn configure_forge_local(game_dir: &Path, model: &Model) -> crate::Result<()> {
    use serde_json::{json, Value};
    let root = game_dir.join("CustomSkinLoader");
    let config = root.join("CustomSkinLoader.json");
    let extras = root.join("ExtraList");
    let marker = root.join(".tlauchcraft-config");
    if !game_dir.is_absolute()
        || !no_symlink_ancestors(&config)
        || !no_symlink_ancestors(&extras)
        || !no_symlink_ancestors(&marker)
        || (extras.exists() && fs::read_dir(&extras)?.next().is_some())
    {
        return Err(SkinError::InvalidInput(
            "unsafe or conflicting skin configuration".into(),
        ));
    }
    let model = match model {
        Model::Classic => "default",
        Model::Slim => "slim",
    };
    let mut value = if config.exists() {
        if !marker.is_file()
            || fs::metadata(&marker)?.len() != 20
            || fs::read(&marker)? != b"tlauchcraft-local/v1"
            || fs::metadata(&config)?.len() > 64 * 1024
        {
            return Err(SkinError::InvalidInput(
                "unmanaged skin configuration".into(),
            ));
        }
        let value: Value = serde_json::from_slice(&fs::read(&config)?)?;
        let sources = value["loadlist"].as_array().ok_or_else(|| {
            SkinError::InvalidInput("invalid managed skin configuration".into())
        })?;
        if sources.len() != 1
            || sources[0]["name"] != CONFIG_NAME
            || sources[0]["type"] != "Legacy"
            || sources[0]["skin"] != CONFIG_SKIN
            || !sources[0]["cape"].is_null()
            || !sources[0]["elytra"].is_null()
        {
            return Err(SkinError::InvalidInput(
                "managed skin source was changed".into(),
            ));
        }
        value
    } else {
        json!({"loadlist": []})
    };
    value["loadlist"] = json!([{
        "name": CONFIG_NAME,
        "type": "Legacy",
        "skin": CONFIG_SKIN,
        "model": model,
        "checkPNG": false
    }]);
    value["enableCape"] = json!(false);
    value["forceLoadAllTextures"] = json!(false);
    value["enableLocalProfileCache"] = json!(false);
    value["cacheExpiry"] = json!(30);
    if marker.exists() {
        if fs::metadata(&marker)?.len() != 20 || fs::read(&marker)? != b"tlauchcraft-local/v1" {
            return Err(SkinError::InvalidInput("invalid skin config marker".into()));
        }
    } else {
        write_new(&marker, b"tlauchcraft-local/v1")?;
    }
    if config.exists() && fs::read(&config)? == serde_json::to_vec_pretty(&value)? {
        return Ok(());
    }
    crate::atomic_write(&config, &serde_json::to_vec_pretty(&value)?)
}
