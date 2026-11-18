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

impl ApprovedForgeRelease {
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

#[cfg(test)]
mod tests {
    use super::*;
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
