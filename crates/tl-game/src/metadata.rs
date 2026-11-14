//! Converts Mojang version metadata to an explicit, auditable download plan.
//! URLs are taken from official metadata; caller must still validate origin before download.
use crate::{asset_object_path, libraries, Environment, GameError};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileRole {
    ClientJar,
    Library,
    NativeArchive,
    AssetIndex,
    AssetObject,
    LoggingConfig,
}
#[derive(Clone, Debug)]
pub struct PlannedFile {
    pub role: FileRole,
    pub url: String,
    pub sha1: String,
    pub size: Option<u64>,
    pub target: PathBuf,
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, GameError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| GameError::InvalidInput(format!("missing {field}")))
}
fn entry(obj: &Value, role: FileRole, target: PathBuf) -> Result<PlannedFile, GameError> {
    let url = text(obj, "url")?.to_owned();
    let sha1 = text(obj, "sha1")?.to_ascii_lowercase();
    if sha1.len() != 40 || !sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(GameError::InvalidInput("invalid metadata SHA1".into()));
    }
    let size = obj.get("size").and_then(Value::as_u64);
    Ok(PlannedFile {
        role,
        url,
        sha1,
        size,
        target,
    })
}
fn safe_component(s: &str) -> Result<&str, GameError> {
    if s.is_empty()
        || s == "."
        || s == ".."
        || s.contains('/')
        || s.contains('\\')
        || s.contains(':')
        || s.chars().any(char::is_control)
    {
        return Err(GameError::InvalidInput(format!(
            "unsafe path component: {s}"
        )));
    }
    Ok(s)
}
fn safe_relative(s: &str) -> Result<PathBuf, GameError> {
    let mut path = PathBuf::new();
    for part in s.split('/') {
        path.push(safe_component(part)?);
    }
    Ok(path)
}
fn maven_path(name: &str) -> Result<PathBuf, GameError> {
    let parts: Vec<_> = name.split(':').collect();
    if parts.len() < 3 || parts.len() > 4 {
        return Err(GameError::InvalidInput(format!(
            "invalid Maven coordinates: {name}"
        )));
    }
    let group = parts[0]
        .split('.')
        .map(safe_component)
        .collect::<Result<Vec<_>, _>>()?;
    let artifact = safe_component(parts[1])?;
    let version = safe_component(parts[2])?;
    let classifier = if parts.len() == 4 {
        format!("-{}", safe_component(parts[3])?)
    } else {
        String::new()
    };
    let mut p = PathBuf::new();
    for part in group {
        p.push(part);
    }
    p.push(artifact);
    p.push(version);
    p.push(format!("{artifact}-{version}{classifier}.jar"));
    Ok(p)
}
/// Mojang modern native archives are separate Maven artifacts whose fourth
/// coordinate is an exact platform classifier. Do not treat every artifact with
/// 'native' somewhere in the name as a native or include native-only jars on the
/// Java classpath. Legacy 'natives' + 'classifiers' uses a separate path below.
fn maven_native_classifier(name: &str) -> Option<&str> {
    name.split(':').nth(3).filter(|v| v.starts_with("natives-"))
}

fn native_classifier_matches(classifier: &str, env: &Environment) -> bool {
    matches!(
        (env.os.as_str(), env.arch.as_str(), classifier),
        ("windows", "x86_64", "natives-windows")
            | ("windows", "x86", "natives-windows-x86")
            | ("windows", "arm64", "natives-windows-arm64")
            | ("linux", "x86_64", "natives-linux")
            | ("linux", "arm64", "natives-linux-arm64")
            | ("osx", "x86_64", "natives-macos")
            | ("osx", "arm64", "natives-macos-arm64")
    )
}

