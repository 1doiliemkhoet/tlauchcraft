//! Typed adapter to Foundation contract v1; never invent profile identities.
use tl_core::{
    validate_username, CoreError, GameInstance, LaunchRequest, LoaderKind, OfflineProfile,
};

/// Check persistent identity and instance binding before preparing a local game process.
/// This does not assert that the local profile has a Minecraft entitlement.
fn validate_persisted_identity_and_memory(
    instance: &GameInstance,
    profile: &OfflineProfile,
    request: &LaunchRequest,
) -> Result<(), CoreError> {
    instance.validate()?;
    validate_username(&profile.username)?;
    if instance.profile_id != profile.id
        || request.profile_id != profile.id
        || request.instance_id != instance.id
    {
        return Err(CoreError::InvalidInput);
    }
    if request.min_ram_mb.is_some_and(|value| value < 256)
        || request.max_ram_mb.is_some_and(|value| value > 65_536)
        || matches!(
            (request.min_ram_mb, request.max_ram_mb),
            (Some(min), Some(max)) if min > max
        )
    {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}

/// Legacy compatibility: Vanilla-only consumers keep the original strict gate.
pub fn validate_launch_context(
    instance: &GameInstance,
    profile: &OfflineProfile,
    request: &LaunchRequest,
) -> Result<(), CoreError> {
    validate_persisted_identity_and_memory(instance, profile, request)?;
    if instance.loader.kind != LoaderKind::Vanilla {
        return Err(CoreError::UnsupportedVersion);
    }
    Ok(())
}

/// Forge callers must explicitly opt into a version-selected loader adapter.
/// This checks identity and request safety only; it never certifies Forge files,
/// execution, or an instance's READY state.
pub fn validate_forge_launch_context(
    instance: &GameInstance,
    profile: &OfflineProfile,
    request: &LaunchRequest,
) -> Result<(), CoreError> {
    validate_persisted_identity_and_memory(instance, profile, request)?;
    if instance.loader.kind != LoaderKind::Forge
        || instance.loader.version.as_deref().is_none_or(str::is_empty)
    {
        return Err(CoreError::UnsupportedVersion);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tl_core::LoaderSpec;

    #[test]
    fn forge_launch_binding_requires_explicit_adapter_without_vanilla_regression() {
        let profile = OfflineProfile::new("OfflinePeer").unwrap();
        let forge = GameInstance::new(
            "Forge Pack",
            "1.20.1",
            LoaderSpec {
                kind: LoaderKind::Forge,
                version: Some("47.3.0".into()),
            },
            profile.id,
        )
        .unwrap();
        let request = LaunchRequest {
            instance_id: forge.id,
            profile_id: profile.id,
            java_path: None,
            min_ram_mb: None,
            max_ram_mb: None,
            extra_jvm_args: None,
        };
        assert!(validate_launch_context(&forge, &profile, &request).is_err());
        assert!(validate_forge_launch_context(&forge, &profile, &request).is_ok());
        let other = OfflineProfile::new("OtherPeer").unwrap();
        assert!(validate_forge_launch_context(&forge, &other, &request).is_err());
    }

    #[test]
    fn stable_identity_and_request_binding() {
        let profile = OfflineProfile::new("Player_01").unwrap();
        let instance = GameInstance::new(
            "Vanilla",
            "1.21.1",
            LoaderSpec {
                kind: LoaderKind::Vanilla,
                version: None,
            },
            profile.id,
        )
        .unwrap();
        let request = LaunchRequest {
            instance_id: instance.id,
            profile_id: profile.id,
            java_path: None,
            min_ram_mb: Some(512),
            max_ram_mb: Some(2048),
            extra_jvm_args: None,
        };
        assert!(validate_launch_context(&instance, &profile, &request).is_ok());
        let different = OfflineProfile::new("Different").unwrap();
        assert!(validate_launch_context(&instance, &different, &request).is_err());
    }
}
