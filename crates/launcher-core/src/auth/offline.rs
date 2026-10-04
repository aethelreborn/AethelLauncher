use md5::{Digest, Md5};
use uuid::Uuid;

pub fn offline_uuid(name: &str) -> Uuid {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{name}").as_bytes());
    let digest = hasher.finalize();
    let mut b: [u8; 16] = digest.into();
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    Uuid::from_bytes(b)
}

pub fn validate_username(name: &str) -> Result<(), String> {
    if name.len() < 3 || name.len() > 16 {
        return Err(format!(
            "Username must be 3-16 characters, got {}",
            name.len()
        ));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("Username may only contain A-Z, a-z, 0-9, and _".to_string());
    }
    if name.starts_with('_') || name.ends_with('_') {
        return Err("Username cannot start or end with _".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct OfflineAccount {
    pub name: String,
    pub uuid: Uuid,
}

impl OfflineAccount {
    pub fn new(name: &str) -> Result<Self, String> {
        validate_username(name)?;
        Ok(Self {
            name: name.to_string(),
            uuid: offline_uuid(name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_username_validation() {
        assert!(validate_username("Steve").is_ok());
        assert!(validate_username("aethel_01").is_ok());
        assert!(validate_username("x").is_err());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("a b").is_err());
        assert!(validate_username("_start").is_err());
        assert!(validate_username("end_").is_err());
        assert!(validate_username("steve").is_ok());
        assert!(validate_username("STEVE").is_ok());
    }

    #[test]
    fn test_offline_account_creation() {
        let account = OfflineAccount::new("TestUser").expect("valid username");
        assert_eq!(account.name, "TestUser");
        assert_eq!(account.uuid.get_version(), Some(uuid::Version::Md5));
    }

    #[test]
    fn test_uuid_deterministic() {
        let u1 = offline_uuid("Steve");
        let u2 = offline_uuid("Steve");
        assert_eq!(u1, u2);
    }
}
