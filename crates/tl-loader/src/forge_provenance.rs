//! Offline provenance audit for official loader artifact metadata.
//! Audit is evidence planning, NOT download, execution, or installation.
use crate::{maven, LoaderError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceRequirement {
    /// Official metadata pins both the URL and checksum; local cache still needs checking.
    CanonicalMaven { url: String },
    /// An empty URL is not an invitation to construct a mirror or download.
    VerifiedInstallerOrGenerated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRequirement {
    pub coordinate: String,
    pub relative_path: String,
    pub sha1: String,
    pub size_bytes: u64,
    pub source: SourceRequirement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeProvenanceAudit {
    pub minecraft_version: String,
    pub derived_version: String,
    pub profile_libraries: Vec<ArtifactRequirement>,
    pub derived_libraries: Vec<ArtifactRequirement>,
    /// These entries must be reconciled with verified installer extraction,
    /// upstream Mojang metadata or fully verified processor output.
    pub needs_action: Vec<String>,
    /// The client patch lives in the verified Forge installer ZIP, not user .minecraft.
    pub embedded_client_patch: Option<String>,
    pub installed: bool,
}

pub fn audit_official_forge_metadata(
    profile_json: &str,
    derived: &Value,
) -> Result<ForgeProvenanceAudit, LoaderError> {
    let profile: Value = serde_json::from_str(profile_json)
        .map_err(|e| LoaderError::InvalidMetadata(format!("forge profile JSON: {e}")))?;
    let minecraft = profile["minecraft"].as_str().ok_or(LoaderError::InvalidInput)?;
    let version = profile["version"].as_str().ok_or(LoaderError::InvalidInput)?;
    if derived["id"].as_str() != Some(version)
        || derived["inheritsFrom"].as_str() != Some(minecraft)
    {
        return Err(LoaderError::CompatibilityConflict);
    }
    let mut needs_action = Vec::new();
    let profile_libraries = inspect_libraries(&profile["libraries"], "profile", &mut needs_action)?;
    let derived_libraries = inspect_libraries(&derived["libraries"], "derived", &mut needs_action)?;
    let embedded_client_patch = profile["data"]["BINPATCH"]["client"]
        .as_str()
        .map(str::to_owned);
    if let Some(ref path) = embedded_client_patch {
        if !path.starts_with("/data/")
            || !path.ends_with(".lzma")
            || path.contains("..")
            || path.contains('\\')
            || path.contains(':')
        {
            return Err(LoaderError::InvalidMetadata("unsafe Forge embedded patch entry".into()));
        }
        needs_action.push(format!("verified-installer-entry:{}", path.trim_start_matches('/')));
    } else {
        needs_action.push("missing official BINPATCH entry descriptor".into());
    }
    needs_action.push(format!("verified-mojang-client:{minecraft}"));
    Ok(ForgeProvenanceAudit {
        minecraft_version: minecraft.to_owned(),
        derived_version: version.to_owned(),
        profile_libraries,
        derived_libraries,
        needs_action,
        embedded_client_patch,
        installed: false,
    })
}

fn inspect_libraries(
    value: &Value,
    collection: &str,
    missing: &mut Vec<String>,
) -> Result<Vec<ArtifactRequirement>, LoaderError> {
    let entries = value.as_array().ok_or_else(|| {
        LoaderError::InvalidMetadata(format!("{collection} libraries missing"))
    })?;
    if entries.is_empty() || entries.len() > 1024 {
        return Err(LoaderError::InvalidMetadata("invalid Forge library count".into()));
    }
    let mut result = Vec::new();
    for lib in entries {
        let coordinate = lib["name"].as_str().ok_or(LoaderError::InvalidInput)?;
        let item = &lib["downloads"]["artifact"];
        let path = item["path"].as_str().ok_or(LoaderError::InvalidInput)?;
        let sha1 = item["sha1"].as_str().ok_or(LoaderError::InvalidInput)?;
        let bytes = item["size"].as_u64().ok_or(LoaderError::InvalidInput)?;
        let url = item["url"].as_str().ok_or(LoaderError::InvalidInput)?;
        if bytes == 0 || sha1.len() != 40 || !sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(LoaderError::InvalidMetadata(format!("missing official checksum/size: {coordinate}")));
        }
        let relative = if let Some((base, ext)) = coordinate.rsplit_once('@') {
            if !matches!(ext, "zip" | "txt") { return Err(LoaderError::InvalidInput); }
            let jar = maven::artifact_path(base)?;
            format!("{}.{}", jar.strip_suffix(".jar").ok_or(LoaderError::InvalidInput)?, ext)
        } else {
            maven::artifact_path(coordinate)?
        };
        if relative != path {
            return Err(LoaderError::InvalidMetadata(format!("official path differs from Maven coordinate: {coordinate}")));
        }
        let source = if url.is_empty() {
            missing.push(format!("source:{collection}:{coordinate}:{sha1}"));
            SourceRequirement::VerifiedInstallerOrGenerated
        } else {
            let canonical = [
                "https://maven.minecraftforge.net",
                "https://repo.maven.apache.org/maven2",
                "https://libraries.minecraft.net",
            ]
            .iter()
            .any(|host| url == format!("{host}/{path}"));
            if !canonical {
                return Err(LoaderError::InvalidMetadata(format!("noncanonical Maven source: {coordinate}")));
            }
            SourceRequirement::CanonicalMaven { url: url.to_owned() }
        };
        result.push(ArtifactRequirement {
            coordinate: coordinate.to_owned(),
            relative_path: path.to_owned(),
            sha1: sha1.to_ascii_lowercase(),
            size_bytes: bytes,
            source,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const PROFILE: &str = include_str!("../tests/fixtures/forge_36229_official_profile.json");
    const VERSION: &str = include_str!("../tests/fixtures/forge_36229_official_version.json");
    #[test]
    fn real_official_inventory_has_two_source_gaps_and_embedded_patch() {
        let audit = audit_official_forge_metadata(PROFILE, &serde_json::from_str(VERSION).unwrap()).unwrap();
        assert_eq!(audit.profile_libraries.len(), 39);
        assert_eq!(audit.derived_libraries.len(), 26);
        assert_eq!(audit.embedded_client_patch.as_deref(), Some("/data/client.lzma"));
        assert!(audit.needs_action.iter().any(|x| x.contains("forge:1.16.5-36.2.29:universal")));
        assert!(audit.needs_action.iter().any(|x| x.contains("source:derived:net.minecraftforge:forge:1.16.5-36.2.29")));
        assert!(audit.needs_action.iter().any(|x| x.contains("verified-mojang-client:1.16.5")));
        assert!(!audit.installed);
    }
    #[test]
    fn rejects_manifest_mirror_tampering() {
        let mut p: Value = serde_json::from_str(PROFILE).unwrap();
        p["libraries"][0]["downloads"]["artifact"]["url"] = json!("https://mirror.example.net/bad.jar");
        assert!(audit_official_forge_metadata(&p.to_string(), &serde_json::from_str(VERSION).unwrap()).is_err());
    }
    #[test]
    fn rejects_forged_universal_checksum_and_path() {
        let mut p: Value = serde_json::from_str(PROFILE).unwrap();
        let libraries = p["libraries"].as_array_mut().unwrap();
        let universal = libraries.iter_mut().find(|x| x["name"].as_str().unwrap().contains(":universal")).unwrap();
        universal["downloads"]["artifact"]["sha1"] = json!("fake");
        assert!(audit_official_forge_metadata(&p.to_string(), &serde_json::from_str(VERSION).unwrap()).is_err());
    }
}