/// Vanilla client downloads, libraries and asset index; asset objects are planned separately.
pub fn plan_version(
    version: &Value,
    root: &Path,
    env: &Environment,
) -> Result<Vec<PlannedFile>, GameError> {
    let id = safe_component(text(version, "id")?)?;
    let mut plan = Vec::new();
    if let Some(client) = version.get("downloads").and_then(|v| v.get("client")) {
        plan.push(entry(
            client,
            FileRole::ClientJar,
            root.join("versions").join(id).join(format!("{id}.jar")),
        )?);
    }
    if let Some(index) = version.get("assetIndex") {
        let asset_id = safe_component(text(index, "id")?)?;
        plan.push(entry(
            index,
            FileRole::AssetIndex,
            root.join("assets/indexes").join(format!("{asset_id}.json")),
        )?);
    }
    if let Some(config) = version
        .get("logging")
        .and_then(|v| v.get("client"))
        .and_then(|v| v.get("file"))
    {
        let id = safe_component(text(config, "id")?)?;
        plan.push(entry(
            config,
            FileRole::LoggingConfig,
            root.join("assets/log_configs").join(id),
        )?);
    }
    for lib in libraries(version, env) {
        let name = text(&lib, "name")?;
        let fallback = maven_path(name)?;
        let downloads = lib.get("downloads");
        if let Some(artifact) = downloads.and_then(|v| v.get("artifact")) {
            let native = maven_native_classifier(name);
            // Mojang's OS rules can allow all Windows native architecture
            // variants. Select exactly the variant matching this host.
            if native.is_none_or(|classifier| native_classifier_matches(classifier, env)) {
                let path = artifact
                    .get("path")
                    .and_then(Value::as_str)
                    .map(safe_relative)
                    .transpose()?
                    .unwrap_or(fallback);
                plan.push(entry(
                    artifact,
                    if native.is_some() {
                        FileRole::NativeArchive
                    } else {
                        FileRole::Library
                    },
                    root.join("libraries").join(path),
                )?);
            }
        }
        if let Some(natives) = lib.get("natives").and_then(Value::as_object) {
            if let Some(template) = natives.get(&env.os).and_then(Value::as_str) {
                let arch = if env.arch == "x86_64" { "64" } else { "32" };
                let classifier = template.replace("${arch}", arch);
                if let Some(native) = downloads
                    .and_then(|v| v.get("classifiers"))
                    .and_then(|v| v.get(&classifier))
                {
                    let path = native
                        .get("path")
                        .and_then(Value::as_str)
                        .map(safe_relative)
                        .transpose()?
                        .ok_or_else(|| {
                            GameError::InvalidInput("native classifier lacks path".into())
                        })?;
                    plan.push(entry(
                        native,
                        FileRole::NativeArchive,
                        root.join("libraries").join(path),
                    )?);
                }
            }
        }
    }
    Ok(plan)
}
pub fn plan_asset_objects(index: &Value, root: &Path) -> Result<Vec<PlannedFile>, GameError> {
    let objects = index
        .get("objects")
        .and_then(Value::as_object)
        .ok_or_else(|| GameError::InvalidInput("asset index objects missing".into()))?;
    let mut result = Vec::new();
    for asset in objects.values() {
        let sha1 = text(asset, "hash")?;
        let target = asset_object_path(root, sha1)?;
        let url = format!(
            "https://resources.download.minecraft.net/{}/{}",
            &sha1[..2],
            sha1
        );
        result.push(PlannedFile {
            role: FileRole::AssetObject,
            url,
            sha1: sha1.into(),
            size: asset.get("size").and_then(Value::as_u64),
            target,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    fn env() -> Environment {
        Environment {
            os: "windows".into(),
            arch: "x86_64".into(),
            os_version: "".into(),
            features: HashMap::new(),
        }
    }
    fn lwjgl_native(name: &str, suffix: &str) -> Value {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        json!({
            "name": name,
            "downloads": {"artifact": {
                "path": format!("org/lwjgl/lwjgl-freetype/3.3.3/lwjgl-freetype-3.3.3-{suffix}.jar"),
                "url": format!("https://libraries.minecraft.net/org/lwjgl/lwjgl-freetype/3.3.3/lwjgl-freetype-3.3.3-{suffix}.jar"),
                "sha1": sha,
                "size": 1035586
            }},
            "rules": [{"action":"allow","os":{"name":"windows"}}]
        })
    }

    #[test]
    fn actual_1211_lwjgl_fixture_keeps_only_selected_arch_natives() {
        // Structure and SHA1 values copied from a 1.21.1 version metadata
        // snapshot, not fabricated runtime download outcomes.
        let version: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/mojang_1_21_1_lwjgl_windows.json"
        ))
        .unwrap();
        for (arch, suffix) in [
            ("x86_64", "natives-windows"),
            ("x86", "natives-windows-x86"),
            ("arm64", "natives-windows-arm64"),
        ] {
            let mut host = env();
            host.arch = arch.into();
            let files = plan_version(&version, Path::new("C:/E2E"), &host).unwrap();
            let normals: Vec<_> = files
                .iter()
                .filter(|file| file.role == FileRole::Library)
                .collect();
            let natives: Vec<_> = files
                .iter()
                .filter(|file| file.role == FileRole::NativeArchive)
                .collect();
            assert_eq!(normals.len(), 3, "normal jars for {arch}");
            assert_eq!(natives.len(), 3, "only selected native jars for {arch}");
            for native in natives {
                assert!(
                    native
                        .target
                        .to_string_lossy()
                        .ends_with(&format!("-{suffix}.jar")),
                    "wrong architecture jar: {}",
                    native.target.display()
                );
            }
        }
    }

    #[test]
    fn modern_lwjgl_natives_1211_exact_architecture_and_classpath() {
        // Mojang 1.21.1 structure: standalone 4-coordinate Maven artifacts
        // with downloads.artifact, plus Windows-only rules (not arch rules).
        let mut libraries = vec![
            json!({"name":"org.lwjgl:lwjgl-freetype:3.3.3","downloads":{"artifact":{
                "path":"org/lwjgl/lwjgl-freetype/3.3.3/lwjgl-freetype-3.3.3.jar",
                "url":"https://libraries.minecraft.net/org/lwjgl/lwjgl-freetype/3.3.3/lwjgl-freetype-3.3.3.jar",
                "sha1":"0123456789abcdef0123456789abcdef01234567"
            }}}),
        ];
        for suffix in ["natives-windows", "natives-windows-x86", "natives-windows-arm64"] {
            libraries.push(lwjgl_native(
                &format!("org.lwjgl:lwjgl-freetype:3.3.3:{suffix}"),
                suffix,
            ));
        }
        let version = json!({"id":"1.21.1","libraries":libraries});
        for (arch, classifier) in [
            ("x86_64","natives-windows"),
            ("x86","natives-windows-x86"),
            ("arm64","natives-windows-arm64"),
        ] {
            let mut host = env();
            host.arch = arch.into();
            let plan = plan_version(&version, Path::new("C:/Game"), &host).unwrap();
            let libs: Vec<_> = plan.iter().filter(|v| v.role == FileRole::Library).collect();
            let natives: Vec<_> = plan.iter().filter(|v| v.role == FileRole::NativeArchive).collect();
            assert_eq!(libs.len(), 1, "{arch} normal classpath library");
            assert_eq!(natives.len(), 1, "{arch} only matching native jar");
            assert!(natives[0].target.to_string_lossy().ends_with(
                &format!("lwjgl-freetype-3.3.3-{classifier}.jar")
            ));
        }
    }

    #[test]
    fn modern_native_must_obey_metadata_os_and_arch_rules() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let version = json!({"id":"1.21.1","libraries":[
            lwjgl_native("org.lwjgl:lwjgl-freetype:3.3.3:natives-windows", "natives-windows"),
            {"name":"org.lwjgl:lwjgl-glfw:3.3.3:natives-windows",
             "rules":[{"action":"allow","os":{"name":"windows","arch":"x86"}}],
             "downloads":{"artifact":{
                "sha1":sha,"url":"https://libraries.minecraft.net/other",
                "path":"org/lwjgl/lwjgl-glfw/3.3.3/lwjgl-glfw-3.3.3-natives-windows.jar"
             }}}
        ]});
        let plan = plan_version(&version,Path::new("C:/Game"),&env()).unwrap();
        assert_eq!(plan.len(),1);
        assert_eq!(plan[0].role,FileRole::NativeArchive);
    }

    #[test]
    fn legacy_classifier_remains_native_and_off_classpath() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let version = json!({"id":"1.12.2","libraries":[{
            "name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
            "natives":{"windows":"natives-windows-${arch}"},
            "downloads":{
                "artifact":{"sha1":sha,"url":"https://libraries.minecraft.net/main"},
                "classifiers":{"natives-windows-64":{
                    "path":"org/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows-64.jar",
                    "sha1":sha,"url":"https://libraries.minecraft.net/natives"
                }}
            }
        }]});
        let plan = plan_version(&version,Path::new("C:/Game"),&env()).unwrap();
        assert_eq!(plan.iter().filter(|p|p.role==FileRole::Library).count(),1);
        assert_eq!(plan.iter().filter(|p|p.role==FileRole::NativeArchive).count(),1);
    }

    #[test]
    fn classifier_path_traversal_is_rejected() {
        let version = json!({"id":"1.21.1","libraries":[{
            "name":"org.lwjgl:lwjgl:3.3.3:natives-windows",
            "downloads":{"artifact":{"path":"../escape.jar",
                "url":"https://libraries.minecraft.net/a",
                "sha1":"0123456789abcdef0123456789abcdef01234567"}}
        }]});
        assert!(plan_version(&version,Path::new("C:/Game"),&env()).is_err());
    }

    #[test]
    fn plan_client_assets_libs() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let json = json!({"id":"1.20.1","downloads":{"client":{"url":"https://piston-data.mojang.com/a","sha1":sha}},
          "assetIndex":{"id":"5","url":"https://piston-meta.mojang.com/assets","sha1":sha},
          "libraries":[{"name":"org.example:lib:1","downloads":{"artifact":{"url":"https://libraries.minecraft.net/a","sha1":sha,"path":"org/example/lib/1/lib-1.jar"}}}]});
        let p = plan_version(&json, Path::new("C:/Game"), &env()).unwrap();
        assert_eq!(p.len(), 3);
        assert!(p[2].target.to_string_lossy().contains("libraries"));
    }
    #[test]
    fn no_traversal() {
        assert!(safe_relative("../outside").is_err());
        assert!(safe_relative("a/../../b").is_err());
        assert!(maven_path("bad/thing:a:1").is_err());
    }
    #[test]
    fn asset_layout() {
        let id = "0123456789abcdef0123456789abcdef01234567";
        let p = plan_asset_objects(
            &json!({"objects":{"stone":{"hash":id,"size":9}}}),
            Path::new("/tmp/mc"),
        )
        .unwrap();
        assert_eq!(
            p[0].url,
            format!("https://resources.download.minecraft.net/01/{id}")
        );
    }
}
