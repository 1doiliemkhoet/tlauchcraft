//! Standalone game core. Chat 1 owns workspace wiring and shared public types.
//! No authentication or proprietary game files are provided by this crate.
pub mod foundation;
pub mod metadata;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported version: {0}")]
    UnsupportedVersion(String),
    #[error("integrity mismatch: {0}")]
    IntegrityMismatch(String),
    #[error("network failure: {0}")]
    NetworkFailure(String),
    #[error("cancelled")]
    Cancelled,
    #[error("java compatibility conflict: expected {expected}, found {actual}")]
    CompatibilityConflict { expected: u32, actual: u32 },
    #[error("launch failed: {0}")]
    LaunchFailed(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub os: String,
    pub arch: String,
    pub os_version: String,
    pub features: HashMap<String, bool>,
}
impl Environment {
    pub fn current() -> Self {
        Self {
            os: match std::env::consts::OS {
                "windows" => "windows",
                "macos" => "osx",
                _ => "linux",
            }
            .into(),
            arch: match std::env::consts::ARCH {
                "x86_64" => "x86_64",
                "x86" => "x86",
                "aarch64" => "arm64",
                v => v,
            }
            .into(),
            os_version: String::new(),
            features: HashMap::new(),
        }
    }
}

/// Mojang rules are ordered: last matching action wins; a nonmatching rule does not reset it.
pub fn permitted(rules: Option<&Value>, env: &Environment) -> bool {
    let Some(items) = rules.and_then(Value::as_array) else {
        return true;
    };
    let mut result = false;
    for rule in items {
        let os_matches = rule.get("os").is_none_or(|os| {
            os.get("name")
                .and_then(Value::as_str)
                .is_none_or(|v| v == env.os)
                && os
                    .get("arch")
                    .and_then(Value::as_str)
                    .is_none_or(|v| v == env.arch || (v == "x86" && env.arch == "x86"))
                && os.get("version").and_then(Value::as_str).is_none_or(|v| {
                    // Fail closed on regex constraints until a tested regex engine is integrated.
                    !v.is_empty() && v == env.os_version
                })
        });
        let features_match = rule
            .get("features")
            .and_then(Value::as_object)
            .is_none_or(|f| {
                f.iter().all(|(k, v)| {
                    env.features.get(k).copied().unwrap_or(false) == v.as_bool().unwrap_or(false)
                })
            });
        if os_matches && features_match {
            result = rule.get("action").and_then(Value::as_str) == Some("allow");
        }
    }
    result
}

/// Parent and child metadata merge: parent libraries first, child overrides matching 'name';
/// argument arrays append, while scalar/object settings prefer the child.
pub fn inherit(parent: &Value, child: &Value) -> Result<Value, GameError> {
    let mut merged = parent.clone();
    let base = merged
        .as_object_mut()
        .ok_or_else(|| GameError::InvalidInput("parent must be JSON object".into()))?;
    let source = child
        .as_object()
        .ok_or_else(|| GameError::InvalidInput("child must be JSON object".into()))?;
    for (key, value) in source {
        if key == "libraries" {
            let mut libs = base
                .get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for lib in value
                .as_array()
                .ok_or_else(|| GameError::InvalidInput("libraries must be array".into()))?
            {
                if let Some(name) = lib.get("name") {
                    libs.retain(|old| old.get("name") != Some(name));
                }
                libs.push(lib.clone());
            }
            base.insert(key.clone(), Value::Array(libs));
        } else if key == "arguments" {
            let mut args = base
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (kind, entries) in value
                .as_object()
                .ok_or_else(|| GameError::InvalidInput("arguments must be object".into()))?
            {
                let mut old = args
                    .get(kind)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                old.extend(
                    entries
                        .as_array()
                        .ok_or_else(|| {
                            GameError::InvalidInput("argument list must be array".into())
                        })?
                        .iter()
                        .cloned(),
                );
                args.insert(kind.clone(), Value::Array(old));
            }
            base.insert(key.clone(), Value::Object(args));
        } else if value.is_object() && base.get(key).is_some_and(Value::is_object) {
            let mut old = base
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (k, v) in value.as_object().unwrap() {
                old.insert(k.clone(), v.clone());
            }
            base.insert(key.clone(), Value::Object(old));
        } else {
            base.insert(key.clone(), value.clone());
        }
    }
    Ok(merged)
}

