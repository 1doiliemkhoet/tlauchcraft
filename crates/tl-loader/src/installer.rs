//! Safe inspection of official Forge / NeoForge installer profile JSON.
//! The caller MUST obtain and checksum-verify the official installer JAR first.
//! This module never executes an installer, arbitrary Java processors, or writes files.
use crate::{maven, LoaderError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessorStep {
    pub jar_coordinate: String,
    pub classpath_coordinates: Vec<String>,
    /// Unexpanded metadata tokens. This is NOT a shell command.
    pub opaque_arguments: Vec<String>,
    /// Installation side: client or server; absent means both.
    pub sides: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessorInspection {
    pub minecraft_version: String,
    pub processors: Vec<ProcessorStep>,
    /// The upstream profile is inspection-only until a separate trusted runner
    /// verifies each processor dependency, output and transaction boundary.
    pub ready_to_execute: bool,
}

#[derive(Deserialize)]
struct InstallProfile {
    minecraft: String,
    #[serde(default)]
    processors: Vec<RawProcessor>,
}
#[derive(Deserialize)]
struct RawProcessor {
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    sides: Vec<String>,
}

/// Modern NeoForge install_profile processor classpath supports explicit
/// Maven artifact type suffixes, e.g. group:name:version@jar or :all@jar.
/// Treat this as metadata, never interpolate it into a shell/path. Only the
/// Java JAR type is allowed until a separately reviewed executor exists.
fn validate_processor_coordinate(raw: &str) -> Result<(), LoaderError> {
    let coordinate = if let Some(stripped) = raw.strip_suffix("@jar") {
        if stripped.contains('@') {
            return Err(LoaderError::InvalidInput);
        }
        stripped
    } else {
        if raw.contains('@') {
            return Err(LoaderError::InvalidInput);
        }
        raw
    };
    maven::artifact_path(coordinate)?;
    Ok(())
}

/// Inspect an installation profile for the selected Minecraft version.
/// Unsupported legacy profiles are rejected rather than guessed.
pub fn inspect_install_profile(
    json: &str,
    expected_minecraft: &str,
) -> Result<ProcessorInspection, LoaderError> {
    crate::validate_component(expected_minecraft)?;
    let raw: InstallProfile =
        serde_json::from_str(json).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    if raw.minecraft != expected_minecraft {
        return Err(LoaderError::CompatibilityConflict);
    }
    if raw.processors.is_empty() {
        return Err(LoaderError::UnsupportedVersion(
            "installer profile has no processor list; legacy layouts require a dedicated adapter"
                .into(),
        ));
    }
    let mut processors = Vec::with_capacity(raw.processors.len());
    for p in raw.processors {
        validate_processor_coordinate(&p.jar)?;
        if p.classpath.len() > 256 || p.args.len() > 256 {
            return Err(LoaderError::InvalidMetadata(
                "excessive processor entries".into(),
            ));
        }
        for item in &p.classpath {
            validate_processor_coordinate(item)?;
        }
        for side in &p.sides {
            if side != "client" && side != "server" {
                return Err(LoaderError::InvalidMetadata(
                    "unknown processor side".into(),
                ));
            }
        }
        // Never interpret these as command lines. Reject control characters,
        // and enforce bounded strings before passing to a future trusted runner.
        for arg in &p.args {
            if arg.is_empty() || arg.len() > 8192 || arg.chars().any(|ch| ch.is_control()) {
                return Err(LoaderError::InvalidMetadata(
                    "unsafe processor argument".into(),
                ));
            }
        }
        processors.push(ProcessorStep {
            jar_coordinate: p.jar,
            classpath_coordinates: p.classpath,
            opaque_arguments: p.args,
            sides: p.sides,
        });
    }
    Ok(ProcessorInspection {
        minecraft_version: raw.minecraft,
        processors,
        ready_to_execute: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspects_valid_processor_without_execution() {
        let profile = r#"{"minecraft":"1.20.1","processors":[{"jar":"net.minecraftforge:installertools:1.4.1","classpath":["net.minecraftforge:installertools:1.4.1"],"args":["--task","EXTRACT_FILES"],"sides":["client"]}]}"#;
        let p = inspect_install_profile(profile, "1.20.1").unwrap();
        assert_eq!(p.processors.len(), 1);
        assert!(!p.ready_to_execute);
        assert_eq!(
            p.processors[0].opaque_arguments,
            vec!["--task", "EXTRACT_FILES"]
        );
    }
    #[test]
    fn accepts_only_explicit_jar_maven_type_in_neoforge_classpath() {
        let profile = r#"{"minecraft":"1.21.1","processors":[{"jar":"net.neoforged.installertools:installertools:2.1.2","classpath":["net.neoforged.installertools:installertools:2.1.2@jar","net.neoforged:AutoRenamingTool:2.0.3:all@jar"],"sides":["client"],"args":["--task","EXTRACT_FILES"]}]}"#;
        let parsed = inspect_install_profile(profile, "1.21.1").unwrap();
        assert_eq!(parsed.processors.len(), 1);
        assert!(!parsed.ready_to_execute);
        for bad in [
            "x:y:1@zip",
            "x:y:1@jar@jar",
            "../escape:x:1@jar",
            "x:y:../../escape@jar",
        ] {
            assert!(validate_processor_coordinate(bad).is_err());
        }
    }
    #[test]
    fn rejects_minecraft_mismatch() {
        let p = r#"{"minecraft":"1.20.1","processors":[{"jar":"x:y:1"}]}"#;
        assert_eq!(
            inspect_install_profile(p, "1.21.1"),
            Err(LoaderError::CompatibilityConflict)
        );
    }
    #[test]
    fn rejects_bad_coordinates_and_untrusted_side() {
        let bad = r#"{"minecraft":"1.20.1","processors":[{"jar":"x:y:../../x"}]}"#;
        assert!(inspect_install_profile(bad, "1.20.1").is_err());
        let bad_side =
            r#"{"minecraft":"1.20.1","processors":[{"jar":"x:y:1","sides":["internet"]}]}"#;
        assert!(inspect_install_profile(bad_side, "1.20.1").is_err());
    }
    #[test]
    fn rejects_legacy_unknown_processor_layout() {
        assert!(matches!(
            inspect_install_profile(r#"{"minecraft":"1.12.2"}"#, "1.12.2"),
            Err(LoaderError::UnsupportedVersion(_))
        ));
    }
}
