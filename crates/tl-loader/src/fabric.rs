//! Fabric profiles from the official /v2/versions/loader/{game}/{loader}/profile/json API.
use crate::{
    check_java, maven, validate_component, LoaderError, LoaderInstaller, LoaderKind, LoaderPlan,
    LoaderSpec,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Profile {
    id: String,
    #[serde(rename = "inheritsFrom")]
    inherits_from: String,
    #[serde(rename = "mainClass")]
    main_class: String,
    libraries: Vec<Library>,
}

#[derive(Deserialize)]
struct Library {
    name: String,
    url: String,
    #[serde(default)]
    sha1: Option<String>,
}

pub fn profile_url(game: &str, loader: &str) -> Result<String, LoaderError> {
    validate_component(game)?;
    validate_component(loader)?;
    Ok(format!(
        "{}/versions/loader/{game}/{loader}/profile/json",
        crate::FABRIC_META_BASE
    ))
}

pub fn parse_versions(json: &str) -> Result<Vec<String>, LoaderError> {
    #[derive(Deserialize)]
    struct Version {
        version: String,
    }
    let versions: Vec<Version> =
        serde_json::from_str(json).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    let mut out = Vec::new();
    for version in versions {
        validate_component(&version.version)?;
        out.push(version.version);
    }
    if out.is_empty() {
        return Err(LoaderError::InvalidMetadata(
            "empty Fabric version list".into(),
        ));
    }
    Ok(out)
}

pub struct Fabric;

impl LoaderInstaller for Fabric {
    fn plan(
        &self,
        minecraft_version: &str,
        spec: &LoaderSpec,
        metadata: &str,
        game_java_major: u32,
    ) -> Result<LoaderPlan, LoaderError> {
        if spec.kind != LoaderKind::Fabric {
            return Err(LoaderError::InvalidInput);
        }
        let loader_version = spec.version.as_deref().ok_or(LoaderError::InvalidInput)?;
        validate_component(minecraft_version)?;
        validate_component(loader_version)?;
        if game_java_major == 0 {
            return Err(LoaderError::InvalidInput);
        }
        let profile: Profile = serde_json::from_str(metadata)
            .map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
        if profile.inherits_from != minecraft_version {
            return Err(LoaderError::CompatibilityConflict);
        }
        let expected_id = format!("fabric-loader-{loader_version}-{minecraft_version}");
        if profile.id != expected_id {
            return Err(LoaderError::CompatibilityConflict);
        }
        if profile.main_class != "net.fabricmc.loader.impl.launch.knot.KnotClient"
            && profile.main_class != "net.fabricmc.loader.launch.knot.KnotClient"
        {
            return Err(LoaderError::InvalidMetadata(
                "unrecognized Fabric client main class".into(),
            ));
        }
        if profile.libraries.is_empty() {
            return Err(LoaderError::InvalidMetadata(
                "empty Fabric classpath".into(),
            ));
        }
        let mut libraries = Vec::new();
        for library in profile.libraries {
            let base = library.url.trim_end_matches('/');
            if base != "https://maven.fabricmc.net"
                && base != "https://repo.maven.apache.org/maven2"
                && base != "https://repo1.maven.org/maven2"
            {
                return Err(LoaderError::InvalidMetadata(format!(
                    "untrusted Maven repository: {base}"
                )));
            }
            libraries.push(maven::artifact(base, &library.name, library.sha1)?);
        }
        Ok(LoaderPlan {
            loader: spec.clone(),
            version_id: profile.id,
            inherits_from: profile.inherits_from,
            main_class: Some(profile.main_class),
            libraries,
            required_java_major: game_java_major,
            installer: None,
            ready_for_metadata_merge: true,
            warnings: vec![
                "Metadata only: tl-game must verify/download libraries, merge arguments and launch."
                    .into(),
            ],
        })
    }
}

pub fn validate_runtime(plan: &LoaderPlan, actual_major: u32) -> Result<(), LoaderError> {
    check_java(plan.required_java_major, actual_major)
}
