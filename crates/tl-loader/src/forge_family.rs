//! Audited Minecraft 1.16.5 Forge installer family. Every permitted
//! installer is bound to the official published SHA-1 and reviewed metadata;
//! this is NOT a wildcard permit for arbitrary Forge releases.
use crate::{forge_1165::forge_1165_client_blueprint, LoaderError};
use serde_json::Value;
use sha1::{Digest, Sha1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovedForgeRelease {
    pub minecraft: &'static str,
    pub forge: &'static str,
    pub installer_sha1: &'static str,
    pub profile_sha1: &'static str,
    pub derived_canonical_sha1: &'static str,
    pub installertools: &'static str,
    pub patched_sha1: &'static str,
    pub profile_libraries: usize,
}

pub const FORGE_1165_RELEASES: &[ApprovedForgeRelease] = &[
    ApprovedForgeRelease {
        minecraft: "1.16.5",
        forge: "36.2.29",
        installer_sha1: "7d247279cf06eed6ffcf609f36d11e34c70350f3",
        profile_sha1: "c0cad8e48ccc275c476733be75f1ee2fff7da5dc",
        derived_canonical_sha1: "6c24a13715ae66a78dbc8cd56069a1e60fd810e3",
        installertools: "net.minecraftforge:installertools:1.2.6",
        patched_sha1: "42710a50971139d0f218e438a211a0dbe9bbb3d2",
        profile_libraries: 39,
    },
    ApprovedForgeRelease {
        minecraft: "1.16.5",
        forge: "36.2.34",
        installer_sha1: "ab306b654c44d659ce69da5d4f87590b66dc91e8",
        profile_sha1: "39afdc71d5568710b096c52aa25d36b502f040ba",
        derived_canonical_sha1: "abcb4a9826e1d6461feb9e4f5879d99e3f4d6210",
        installertools: "net.minecraftforge:installertools:1.2.6",
        patched_sha1: "f0b1ece7b16290e61d1cd76b67b94ae499ecdd9a",
        profile_libraries: 39,
    },
    ApprovedForgeRelease {
        minecraft: "1.16.5",
        forge: "36.2.42",
        installer_sha1: "e09ecf910e4d5eae12fb3564d9b7de212c1958b2",
        profile_sha1: "1937994ba2c8ab39294542d4ef5daa74e1377242",
        derived_canonical_sha1: "68b823079cbf2a7fcfc9a203f05c4e4e75e8a4ea",
        installertools: "net.minecraftforge:installertools:1.3.0",
        patched_sha1: "427f13ce6f486c182c9693e94e907e2eb049773f",
        profile_libraries: 40,
    },
];

pub fn approved_forge_release(
    minecraft: &str,
    forge: &str,
) -> Result<&'static ApprovedForgeRelease, LoaderError> {
    FORGE_1165_RELEASES
        .iter()
        .find(|release| release.minecraft == minecraft && release.forge == forge)
        .ok_or_else(|| {
            LoaderError::UnsupportedVersion(format!(
                "Forge {minecraft}-{forge}: official installer not reviewed/pinned"
            ))
        })
}

/// All independently reviewed release families. Legacy callers may continue
/// using approved_forge_release to explicitly require a 1.16.5 release.
pub fn reviewed_forge_release(
    minecraft: &str,
    forge: &str,
) -> Result<&'static ApprovedForgeRelease, LoaderError> {
    if (minecraft, forge) == ("1.20.1", "47.4.0") {
        return Ok(&FORGE_1201_RELEASE);
    }
    approved_forge_release(minecraft, forge)
}

pub const FORGE_1201_RELEASE: ApprovedForgeRelease = ApprovedForgeRelease {
    minecraft: "1.20.1",
    forge: "47.4.0",
    installer_sha1: FORGE_1201_INSTALLER_SHA1,
    profile_sha1: FORGE_1201_PROFILE_SHA1,
    derived_canonical_sha1: FORGE_1201_VERSION_SHA1,
    installertools: "net.minecraftforge:installertools:1.4.1",
    patched_sha1: "b268fd6fb2c4829a72e8d0d6c75b7fefbec8479c",
    profile_libraries: 36,
};

impl ApprovedForgeRelease {
    pub fn java_major(self) -> u32 {
        if self.minecraft == "1.20.1" {
            FORGE_1201_JAVA_MAJOR
        } else {
            8
        }
    }

    pub fn processor_count(self) -> usize {
        if self.minecraft == "1.20.1" {
            6
        } else {
            4
        }
    }

