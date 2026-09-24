//! Offline authentication — deterministic UUID v3 derivation matching vanilla behavior.
//!
//! See [06 · Auth](../../../opencode-docs/06-auth.md) for the full spec.

use md5::{Digest, Md5};
use uuid::Uuid;

/// Derive a deterministic offline UUID from a username.
///
/// Matches Mojang's exact algorithm: MD5("OfflinePlayer:" + name),
/// then set version 3 / variant 1 bits.
pub fn offline_uuid(name: &str) -> Uuid {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{name}").as_bytes());
    let digest = hasher.finalize();
    let mut b: [u8; 16] = digest.into();
    // Set version = 3 (MD5-based)
    b[6] = (b[6] & 0x0f) | 0x30;
    // Set variant = RFC 4122 (10xxxxxx)
    b[8] = (b[8] & 0x3f) | 0x80;
    Uuid::from_bytes(b)
}

/// Validate a Minecraft username per [06 §2.1].
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
        assert!(validate_username("x").is_err()); // too short
        assert!(validate_username("ab").is_err()); // too short
        assert!(validate_username("a b").is_err()); // whitespace
        assert!(validate_username("_start").is_err()); // leading underscore
        assert!(validate_username("end_").is_err()); // trailing underscore
        assert!(validate_username("steve").is_ok()); // all lowercase is fine
        assert!(validate_username("STEVE").is_ok()); // all uppercase is fine
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
        assert_eq!(u1, u2); // same input → same UUID
    }
}
