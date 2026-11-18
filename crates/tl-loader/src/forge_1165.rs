//! Forge 1.16.5 client metadata contract; only verified installer archive contents may be parsed.
use crate::LoaderError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeClientStep {
    pub jar: String,
    pub classpath: Vec<String>,
    pub args: Vec<String>,
    /// Raw output reference → official expected SHA-1 token.
    /// Chat 4 resolves these via the verified install_profile.json data section.
    pub outputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeClientBlueprint {
    pub minecraft: String,
    pub forge_version: String,
    pub version_id: String,
    pub steps: Vec<ForgeClientStep>,
    /// Describes loader-specific execution, NOT installation success.
    pub verified_installation: bool,
}

#[derive(Deserialize)]
struct Profile {
    minecraft: String,
    version: String,
    processors: Vec<RawStep>,
}
#[derive(Deserialize)]
struct RawStep {
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    sides: Vec<String>,
    #[serde(default)]
    outputs: BTreeMap<String, String>,
}

pub fn forge_1165_client_blueprint(
    install_profile: &str,
    derived_version: &Value,
    requested_forge: &str,
) -> Result<ForgeClientBlueprint, LoaderError> {
    crate::validate_component(requested_forge)?;
    let p: Profile = serde_json::from_str(install_profile)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    if p.minecraft != "1.16.5" || p.processors.is_empty() || p.processors.len() > 64 {
        return Err(LoaderError::CompatibilityConflict);
    }
    let expected = format!("1.16.5-forge-{requested_forge}");
    if p.version != expected
        || derived_version.get("id").and_then(Value::as_str) != Some(expected.as_str())
        || derived_version.get("inheritsFrom").and_then(Value::as_str) != Some("1.16.5")
        || derived_version.get("mainClass").and_then(Value::as_str)
            != Some("cpw.mods.modlauncher.Launcher")
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    let mut steps = Vec::new();
    for raw in p.processors {
        if raw.sides.iter().any(|x| x != "client" && x != "server") {
            return Err(LoaderError::InvalidInput);
        }
        if !raw.sides.is_empty() && !raw.sides.iter().any(|x| x == "client") {
            continue;
        }
        crate::maven::artifact_path(&raw.jar)?;
        for dep in &raw.classpath {
            crate::maven::artifact_path(dep)?;
        }
        if raw.args.len() > 256
            || raw.outputs.len() > 64
            || raw
                .args
                .iter()
                .any(|x| x.is_empty() || x.len() > 8192 || x.chars().any(char::is_control))
        {
            return Err(LoaderError::InvalidInput);
        }
        for (output, hash_ref) in &raw.outputs {
            if output.is_empty()
                || hash_ref.is_empty()
                || output.len() > 8192
                || hash_ref.len() > 8192
                || output.chars().any(char::is_control)
                || hash_ref.chars().any(char::is_control)
            {
                return Err(LoaderError::InvalidInput);
            }
        }
        steps.push(ForgeClientStep {
            jar: raw.jar,
            classpath: raw.classpath,
            args: raw.args,
            outputs: raw.outputs,
        });
    }
    if steps.is_empty() {
        return Err(LoaderError::InvalidMetadata("no client processors".into()));
    }
    Ok(ForgeClientBlueprint {
        minecraft: "1.16.5".into(),
        forge_version: requested_forge.into(),
        version_id: expected,
        steps,
        verified_installation: false,
    })
}

/// Inherit the verified Mojang parent using Chat 2's only version merge path.
pub fn merge_forge_1165_version(
    parent: &Value,
    derived: &Value,
    requested_forge: &str,
) -> Result<Value, LoaderError> {
    let expected = format!("1.16.5-forge-{requested_forge}");
    if parent.get("id").and_then(Value::as_str) != Some("1.16.5")
        || derived.get("id").and_then(Value::as_str) != Some(expected.as_str())
        || derived.get("inheritsFrom").and_then(Value::as_str) != Some("1.16.5")
        || derived.get("mainClass").and_then(Value::as_str) != Some("cpw.mods.modlauncher.Launcher")
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    tl_game::inherit(parent, derived).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const FIXTURE: &str = include_str!("../tests/fixtures/forge_1165_profile.json");
    fn version() -> Value {
        json!({"id":"1.16.5-forge-36.2.34","inheritsFrom":"1.16.5","mainClass":"cpw.mods.modlauncher.Launcher","libraries":[{"name":"net.minecraftforge:forge:1.16.5-36.2.34"}]})
    }
    #[test]
    fn client_side_steps_and_outputs_from_fixture() {
        let p = forge_1165_client_blueprint(FIXTURE, &version(), "36.2.34").unwrap();
        assert_eq!(p.minecraft, "1.16.5");
        assert_eq!(p.steps.len(), 2);
        assert_eq!(p.steps[1].outputs.len(), 1);
        assert!(!p.verified_installation);
    }
    #[test]
    fn mismatched_forge_refused() {
        assert_eq!(
            forge_1165_client_blueprint(FIXTURE, &version(), "36.2.42"),
            Err(LoaderError::CompatibilityConflict)
        );
    }
    #[test]
    fn rejects_unknown_processor_coordinate() {
        let bad = FIXTURE.replace("net.minecraftforge:installertools:1.2.0", "../evil");
        assert!(forge_1165_client_blueprint(&bad, &version(), "36.2.34").is_err());
    }
    #[test]
    fn forge_36229_overrides_legacy_parent_log4j_without_classpath_duplicates() {
        let derived: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/forge_36229_official_version.json"
        ))
        .unwrap();
        let parent = json!({
            "id":"1.16.5",
            "libraries":[
                {"name":"org.apache.logging.log4j:log4j-api:2.8.1"},
                {"name":"org.apache.logging.log4j:log4j-core:2.8.1"},
                {"name":"com.example:preserved:1.0"}
            ]
        });
        let merged = merge_forge_1165_version(&parent, &derived, "36.2.29").unwrap();
        let libs = merged["libraries"].as_array().unwrap();
        for artifact in ["log4j-api", "log4j-core"] {
            let names = libs
                .iter()
                .filter_map(|item| item["name"].as_str())
                .filter(|name| name.starts_with(&format!("org.apache.logging.log4j:{artifact}:")))
                .collect::<Vec<_>>();
            assert_eq!(names, vec![format!("org.apache.logging.log4j:{artifact}:2.15.0")]);
        }
        assert!(libs.iter().any(|item| item["name"] == "com.example:preserved:1.0"));
        assert_eq!(merged["mainClass"], "cpw.mods.modlauncher.Launcher");
    }

    #[test]
    fn merges_only_exact_minecraft_inheritance() {
        let parent = json!({"id":"1.16.5","libraries":[{"name":"base"}]});
        let merged = merge_forge_1165_version(&parent, &version(), "36.2.34").unwrap();
        assert_eq!(merged["mainClass"], "cpw.mods.modlauncher.Launcher");
        assert_eq!(merged["libraries"].as_array().unwrap().len(), 2);
        assert!(merge_forge_1165_version(&json!({"id":"1.20.1"}), &version(), "36.2.34").is_err());
    }
}