    pub fn main_class(self) -> &'static str {
        if self.minecraft == "1.20.1" {
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        } else {
            "cpw.mods.modlauncher.Launcher"
        }
    }

    pub fn client_blueprint(
        self,
        profile: &str,
        derived: &Value,
    ) -> Result<crate::forge_1165::ForgeClientBlueprint, LoaderError> {
        if self.minecraft == "1.20.1" {
            audit_forge_1201_metadata(profile, derived)?;
            let mut plan = crate::forge_1165::client_blueprint(
                profile,
                derived,
                self.minecraft,
                self.forge,
                self.main_class(),
            )?;
            // Forge 47 declares the final digest in data, not outputs.
            // Mojang mappings are downloaded by an official processor and
            // independently pinned to the verified Mojang parent metadata.
            plan.steps[1]
                .outputs
                .insert("{MOJMAPS}".into(), "{MOJMAPS_SHA}".into());
            plan.steps[5]
                .outputs
                .insert("{PATCHED}".into(), "{PATCHED_SHA}".into());
            Ok(plan)
        } else {
            forge_1165_client_blueprint(profile, derived, self.forge)
        }
    }

    pub fn installer_filename(self) -> String {
        format!("forge-{}-{}-installer.jar", self.minecraft, self.forge)
    }

    pub fn installer_url(self) -> String {
        format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{}-{}/{}",
            self.minecraft,
            self.forge,
            self.installer_filename()
        )
    }

    /// The release's metadata is authenticated by the SHA-1-pinned installer,
    /// and additionally compared to audited independent metadata fingerprints.
    /// A new processor family needs a new explicitly reviewed adapter.
    pub fn verify_metadata(self, profile_json: &str, derived: &Value) -> Result<(), LoaderError> {
        if self.minecraft == "1.20.1" {
            return audit_forge_1201_metadata(profile_json, derived);
        }
        let profile = serde_json::from_str::<Value>(profile_json)
            .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
        let profile_hash = format!("{:x}", Sha1::digest(profile_json.as_bytes()));
        let canonical_derived =
            serde_json::to_vec(derived).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
        let derived_hash = format!("{:x}", Sha1::digest(&canonical_derived));
        // Preserve the originally shipped, semantically pinned 36.2.29
        // fixtures. They may have different whitespace from the ZIP member,
        // but represent the exact verified official JSON trees. New releases
        // additionally pin the raw profile and canonical derived fingerprints.
        let metadata_matches = if self.forge == "36.2.29" {
            let baseline_profile: Value = serde_json::from_str(include_str!(
                "../tests/fixtures/forge_36229_official_profile.json"
            ))
            .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
            let baseline_version: Value = serde_json::from_str(include_str!(
                "../tests/fixtures/forge_36229_official_version.json"
            ))
            .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
            profile == baseline_profile && derived == &baseline_version
        } else {
            profile_hash == self.profile_sha1 && derived_hash == self.derived_canonical_sha1
        };
        if !metadata_matches
            || profile["libraries"].as_array().map(Vec::len) != Some(self.profile_libraries)
            || derived["libraries"].as_array().map(Vec::len) != Some(26)
            || profile["data"]["PATCHED_SHA"]["client"]
                .as_str()
                .map(|s| s.trim_matches('\''))
                != Some(self.patched_sha1)
        {
            return Err(LoaderError::InvalidMetadata(
                "installer profile/version differs from reviewed Forge release".into(),
            ));
        }
        let blueprint = forge_1165_client_blueprint(profile_json, derived, self.forge)?;
        let expected = [
            self.installertools,
            "net.minecraftforge:jarsplitter:1.1.4",
            "net.md-5:SpecialSource:1.8.5",
            "net.minecraftforge:binarypatcher:1.0.12",
        ];
        if blueprint.steps.len() != expected.len()
            || !blueprint
                .steps
                .iter()
                .zip(expected)
                .all(|(step, jar)| step.jar == jar)
        {
            return Err(LoaderError::InvalidMetadata(
                "unapproved Forge processor graph".into(),
            ));
        }
        Ok(())
    }
}

/// Reviewed Forge 47.4.0 metadata identity (CI run 44444507662).
/// Modern release pins; execution still requires the verified Windows runner and consent.
pub const FORGE_1201_INSTALLER_SHA1: &str = "b73203963ea70aee0c38dc58de1f6d16a766605b";
pub const FORGE_1201_PROFILE_SHA1: &str = "4d39915a30ca8e8da5e05cc794f9139fe035e723";
pub const FORGE_1201_VERSION_SHA1: &str = "280a9c77703ba10a3c16b5a918423eace78bb0bd";
pub const FORGE_1201_JAVA_MAJOR: u32 = 17;