/// Resolve inherited version JSON using already verified local metadata. Cycles and missing parents are errors.
pub fn resolve_version(id: &str, versions: &HashMap<String, Value>) -> Result<Value, GameError> {
    fn walk(
        id: &str,
        versions: &HashMap<String, Value>,
        stack: &mut Vec<String>,
    ) -> Result<Value, GameError> {
        if stack.iter().any(|v| v == id) {
            return Err(GameError::InvalidInput(format!("inheritance cycle: {id}")));
        }
        let version = versions
            .get(id)
            .ok_or_else(|| GameError::UnsupportedVersion(id.into()))?;
        stack.push(id.into());
        let result = if let Some(parent) = version.get("inheritsFrom").and_then(Value::as_str) {
            inherit(&walk(parent, versions, stack)?, version)?
        } else {
            version.clone()
        };
        stack.pop();
        Ok(result)
    }
    walk(id, versions, &mut Vec::new())
}

pub fn argument_list(value: &Value, env: &Environment) -> Result<Vec<String>, GameError> {
    let mut out = Vec::new();
    for arg in value
        .as_array()
        .ok_or_else(|| GameError::InvalidInput("argument list required".into()))?
    {
        if let Some(s) = arg.as_str() {
            out.push(s.into());
        } else if permitted(arg.get("rules"), env) {
            let v = arg
                .get("value")
                .ok_or_else(|| GameError::InvalidInput("missing argument value".into()))?;
            if let Some(s) = v.as_str() {
                out.push(s.into());
            } else if let Some(values) = v.as_array() {
                for s in values {
                    out.push(
                        s.as_str()
                            .ok_or_else(|| GameError::InvalidInput("argument must be text".into()))?
                            .into(),
                    );
                }
            } else {
                return Err(GameError::InvalidInput("unsupported argument value".into()));
            }
        }
    }
    Ok(out)
}

pub fn asset_object_path(root: &Path, sha1: &str) -> Result<PathBuf, GameError> {
    if sha1.len() != 40 || !sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(GameError::InvalidInput("invalid asset sha1".into()));
    }
    Ok(root
        .join("assets")
        .join("objects")
        .join(&sha1[..2])
        .join(sha1))
}

