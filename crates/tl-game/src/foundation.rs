//! Typed adapter to Foundation contract v1; never invent profile identities.
use tl_core::{
    validate_username, CoreError, GameInstance, LaunchRequest, LoaderKind, OfflineProfile,
};

/// Check persistent identity and instance binding before preparing a local game process.
/// This does not assert that the local profile has a Minecraft entitlement.
pub fn validate_launch_context(
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
    if !matches!(instance.loader.kind, LoaderKind::Vanilla) {
        return Err(CoreError::UnsupportedVersion);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tl_core::LoaderSpec;

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
