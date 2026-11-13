//! Offline identity contract. Application UUID is NEVER the offline Minecraft UUID.
//! Vanilla server behavior must be tested by Chat 5 against the target game version.
use serde::{Deserialize,Serialize};
use uuid::Uuid;
use crate::{CoreError,OfflineProfile,validate_username};

#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
pub struct OfflineGameIdentity {
    pub profile_id: Uuid,
    pub username: String,
    pub effective_game_uuid: Uuid,
    pub offline_server_uuid: Uuid,
    pub algorithm: String,
}

#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
pub struct RenameImpact {
    pub profile_id: Uuid,
    pub old_username: String,
    pub new_username: String,
    pub old_offline_server_uuid: Uuid,
    pub new_offline_server_uuid: Uuid,
    pub requires_world_backup: bool,
    pub inventory_may_change: bool,
}
/// Minecraft Java offline UUID formula used by vanilla offline-mode server:
/// MD5(UTF8("OfflinePlayer:" + exact_case_username)), with RFC4122 v3 bits.
/// Does not imply authenticated identity, anti-spoofing, or support for online servers.
pub fn vanilla_offline_server_uuid(username: &str) -> Result<Uuid,CoreError> {
    validate_username(username)?;
    let digest = md5::compute(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes=digest.0;
    bytes[6]=(bytes[6]&0x0f)|0x30;
    bytes[8]=(bytes[8]&0x3f)|0x80;
    Ok(Uuid::from_bytes(bytes))
}
pub fn derive_vanilla_offline_identity(profile: &OfflineProfile) -> Result<OfflineGameIdentity,CoreError>{
    let uuid=vanilla_offline_server_uuid(&profile.username)?;
    Ok(OfflineGameIdentity{
        profile_id:profile.id, username:profile.username.clone(),
        effective_game_uuid:uuid, offline_server_uuid:uuid,
        algorithm:"vanilla_offline_md5_v3".into()
    })
}
pub fn preview_rename_impact(profile: &OfflineProfile,new_username: &str)->Result<RenameImpact,CoreError>{
    let old=vanilla_offline_server_uuid(&profile.username)?;
    let new=vanilla_offline_server_uuid(new_username)?;
    Ok(RenameImpact{
        profile_id:profile.id,old_username:profile.username.clone(),
        new_username:new_username.into(),old_offline_server_uuid:old,
        new_offline_server_uuid:new,requires_world_backup:old!=new,
        inventory_may_change:old!=new
    })
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn identity_not_internal_profile_id() {
        let profile=OfflineProfile::new("Notch").unwrap();
        let i=derive_vanilla_offline_identity(&profile).unwrap();
        assert_ne!(profile.id,i.offline_server_uuid);
        // Standard vanilla offline-mode fixture for "Notch".
        assert_eq!(i.offline_server_uuid.to_string(),"b50ad385-829d-3141-a216-7e7d7539ba7f");
    }
    #[test]fn renamed_player_must_not_silently_claim_inventory_preserved(){
        let profile=OfflineProfile::new("PlayerOne").unwrap();
        let impact=preview_rename_impact(&profile,"PlayerTwo").unwrap();
        assert!(impact.requires_world_backup);
        assert!(impact.inventory_may_change);
        assert_ne!(impact.old_offline_server_uuid,impact.new_offline_server_uuid);
        assert_eq!(impact.profile_id,profile.id);
    }
    #[test]fn same_name_is_stable_and_case_sensitive(){
        let p=OfflineProfile::new("CaseName").unwrap();
        let impact=preview_rename_impact(&p,"CaseName").unwrap();
        assert!(!impact.requires_world_backup);
        assert_ne!(vanilla_offline_server_uuid("CaseName").unwrap(),vanilla_offline_server_uuid("casename").unwrap());
    }
}