#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: String,
    pub expected_sha1: String,
    pub expected_size: Option<u64>,
    pub target: PathBuf,
}
fn check_sha1(path: &Path, expected: &str, size: Option<u64>) -> Result<(), GameError> {
    if expected.len() != 40 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(GameError::InvalidInput(
            "sha1 must be 40 hex characters".into(),
        ));
    }
    let file = fs::File::open(path)?;
    if let Some(n) = size {
        if file.metadata()?.len() != n {
            return Err(GameError::IntegrityMismatch(format!(
                "size: {}",
                path.display()
            )));
        }
    }
    let mut reader = std::io::BufReader::new(file);
    let mut hash = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != expected.to_ascii_lowercase() {
        return Err(GameError::IntegrityMismatch(format!(
            "sha1: {}",
            path.display()
        )));
    }
    Ok(())
}
/// Reject symlink components before writing to a launcher-managed destination.
fn ensure_no_symlink_ancestors(path: &Path) -> Result<(), GameError> {
    for component in path.ancestors() {
        if component.is_symlink() {
            return Err(GameError::InvalidInput(format!(
                "symlink path rejected: {}",
                component.display()
            )));
        }
    }
    Ok(())
}
pub fn install_file(spec: &DownloadSpec, cancelled: &AtomicBool) -> Result<(), GameError> {
    if !spec.url.starts_with("https://") || spec.url.contains('@') || spec.url.contains('#') {
        return Err(GameError::InvalidInput(
            "only HTTPS URLs without credentials/fragments are allowed".into(),
        ));
    }
    let parsed =
        reqwest::Url::parse(&spec.url).map_err(|e| GameError::InvalidInput(e.to_string()))?;
    let trusted = matches!(
        parsed.host_str(),
        Some(
            "piston-meta.mojang.com"
                | "piston-data.mojang.com"
                | "resources.download.minecraft.net"
                | "libraries.minecraft.net"
                | "launchermeta.mojang.com"
                | "launcher.mojang.com"
        )
    );
    if !trusted || parsed.port().is_some() {
        return Err(GameError::InvalidInput("unapproved download origin".into()));
    }
    ensure_no_symlink_ancestors(&spec.target)?;
    if spec.target.is_file()
        && check_sha1(&spec.target, &spec.expected_sha1, spec.expected_size).is_ok()
    {
        return Ok(());
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err(GameError::Cancelled);
    }
    let parent = spec
        .target
        .parent()
        .ok_or_else(|| GameError::InvalidInput("target has no parent".into()))?;
    fs::create_dir_all(parent)?;
    // A unique temporary name avoids clobbering another download in the same directory.
    let stem = spec
        .target
        .file_name()
        .ok_or_else(|| GameError::InvalidInput("target has no filename".into()))?
        .to_string_lossy();
    let part = parent.join(format!(".{stem}.{}.part", std::process::id()));
    if part.exists() {
        return Err(GameError::InvalidInput(
            "a download staging file already exists; refusing to overwrite".into(),
        ));
    }
    let agent = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| GameError::NetworkFailure(e.to_string()))?;
    let operation = (|| -> Result<(), GameError> {
        for attempt in 0..3 {
            if cancelled.load(Ordering::Relaxed) {
                return Err(GameError::Cancelled);
            }
            let response = agent.get(&spec.url).send();
            match response {
                Ok(mut response) if response.status() == reqwest::StatusCode::OK => {
                    let mut output = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&part)?;
                    let mut buffer = [0u8; 64 * 1024];
                    loop {
                        if cancelled.load(Ordering::Relaxed) {
                            return Err(GameError::Cancelled);
                        }
                        let n = response
                            .read(&mut buffer)
                            .map_err(|e| GameError::NetworkFailure(e.to_string()))?;
                        if n == 0 {
                            break;
                        }
                        output.write_all(&buffer[..n])?;
                    }
                    output.sync_all()?;
                    check_sha1(&part, &spec.expected_sha1, spec.expected_size)?;
                    if spec.target.exists() {
                        // Preserve preexisting (possibly user-owned) files; repair must be explicitly approved.
                        return Err(GameError::IntegrityMismatch(format!(
                            "destination occupied: {}",
                            spec.target.display()
                        )));
                    }
                    fs::rename(&part, &spec.target)?;
                    return Ok(());
                }
                Ok(response)
                    if !response.status().is_server_error()
                        && response.status().as_u16() != 429 =>
                {
                    return Err(GameError::NetworkFailure(format!(
                        "HTTP {}",
                        response.status()
                    )))
                }
                Ok(_) | Err(_) => {
                    if attempt < 2 {
                        std::thread::sleep(Duration::from_millis(250 * (1 << attempt)));
                    }
                }
            }
        }
        Err(GameError::NetworkFailure("retries exhausted".into()))
    })();
    if part.exists() {
        let _ = fs::remove_file(&part);
    }
    operation
}

/// Extract Java major from java -version output (8: 1.8, modern: 17, 21, 25).
pub fn parse_java_major(output: &str) -> Result<u32, GameError> {
    let version = output
        .split('"')
        .nth(1)
        .ok_or_else(|| GameError::UnsupportedVersion("unrecognized Java version".into()))?;
    let first = version.split('.').next().unwrap_or("");
    let major = if first == "1" {
        version.split('.').nth(1).unwrap_or("")
    } else {
        first
    };
    major
        .parse()
        .map_err(|_| GameError::UnsupportedVersion(version.into()))
}
pub fn probe_java(java: &Path, required: u32) -> Result<u32, GameError> {
    let result = Command::new(java)
        .arg("-version")
        .output()
        .map_err(|e| GameError::LaunchFailed(e.to_string()))?;
    if !result.status.success() {
        return Err(GameError::LaunchFailed("java -version failed".into()));
    }
    let info = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stderr),
        String::from_utf8_lossy(&result.stdout)
    );
    let major = parse_java_major(&info)?;
    if major != required {
        return Err(GameError::CompatibilityConflict {
            expected: required,
            actual: major,
        });
    }
    Ok(major)
}

