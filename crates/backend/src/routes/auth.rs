use crate::middleware::auth::mint_token;
use crate::state::{ApiError, AppState, ACCESS_TTL_SECS, REFRESH_TTL_DAYS};
use crate::validate::validate_username;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub token_type: &'static str,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub status: &'static str,
}

pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    let pool = state.db()?;
    let email = normalize_email(&body.email)?;
    let username = body.username.trim().to_string();
    validate_username(&username)?;
    let password = body.password.as_str();
    if password.len() < 8 || password.len() > 128 {
        return Err(ApiError::BadRequest(
            "password must be 8-128 characters".to_string(),
        ));
    }

    let password_hash = hash_password(password)
        .map_err(|e| ApiError::BadRequest(format!("could not hash password: {e}")))?;

    let mut tx = pool.begin().await?;

    let (profile_id,): (uuid::Uuid,) = sqlx::query_as(
        "insert into public.profiles (username)
         values ($1)
         on conflict (username_ci) do update set updated_at = now()
         returning id",
    )
    .bind(&username)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "insert into public.wallets (player_id) values ($1) on conflict (player_id) do nothing",
    )
    .bind(profile_id)
    .execute(&mut *tx)
    .await?;

    let inserted = sqlx::query(
        "insert into public.auth_users (profile_id, email, password_hash)
         values ($1, $2, $3)
         on conflict do nothing",
    )
    .bind(profile_id)
    .bind(&email)
    .bind(&password_hash)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(ApiError::Conflict(
            "an account with that email or username already exists".to_string(),
        ));
    }

    tx.commit().await?;

    issue_tokens(&state, profile_id, &username, "user").await
}

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    let pool = state.db()?;
    let email = normalize_email(&body.email)?;

    let row: Option<(uuid::Uuid, uuid::Uuid, String, String, String)> = sqlx::query_as(
        "select u.id, u.profile_id, u.password_hash, u.role, p.username
           from public.auth_users u
           join public.profiles p on p.id = u.profile_id
          where lower(u.email) = lower($1)",
    )
    .bind(&email)
    .fetch_optional(pool)
    .await?;

    let Some((_, profile_id, password_hash, role, username)) = row else {
        let _ = verify_password(&body.password, DUMMY_HASH);
        return Err(ApiError::Unauthorized(
            "invalid email or password".to_string(),
        ));
    };

    if !verify_password(&body.password, &password_hash) {
        return Err(ApiError::Unauthorized(
            "invalid email or password".to_string(),
        ));
    }

    issue_tokens(&state, profile_id, &username, &role).await
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    let pool = state.db()?;
    let token_hash = hash_refresh_token(&body.refresh_token);

    let row: Option<(uuid::Uuid, String, String)> = sqlx::query_as(
        "update public.auth_refresh_tokens t
            set revoked_at = now()
           from public.auth_users u
           join public.profiles p on p.id = u.profile_id
          where t.token_hash = $1
            and t.revoked_at is null
            and t.expires_at > now()
            and u.id = t.user_id
      returning u.profile_id, u.role, p.username",
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await?;

    let Some((profile_id, role, username)) = row else {
        return Err(ApiError::Unauthorized(
            "refresh token is invalid or expired".to_string(),
        ));
    };

    issue_tokens(&state, profile_id, &username, &role).await
}

pub async fn logout(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<LogoutResponse>, ApiError> {
    let pool = state.db()?;
    sqlx::query(
        "update public.auth_refresh_tokens set revoked_at = now()
          where token_hash = $1 and revoked_at is null",
    )
    .bind(hash_refresh_token(&body.refresh_token))
    .execute(pool)
    .await?;
    Ok(Json(LogoutResponse {
        status: "signed_out",
    }))
}

fn normalize_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    let valid = !email.is_empty()
        && email.len() <= 254
        && email.contains('@')
        && !email.starts_with('@')
        && !email.ends_with('@')
        && email.matches('@').count() == 1
        && !email.contains(' ');
    if !valid {
        return Err(ApiError::BadRequest("invalid email address".to_string()));
    }
    Ok(email)
}

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt_bytes = {
        use rand::RngCore;
        let mut bytes = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        bytes
    };
    let salt = SaltString::encode_b64(&salt_bytes)?;
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    PasswordHash::new(stored_hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

const DUMMY_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$0000000000000000000000000000000000000000000";

fn hash_refresh_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn random_refresh_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

async fn issue_tokens(
    state: &AppState,
    profile_id: uuid::Uuid,
    username: &str,
    role: &str,
) -> Result<Json<TokenResponse>, ApiError> {
    let pool = state.db()?;
    let access_token = mint_token(
        &state.auth,
        &profile_id.to_string(),
        username,
        role,
        ACCESS_TTL_SECS,
    )
    .map_err(|e| ApiError::BadRequest(format!("could not sign token: {e}")))?;

    let refresh_token = random_refresh_token();
    sqlx::query(
        "insert into public.auth_refresh_tokens (user_id, token_hash, expires_at)
         select id, $2, now() + ($3 || ' days')::interval
           from public.auth_users where profile_id = $1",
    )
    .bind(profile_id)
    .bind(hash_refresh_token(&refresh_token))
    .bind(REFRESH_TTL_DAYS.to_string())
    .execute(pool)
    .await?;

    tracing::info!(username, "session issued");
    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
        expires_in: ACCESS_TTL_SECS,
        token_type: "bearer",
        username: username.to_string(),
        role: role.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_validation_rejects_obvious_garbage() {
        assert!(normalize_email("  Player@Example.COM ").is_ok());
        assert_eq!(
            normalize_email("Player@Example.COM").unwrap(),
            "player@example.com"
        );
        assert!(normalize_email("").is_err());
        assert!(normalize_email("no-at-sign").is_err());
        assert!(normalize_email("@leading").is_err());
        assert!(normalize_email("trailing@").is_err());
        assert!(normalize_email("two@@ats").is_err());
        assert!(normalize_email("has space@x.com").is_err());
    }

    #[test]
    fn password_hash_roundtrip() {
        let hash = hash_password("hunter22!!").unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password("hunter22!!", &hash));
        assert!(!verify_password("wrong-password", &hash));
        assert!(!verify_password("hunter22!!", "not-a-hash"));
    }

    #[test]
    fn refresh_tokens_are_hashed_deterministically() {
        let token = random_refresh_token();
        assert_eq!(token.len(), 64, "32 bytes hex-encoded");
        assert_eq!(hash_refresh_token(&token), hash_refresh_token(&token));
        assert_ne!(
            hash_refresh_token(&token),
            hash_refresh_token(&random_refresh_token())
        );
    }
}
