//! Independently reviewed cross-generation Forge installers. This is a
//! compatibility/pinning boundary, NOT a grant to execute an installer.
//! The approved 1.16.5 runtime is kept in forge_family.rs and remains unchanged.
use crate::{installer, LoaderError};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeInstallerSchema {
    /// Forge 1.12.2 distributes a legacy client profile/universal JAR layout.
    /// Never route this through modern Forge Java processors.
    Legacy1122,
    /// Modern installer records named processors whose binary identity and
    /// output hashes must be approved separately by a generation-specific runner.
    ModernProcessor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewedForgeRelease {
    pub minecraft: &'static str,
    pub forge: &'static str,
    pub installer_sha1: &'static str,
    pub java_major: u32,
    pub schema: ForgeInstallerSchema,
}

// Official SHA-1 values come from the corresponding downloads at
// files.minecraftforge.net/net/minecraftforge/forge/index_<minecraft>.html.
// Pinning an installer is necessary, but does not prove installed runtime.
pub const CROSS_GENERATION_RELEASES: &[ReviewedForgeRelease] = &[
    ReviewedForgeRelease {
        minecraft: "1.12.2",
        forge: "14.23.5.2860",
        installer_sha1: "5e8a91f71ef3d1f77de3f8d3261aedcc2f551c9d",
        java_major: 8,
        schema: ForgeInstallerSchema::Legacy1122,
    },
    ReviewedForgeRelease {
        minecraft: "1.18.2",
        forge: "40.2.21",
        installer_sha1: "5507eebfe3ca62545419e7aa8ee7b5dc559f0e26",
        java_major: 17,
        schema: ForgeInstallerSchema::ModernProcessor,
    },
    ReviewedForgeRelease {
        minecraft: "1.20.1",
        forge: "47.3.0",
        installer_sha1: "0deb8e547a9e3f098a4c9f082903499703c52f5d",
        java_major: 17,
        schema: ForgeInstallerSchema::ModernProcessor,
    },
];

pub fn reviewed_cross_generation(
    minecraft: &str,
    forge: &str,
) -> Result<&'static ReviewedForgeRelease, LoaderError> {
    CROSS_GENERATION_RELEASES
        .iter()
        .find(|r| r.minecraft == minecraft && r.forge == forge)
        .ok_or_else(|| LoaderError::UnsupportedVersion(format!(
            "Forge {minecraft}-{forge} is not in the independently reviewed cross-generation registry"
        )))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeInstallerInspection {
    pub minecraft_version: String,
    pub forge_version: String,
    pub java_major: u32,
    pub schema: ForgeInstallerSchema,
    pub selected_processor_count: usize,
    /// Pins + metadata binding are inspection evidence only.
    pub ready_to_execute: bool,
    pub instance_ready: bool,
}

/// Must receive the metadata extracted as DATA from an independently
/// SHA-1 verified official installer archive. This function does no I/O,
/// does not execute Java, and cannot mint a READY or publish receipt.
pub fn inspect_reviewed_installer(
    minecraft: &str,
    forge: &str,
    observed_installer_sha1: &str,
    profile_json: &str,
    version_json: &str,
) -> Result<ForgeInstallerInspection, LoaderError> {
    let release = reviewed_cross_generation(minecraft, forge)?;
    if observed_installer_sha1.len() != 40
        || !observed_installer_sha1.eq_ignore_ascii_case(release.installer_sha1)
    {
        return Err(LoaderError::InvalidMetadata(
            "official Forge installer SHA-1 differs from reviewed release".into(),
        ));
    }
    let profile: Value = serde_json::from_str(profile_json)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let version: Value = serde_json::from_str(version_json)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let expected_id = format!("{minecraft}-forge-{forge}");
    let actual_version = version["id"].as_str().ok_or_else(|| {
        LoaderError::InvalidMetadata("Forge client version JSON lacks id".into())
    })?;
    if !actual_version.ends_with(forge)
        || !actual_version.contains(minecraft)
        || version["inheritsFrom"] != minecraft
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    let count = match release.schema {
        ForgeInstallerSchema::Legacy1122 => {
            // Legacy 1.12.2 versions can use historical naming, but must
            // derive from exactly the requested Mojang version. The legacy
            // JAR is not equivalent to modern binarypatcher outputs.
            let legacy_mc = profile["minecraft"]
                .as_str()
                .or_else(|| profile["install"]["minecraft"].as_str());
            if legacy_mc != Some(minecraft) && !profile["install"].is_object() {
                return Err(LoaderError::CompatibilityConflict);
            }
            if profile["processors"].as_array().is_some_and(|p| !p.is_empty()) {
                return Err(LoaderError::InvalidMetadata(
                    "legacy Forge profile unexpectedly declares modern processors".into(),
                ));
            }
            0
        }
        ForgeInstallerSchema::ModernProcessor => {
            if profile["version"] != expected_id || version["id"] != expected_id {
                return Err(LoaderError::CompatibilityConflict);
            }
            let inspection = installer::inspect_install_profile(profile_json, minecraft)?;
            // A modern release's processors are metadata-controlled Java code.
            // They MUST be audited by a generation-specific execution adapter.
            if inspection.processors.is_empty() {
                return Err(LoaderError::InvalidMetadata(
                    "modern Forge release has no client processor inventory".into(),
                ));
            }
            inspection.processors.len()
        }
    };
    Ok(ForgeInstallerInspection {
        minecraft_version: minecraft.to_owned(),
        forge_version: forge.to_owned(),
        java_major: release.java_major,
        schema: release.schema,
        selected_processor_count: count,
        ready_to_execute: false,
        instance_ready: false,
    })
}

/// NeoForge versions do not determine Minecraft compatibility solely by their
/// Maven version. These are explicitly reviewed *candidate* associations, not
/// trusted installer hashes and never authorization for execution/publication.
#[derive(Debug, Clone, Copy)]
pub struct NeoForgeCandidate {
    pub minecraft: &'static str,
    pub version: &'static str,
    pub java_major: u32,
}
pub const NEOFORGE_CANDIDATES: &[NeoForgeCandidate] = &[
    NeoForgeCandidate {
        minecraft: "1.21.1",
        version: "21.1.80",
        java_major: 21,
    },
];
pub fn neoforge_candidate(
    minecraft: &str,
    version: &str,
) -> Result<&'static NeoForgeCandidate, LoaderError> {
    NEOFORGE_CANDIDATES
        .iter()
        .find(|r| r.minecraft == minecraft && r.version == version)
        .ok_or_else(|| LoaderError::UnsupportedVersion(format!(
            "NeoForge {version} / Minecraft {minecraft}: no reviewed candidate"
        )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_forge_generations_have_independent_java_and_schemas() {
        for (minecraft, forge, java, legacy) in [
            ("1.12.2", "14.23.5.2860", 8, true),
            ("1.18.2", "40.2.21", 17, false),
            ("1.20.1", "47.3.0", 17, false),
        ] {
            let r = reviewed_cross_generation(minecraft, forge).unwrap();
            assert_eq!(r.java_major, java);
            assert_eq!(r.schema == ForgeInstallerSchema::Legacy1122, legacy);
            assert_eq!(r.installer_sha1.len(), 40);
        }
        assert!(reviewed_cross_generation("1.20.1", "14.23.5.2860").is_err());
        assert!(reviewed_cross_generation("1.18.2", "47.3.0").is_err());
        assert!(reviewed_cross_generation("1.12.2", "14.23.5.9999").is_err());
    }
    #[test]
    fn modern_metadata_never_issues_execution_authority() {
        let profile = r#"{"minecraft":"1.20.1","version":"1.20.1-forge-47.3.0","processors":[{"jar":"net.minecraftforge:installertools:1.4.1","args":["--task","EXTRACT_FILES"],"sides":["client"]}]}"#;
        let version = r#"{"id":"1.20.1-forge-47.3.0","inheritsFrom":"1.20.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher"}"#;
        let r = reviewed_cross_generation("1.20.1", "47.3.0").unwrap();
        let inspected = inspect_reviewed_installer("1.20.1", "47.3.0", r.installer_sha1, profile, version).unwrap();
        assert_eq!(inspected.selected_processor_count, 1);
        assert_eq!(inspected.java_major, 17);
        assert!(!inspected.ready_to_execute);
        assert!(!inspected.instance_ready);
        assert!(inspect_reviewed_installer("1.20.1", "47.3.0", &"0".repeat(40), profile, version).is_err());
        assert!(inspect_reviewed_installer("1.18.2", "40.2.21", r.installer_sha1, profile, version).is_err());
    }
    #[test]
    fn legacy_profile_is_not_treated_as_modern_processor_graph() {
        let profile = r#"{"minecraft":"1.12.2","install":{"minecraft":"1.12.2"}}"#;
        let version = r#"{"id":"1.12.2-forge-14.23.5.2860","inheritsFrom":"1.12.2","mainClass":"net.minecraft.launchwrapper.Launch"}"#;
        let release = reviewed_cross_generation("1.12.2", "14.23.5.2860").unwrap();
        let inspected = inspect_reviewed_installer("1.12.2", "14.23.5.2860", release.installer_sha1, profile, version).unwrap();
        assert_eq!(inspected.selected_processor_count, 0);
        assert!(!inspected.ready_to_execute);
        let malicious = r#"{"minecraft":"1.12.2","install":{},"processors":[{"jar":"x:y:1"}]}"#;
        assert!(inspect_reviewed_installer("1.12.2", "14.23.5.2860", release.installer_sha1, malicious, version).is_err());
    }
    #[test]
    fn neoforge_candidates_are_exact_and_not_wildcard_versions() {
        assert_eq!(neoforge_candidate("1.21.1", "21.1.80").unwrap().java_major, 21);
        assert!(neoforge_candidate("1.20.1", "21.1.80").is_err());
        assert!(neoforge_candidate("1.21.1", "21.1.81").is_err());
    }
}