#[derive(Debug, Clone)]
pub struct LaunchConfig {
    pub java: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub game_dir: PathBuf,
    pub main_class: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub min_ram_mb: u32,
    pub max_ram_mb: u32,
}
pub fn build_command(config: &LaunchConfig) -> Result<Command, GameError> {
    if config.min_ram_mb < 256
        || config.max_ram_mb < config.min_ram_mb
        || config.max_ram_mb > 65_536
    {
        return Err(GameError::InvalidInput("unsafe RAM configuration".into()));
    }
    if config.main_class.is_empty() || config.classpath.is_empty() {
        return Err(GameError::InvalidInput(
            "missing mainClass or classpath".into(),
        ));
    }

    let cp = std::env::join_paths(&config.classpath)
        .map_err(|e| GameError::InvalidInput(e.to_string()))?;
    let mut cmd = Command::new(&config.java);
    cmd.current_dir(&config.game_dir)
        .arg(format!("-Xms{}M", config.min_ram_mb))
        .arg(format!("-Xmx{}M", config.max_ram_mb))
        .arg(format!(
            "-Djava.library.path={}",
            config.natives_dir.display()
        ))
        .args(&config.jvm_args)
        .arg("-cp")
        .arg(cp)
        .arg(&config.main_class)
        .args(&config.game_args);

    Ok(cmd)
}
pub fn launch(config: &LaunchConfig, stdout: &Path, stderr: &Path) -> Result<Child, GameError> {
    fs::create_dir_all(&config.game_dir)?;
    let mut command = build_command(config)?;
    command
        .stdout(Stdio::from(fs::File::create(stdout)?))
        .stderr(Stdio::from(fs::File::create(stderr)?));
    command
        .spawn()
        .map_err(|e| GameError::LaunchFailed(e.to_string()))
}

pub fn expand_arguments(
    args: &[String],
    values: &HashMap<String, String>,
) -> Result<Vec<String>, GameError> {
    args.iter()
        .map(|s| {
            let mut expanded = s.clone();
            for (name, value) in values {
                expanded = expanded.replace(&format!("${{{name}}}"), value);
            }
            if expanded.contains("${") {
                return Err(GameError::InvalidInput(format!(
                    "unresolved placeholder in {s}"
                )));
            }
            Ok(expanded)
        })
        .collect()
}

pub fn required_java(version: &Value) -> Result<u32, GameError> {
    version.get("javaVersion").and_then(|v| v.get("majorVersion")).and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| GameError::UnsupportedVersion("javaVersion.majorVersion not present; require explicit approved compatibility mapping".into()))
}

pub fn main_class(version: &Value) -> Result<&str, GameError> {
    version
        .get("mainClass")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| GameError::UnsupportedVersion("missing mainClass".into()))
}