/// Validate the complete metadata pair from the checksum-verified archive,
/// including the modern client graph, without granting runtime approval.
pub fn audit_forge_1201_metadata(profile_json: &str, derived: &Value) -> Result<(), LoaderError> {
    let profile: Value = serde_json::from_str(profile_json)
        .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let canonical =
        serde_json::to_vec(derived).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    if format!("{:x}", Sha1::digest(profile_json.as_bytes())) != FORGE_1201_PROFILE_SHA1
        || format!("{:x}", Sha1::digest(&canonical)) != FORGE_1201_VERSION_SHA1
        || profile["minecraft"] != "1.20.1"
        || profile["version"] != "1.20.1-forge-47.4.0"
        || profile["data"]["PATCHED_SHA"]["client"]
            .as_str()
            .map(|s| s.trim_matches('\''))
            != Some(FORGE_1201_RELEASE.patched_sha1)
        || derived["id"] != "1.20.1-forge-47.4.0"
        || derived["inheritsFrom"] != "1.20.1"
        || derived["mainClass"] != "cpw.mods.bootstraplauncher.BootstrapLauncher"
        || profile["libraries"].as_array().map(Vec::len) != Some(36)
        || derived["libraries"].as_array().map(Vec::len) != Some(29)
    {
        return Err(LoaderError::InvalidMetadata(
            "Forge 47.4.0 metadata differs from audited release".into(),
        ));
    }
    let processors = profile["processors"].as_array().ok_or_else(|| {
        LoaderError::InvalidMetadata("Forge 47.4.0 processor graph missing".into())
    })?;
    let mut steps = Vec::new();
    for step in processors {
        if let Some(sides) = step.get("sides") {
            let sides = sides.as_array().ok_or(LoaderError::InvalidInput)?;
            if !sides.iter().any(|side| side == "client") {
                continue;
            }
        }
        steps.push(step["jar"].as_str().ok_or(LoaderError::InvalidInput)?);
    }
    if steps
        != [
            "net.minecraftforge:installertools:1.4.1",
            "net.minecraftforge:installertools:1.4.1",
            "net.minecraftforge:installertools:1.4.1",
            "net.minecraftforge:jarsplitter:1.1.4",
            "net.minecraftforge:ForgeAutoRenamingTool:0.1.22:all",
            "net.minecraftforge:binarypatcher:1.1.1",
        ]
    {
        return Err(LoaderError::InvalidMetadata(
            "Forge 47.4.0 processor chain mismatch".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modern_audit_does_not_authorize_legacy_runner() {
        assert!(approved_forge_release("1.20.1", "47.4.0").is_err());
        let modern = reviewed_forge_release("1.20.1", "47.4.0").unwrap();
        assert_eq!(modern.java_major(), 17);
        assert_eq!(modern.processor_count(), 6);
        assert!(reviewed_forge_release("1.20.1", "47.3.0").is_err());
        assert!(audit_forge_1201_metadata("{}", &serde_json::json!({})).is_err());
    }
    #[test]
    fn only_individually_pinned_versions_are_permitted() {
        for version in ["36.2.29", "36.2.34", "36.2.42"] {
            let release = approved_forge_release("1.16.5", version).unwrap();
            assert_eq!(release.installer_sha1.len(), 40);
            assert_eq!(
                release.installer_filename(),
                format!("forge-1.16.5-{version}-installer.jar")
            );
        }
        for (mc, forge) in [
            ("1.16.5", "36.2.35"),
            ("1.16.5", "36.2.43"),
            ("1.20.1", "47.3.0"),
            ("1.21.1", "52.0.0"),
        ] {
            assert!(approved_forge_release(mc, forge).is_err());
        }
    }
    #[test]
    fn real_audited_baseline_rejects_tampered_metadata() {
        let profile = include_str!("../tests/fixtures/forge_36229_official_profile.json");
        let derived: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/forge_36229_official_version.json"
        ))
        .unwrap();
        let release = approved_forge_release("1.16.5", "36.2.29").unwrap();
        // The fixtures were extracted from the authentic 36.2.29 installer.
        assert!(release.verify_metadata(profile, &derived).is_ok());
        let altered = profile.replace("36.2.29", "36.2.34");
        assert!(release.verify_metadata(&altered, &derived).is_err());
    }
}
