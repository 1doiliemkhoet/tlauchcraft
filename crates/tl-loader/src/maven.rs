//! Maven coordinates are parsed, never interpreted as shell commands or filesystem paths.
use crate::{validate_component, Artifact, LoaderError};

pub fn artifact_path(coordinate: &str) -> Result<String, LoaderError> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if !(3..=4).contains(&parts.len()) {
        return Err(LoaderError::InvalidInput);
    }
    for p in &parts {
        validate_component(p)?;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = if parts.len() == 4 {
        format!("-{}", parts[3])
    } else {
        String::new()
    };
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.jar"
    ))
}

pub fn artifact(
    repo: &str,
    coordinate: &str,
    sha1: Option<String>,
) -> Result<Artifact, LoaderError> {
    if !repo.starts_with("https://")
        || repo.contains('@')
        || repo.contains('?')
        || repo.contains('#')
    {
        return Err(LoaderError::InvalidInput);
    }
    let relative_path = artifact_path(coordinate)?;
    if let Some(ref sha) = sha1 {
        if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(LoaderError::InvalidMetadata("invalid SHA-1".into()));
        }
    }
    let base = repo.trim_end_matches('/');
    Ok(Artifact {
        url: format!("{base}/{relative_path}"),
        relative_path,
        expected_sha1: sha1,
    })
}

/// Validate a Maven JAR path provided by an already approved loader plan.
/// A profile cannot substitute a relative traversal for a published coordinate.
pub fn artifact_path_from_relative(relative: &str) -> Result<(), LoaderError> {
    if relative.is_empty()
        || relative.len() > 1024
        || !relative.ends_with(".jar")
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative.contains(':')
    {
        return Err(LoaderError::InvalidInput);
    }
    for component in relative.split('/') {
        validate_component(component)?;
    }
    Ok(())
}

/// Official Maven XML catalog. Parser is deliberately network-free.
pub fn parse_catalog(xml: &str) -> Result<Vec<String>, LoaderError> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| LoaderError::InvalidMetadata(e.to_string()))?;
    if doc.root_element().tag_name().name() != "metadata" {
        return Err(LoaderError::InvalidMetadata(
            "expected Maven metadata".into(),
        ));
    }
    let mut values = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("version")) {
        let version = node.text().ok_or(LoaderError::InvalidInput)?;
        validate_component(version)?;
        values.push(version.to_owned());
    }
    if values.is_empty() {
        return Err(LoaderError::InvalidMetadata("empty version catalog".into()));
    }
    Ok(values)
}

/// Forge and NeoForge require their official Java installers to run processor steps;
/// an installer JAR alone is not a runnable Minecraft version.
pub fn installer_plan(
    minecraft_version: &str,
    spec: &crate::LoaderSpec,
    repository: &str,
    coordinate: &str,
    java_major: u32,
) -> Result<crate::LoaderPlan, LoaderError> {
    use crate::{LoaderKind, LoaderPlan};
    if !matches!(&spec.kind, LoaderKind::Forge | LoaderKind::NeoForge) {
        return Err(LoaderError::InvalidInput);
    }
    validate_component(minecraft_version)?;
    let loader_version = spec.version.as_deref().ok_or(LoaderError::InvalidInput)?;
    validate_component(loader_version)?;
    if java_major == 0 {
        return Err(LoaderError::InvalidInput);
    }
    let mut path = artifact_path(coordinate)?;
    // Installer classifier is an explicitly supported Maven artifact, not arbitrary args.
    path = path
        .strip_suffix(".jar")
        .ok_or(LoaderError::InvalidInput)?
        .to_owned()
        + "-installer.jar";
    let base = repository.trim_end_matches('/');
    if base != crate::FORGE_MAVEN && base != crate::NEOFORGE_MAVEN {
        return Err(LoaderError::InvalidInput);
    }
    // A Maven XML catalog only confirms a coordinate. It never establishes
    // an installer checksum or grants Java execution permission.
    let expected_sha1 = if spec.kind == LoaderKind::Forge {
        crate::loader_matrix::reviewed_cross_generation(minecraft_version, loader_version)
            .ok()
            .map(|r| r.installer_sha1.to_owned())
            .or_else(|| {
                crate::forge_family::approved_forge_release(minecraft_version, loader_version)
                    .ok()
                    .map(|r| r.installer_sha1.to_owned())
            })
    } else {
        crate::loader_matrix::neoforge_candidate(minecraft_version, loader_version)
            .ok()
            .map(|r| r.installer_sha1.to_owned())
    };
    Ok(LoaderPlan {
        loader: spec.clone(),
        version_id: format!("{}-{}-{}", match &spec.kind { LoaderKind::Forge => "forge", _ => "neoforge" }, minecraft_version, loader_version),
        inherits_from: minecraft_version.to_owned(),
        main_class: None,
        libraries: vec![],
        required_java_major: java_major,
        installer: Some(Artifact { url: format!("{base}/{path}"), relative_path: path, expected_sha1 }),
        ready_for_metadata_merge: false,
        warnings: vec!["Catalog/installer planning only. A separate audited generation-specific executor must verify official metadata, Java, all processor outputs and the published instance before Play.".into()],
    })
}

