use crate::state::ApiError;

pub fn validate_username(username: &str) -> Result<(), ApiError> {
    let name = username.trim();
    if name.len() < 3 || name.len() > 16 {
        return Err(ApiError::BadRequest(
            "username must be 3-16 characters".to_string(),
        ));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ApiError::BadRequest(
            "username may only contain A-Z, a-z, 0-9 and _".to_string(),
        ));
    }
    Ok(())
}