pub fn libraries(version: &Value, env: &Environment) -> Vec<Value> {
    version
        .get("libraries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|v| permitted(v.get("rules"), env))
        .cloned()
        .collect()
}

pub fn game_arguments(version: &Value, env: &Environment) -> Result<Vec<String>, GameError> {
    if let Some(args) = version.get("arguments").and_then(|v| v.get("game")) {
        argument_list(args, env)
    } else if let Some(legacy) = version.get("minecraftArguments").and_then(Value::as_str) {
        // Legacy Mojang arguments use whitespace tokens. Quoted legacy fields require a fuller lexer.
        if legacy.contains('"') || legacy.contains('\'') {
            return Err(GameError::UnsupportedVersion(
                "quoted legacy arguments require dedicated parser".into(),
            ));
        }
        Ok(legacy.split_whitespace().map(str::to_owned).collect())
    } else {
        Err(GameError::UnsupportedVersion("no game arguments".into()))
    }
}
pub fn jvm_arguments(version: &Value, env: &Environment) -> Result<Vec<String>, GameError> {
    version
        .get("arguments")
        .and_then(|v| v.get("jvm"))
        .map(|v| argument_list(v, env))
        .transpose()
        .map(|v| v.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn env() -> Environment {
        Environment {
            os: "windows".into(),
            arch: "x86_64".into(),
            os_version: "".into(),
            features: HashMap::new(),
        }
    }
    #[test]
    fn ordered_rules() {
        assert!(permitted(
            Some(
                &json!([{"action":"allow","os":{"name":"windows"}},{"action":"disallow","os":{"name":"linux"}}])
            ),
            &env()
        ));
        assert!(!permitted(
            Some(&json!([{"action":"allow","os":{"name":"linux"}}])),
            &env()
        ));
    }
    #[test]
    fn inheritance_and_cycle() {
        let mut v = HashMap::new();
        v.insert("p".into(),json!({"id":"p","mainClass":"net.minecraft.Main","libraries":[{"name":"a","x":1}],"arguments":{"game":["--parent"]}}));
        v.insert("c".into(),json!({"id":"c","inheritsFrom":"p","libraries":[{"name":"a","x":2}],"arguments":{"game":["--child"]}}));
        let resolved = resolve_version("c", &v).unwrap();
        assert_eq!(resolved["libraries"][0]["x"], 2);
        assert_eq!(
            resolved["arguments"]["game"],
            json!(["--parent", "--child"])
        );
        v.insert("p".into(), json!({"inheritsFrom":"c"}));
        assert!(resolve_version("c", &v).is_err());
    }
    #[test]
    fn java_versions() {
        assert_eq!(parse_java_major("java version \"1.8.0_321\"").unwrap(), 8);
        assert_eq!(parse_java_major("openjdk version \"21.0.2\"").unwrap(), 21);
    }
    #[test]
    fn argument_rules() {
        let a = json!(["--basic",{"rules":[{"action":"allow","os":{"name":"windows"}}],"value":["--win","yes"]},{"rules":[{"action":"allow","os":{"name":"osx"}}],"value":"--mac"}]);
        assert_eq!(
            argument_list(&a, &env()).unwrap(),
            vec!["--basic", "--win", "yes"]
        );
    }
    #[test]
    fn sha_mismatch() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sample");
        fs::write(&p, b"incorrect").unwrap();
        assert!(matches!(
            check_sha1(&p, "0000000000000000000000000000000000000000", None),
            Err(GameError::IntegrityMismatch(_))
        ));
    }
    #[test]
    fn windows_spaces_preserved_as_separate_args() {
        let c = LaunchConfig {
            java: "C:/Program Files/Java/bin/java.exe".into(),
            classpath: vec!["C:/Minecraft Instance/lib a.jar".into()],
            natives_dir: "C:/Minecraft Instance/natives".into(),
            game_dir: "C:/Minecraft Instance".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            jvm_args: vec!["-Dcustom=hello world".into()],
            game_args: vec!["--username".into(), "Example".into()],
            min_ram_mb: 512,
            max_ram_mb: 2048,
        };
        let cmd = build_command(&c).unwrap();
        assert_eq!(cmd.get_program(), "C:/Program Files/Java/bin/java.exe");
        assert!(cmd.get_args().any(|v| v == "-Dcustom=hello world"));
    }
    #[test]
    fn sha_matching_file_cached() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("cached.jar");
        fs::write(&target, b"hello").unwrap();
        let expected = format!("{:x}", Sha1::digest(b"hello"));
        assert!(check_sha1(&target, &expected, Some(5)).is_ok());
        assert!(check_sha1(&target, &expected, Some(4)).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_ancestor_rejected() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let safe = d.path().join("safe");
        fs::create_dir(&safe).unwrap();
        let alias = d.path().join("alias");
        symlink(&safe, &alias).unwrap();
        assert!(ensure_no_symlink_ancestors(&alias.join("child").join("file")).is_err());
    }
    #[test]
    fn rejects_unresolved_placeholders() {
        assert!(expand_arguments(
            &["--user".into(), "${auth_player_name}".into()],
            &HashMap::new()
        )
        .is_err());
    }
}