pub struct Forge;
impl crate::LoaderInstaller for Forge {
    fn plan(
        &self,
        minecraft_version: &str,
        spec: &crate::LoaderSpec,
        metadata: &str,
        game_java_major: u32,
    ) -> Result<crate::LoaderPlan, LoaderError> {
        if spec.kind != crate::LoaderKind::Forge {
            return Err(LoaderError::InvalidInput);
        }
        let available = parse_catalog(metadata)?;
        let full = format!(
            "{}-{}",
            minecraft_version,
            spec.version.as_deref().ok_or(LoaderError::InvalidInput)?
        );
        if !available.contains(&full) {
            return Err(LoaderError::CompatibilityConflict);
        }
        // Preserve the version family's Java requirement independently of
        // what a metadata caller claims is installed on the host.
        let required_java = crate::loader_matrix::reviewed_cross_generation(
            minecraft_version, spec.version.as_deref().unwrap_or_default()
        )
        .ok()
        .map(|r| r.java_major)
        .or_else(|| {
            crate::forge_family::approved_forge_release(
                minecraft_version, spec.version.as_deref().unwrap_or_default()
            )
            .ok().map(|_| 8)
        })
        .unwrap_or(game_java_major);
        installer_plan(
            minecraft_version,
            spec,
            crate::FORGE_MAVEN,
            &format!("net.minecraftforge:forge:{full}"),
            required_java,
        )
    }
}

pub struct NeoForge;
impl crate::LoaderInstaller for NeoForge {
    fn plan(
        &self,
        minecraft_version: &str,
        spec: &crate::LoaderSpec,
        metadata: &str,
        _game_java_major: u32,
    ) -> Result<crate::LoaderPlan, LoaderError> {
        if spec.kind != crate::LoaderKind::NeoForge {
            return Err(LoaderError::InvalidInput);
        }
        let available = parse_catalog(metadata)?;
        if !available
            .iter()
            .any(|v| Some(v.as_str()) == spec.version.as_deref())
        {
            return Err(LoaderError::CompatibilityConflict);
        }
        // Explicit candidate metadata is still not trusted install evidence.
        let version = spec.version.as_deref().ok_or(LoaderError::InvalidInput)?;
        let candidate = crate::loader_matrix::neoforge_candidate(minecraft_version, version)?;
        let mut plan = installer_plan(
            minecraft_version, spec, crate::NEOFORGE_MAVEN,
            &format!("net.neoforged:neoforge:{version}"),
            candidate.java_major,
        )?;
        plan.warnings.push(
            "NeoForge game mapping remains an unexecuted candidate: verify the official installer hash, profile Minecraft binding and all processor outputs before publication".into(),
        );
        Ok(plan)
    }
}
