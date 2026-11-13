//! Converts Mojang version metadata to an explicit, auditable download plan.
//! URLs are taken from official metadata; caller must still validate origin before download.
use crate::{asset_object_path, libraries, Environment, GameError};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileRole { ClientJar, Library, NativeArchive, AssetIndex, AssetObject, LoggingConfig }
#[derive(Clone, Debug)]
pub struct PlannedFile {
    pub role: FileRole,
    pub url: String,
    pub sha1: String,
    pub size: Option<u64>,
    pub target: PathBuf,
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, GameError> {
    value.get(field).and_then(Value::as_str).ok_or_else(|| GameError::InvalidInput(format!("missing {field}")))
}
fn entry(obj: &Value, role: FileRole, target: PathBuf) -> Result<PlannedFile, GameError> {
    let url = text(obj,"url")?.to_owned();
    let sha1 = text(obj,"sha1")?.to_ascii_lowercase();
    if sha1.len()!=40 || !sha1.bytes().all(|b|b.is_ascii_hexdigit()) {return Err(GameError::InvalidInput("invalid metadata SHA1".into()));}
    let size = obj.get("size").and_then(Value::as_u64);
    Ok(PlannedFile {role,url,sha1,size,target})
}
fn safe_component(s: &str) -> Result<&str, GameError> {
    if s.is_empty() || s=="." || s==".." || s.contains('/') || s.contains('\\') || s.contains(':') || s.chars().any(char::is_control) {
        return Err(GameError::InvalidInput(format!("unsafe path component: {s}")));
    }
    Ok(s)
}
fn safe_relative(s: &str) -> Result<PathBuf, GameError> {
    let mut path=PathBuf::new();
    for part in s.split('/') { path.push(safe_component(part)?); }
    Ok(path)
}
fn maven_path(name: &str) -> Result<PathBuf, GameError> {
    let parts: Vec<_>=name.split(':').collect();
    if parts.len()<3 || parts.len()>4 {return Err(GameError::InvalidInput(format!("invalid Maven coordinates: {name}")));}
    let group=parts[0].split('.').map(safe_component).collect::<Result<Vec<_>,_>>()?;
    let artifact=safe_component(parts[1])?;
    let version=safe_component(parts[2])?;
    let classifier=if parts.len()==4 {format!("-{}",safe_component(parts[3])?)} else {String::new()};
    let mut p=PathBuf::new();
    for part in group {p.push(part);}
    p.push(artifact);p.push(version);
    p.push(format!("{artifact}-{version}{classifier}.jar"));
    Ok(p)
}
/// Vanilla client downloads, libraries and asset index; asset objects are planned separately.
pub fn plan_version(version: &Value, root: &Path, env: &Environment) -> Result<Vec<PlannedFile>, GameError> {
    let id=safe_component(text(version,"id")?)?;
    let mut plan=Vec::new();
    if let Some(client)=version.get("downloads").and_then(|v|v.get("client")) {
        plan.push(entry(client,FileRole::ClientJar,root.join("versions").join(id).join(format!("{id}.jar")))?);
    }
    if let Some(index)=version.get("assetIndex") {
        let asset_id=safe_component(text(index,"id")?)?;
        plan.push(entry(index,FileRole::AssetIndex,root.join("assets/indexes").join(format!("{asset_id}.json")))?);
    }
    if let Some(config)=version.get("logging").and_then(|v|v.get("client")).and_then(|v|v.get("file")) {
        let id=safe_component(text(config,"id")?)?;
        plan.push(entry(config,FileRole::LoggingConfig,root.join("assets/log_configs").join(id))?);
    }
    for lib in libraries(version,env) {
        let name=text(&lib,"name")?;
        let fallback=maven_path(name)?;
        let downloads=lib.get("downloads");
        if let Some(artifact)=downloads.and_then(|v|v.get("artifact")) {
            let path=artifact.get("path").and_then(Value::as_str).map(safe_relative).transpose()?.unwrap_or(fallback);
            plan.push(entry(artifact,FileRole::Library,root.join("libraries").join(path))?);
        }
        if let Some(natives)=lib.get("natives").and_then(Value::as_object) {
            if let Some(template)=natives.get(&env.os).and_then(Value::as_str) {
                let arch=if env.arch=="x86_64" {"64"} else {"32"};
                let classifier=template.replace("${arch}",arch);
                if let Some(native)=downloads.and_then(|v|v.get("classifiers")).and_then(|v|v.get(&classifier)) {
                    let path=native.get("path").and_then(Value::as_str).map(safe_relative).transpose()?.ok_or_else(||GameError::InvalidInput("native classifier lacks path".into()))?;
                    plan.push(entry(native,FileRole::NativeArchive,root.join("libraries").join(path))?);
                }
            }
        }
    }
    Ok(plan)
}
pub fn plan_asset_objects(index: &Value, root: &Path) -> Result<Vec<PlannedFile>, GameError> {
    let objects=index.get("objects").and_then(Value::as_object).ok_or_else(||GameError::InvalidInput("asset index objects missing".into()))?;
    let mut result=Vec::new();
    for asset in objects.values() {
        let sha1=text(asset,"hash")?;
        let target=asset_object_path(root,sha1)?;
        let url=format!("https://resources.download.minecraft.net/{}/{}", &sha1[..2],sha1);
        result.push(PlannedFile{role:FileRole::AssetObject,url,sha1:sha1.into(),size:asset.get("size").and_then(Value::as_u64),target});
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;use serde_json::json;use std::collections::HashMap;
    fn env()->Environment {Environment{os:"windows".into(),arch:"x86_64".into(),os_version:"".into(),features:HashMap::new()}}
    #[test] fn plan_client_assets_libs() {
        let sha="0123456789abcdef0123456789abcdef01234567";
        let json=json!({"id":"1.20.1","downloads":{"client":{"url":"https://piston-data.mojang.com/a","sha1":sha}},
          "assetIndex":{"id":"5","url":"https://piston-meta.mojang.com/assets","sha1":sha},
          "libraries":[{"name":"org.example:lib:1","downloads":{"artifact":{"url":"https://libraries.minecraft.net/a","sha1":sha,"path":"org/example/lib/1/lib-1.jar"}}}]});
        let p=plan_version(&json,Path::new("C:/Game"),&env()).unwrap();
        assert_eq!(p.len(),3);
        assert!(p[2].target.to_string_lossy().contains("libraries"));
    }
    #[test] fn no_traversal() {
        assert!(safe_relative("../outside").is_err());
        assert!(safe_relative("a/../../b").is_err());
        assert!(maven_path("bad/thing:a:1").is_err());
    }
    #[test] fn asset_layout() {
        let id="0123456789abcdef0123456789abcdef01234567";
        let p=plan_asset_objects(&json!({"objects":{"stone":{"hash":id,"size":9}}}),Path::new("/tmp/mc")).unwrap();
        assert_eq!(p[0].url,format!("https://resources.download.minecraft.net/01/{id}"));
    }
}
