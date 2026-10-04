use crate::state::{AppState, AuthConfig};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub username: String,
    pub role: String,
    pub iat: usize,
    pub exp: usize,
}

impl JwtClaims {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

pub fn mint_token(
    auth: &AuthConfig,
    sub: &str,
    username: &str,
    role: &str,
    ttl_secs: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or(0);
    let claims = JwtClaims {
        sub: sub.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        iat: now,
        exp: now + ttl_secs.max(60) as usize,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(auth.secret.as_bytes()),
    )
}

pub fn validate_token(
    auth: &AuthConfig,
    token: &str,
) -> Result<JwtClaims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_required_spec_claims(&["exp", "sub"]);
    validation.leeway = 60;
    decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(auth.secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
}

fn bearer_token(req: &Request) -> Option<&str> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

pub async fn require_jwt(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, crate::state::ApiError> {
    let token = bearer_token(&req)
        .ok_or_else(|| crate::state::ApiError::Unauthorized("missing bearer token".to_string()))?;
    let claims = validate_token(&state.auth, token).map_err(|_| {
        crate::state::ApiError::Unauthorized("invalid or expired token".to_string())
    })?;
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

pub async fn require_admin(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, crate::state::ApiError> {
    let token = bearer_token(&req)
        .ok_or_else(|| crate::state::ApiError::Unauthorized("missing bearer token".to_string()))?;
    let claims = validate_token(&state.auth, token).map_err(|_| {
        crate::state::ApiError::Unauthorized("invalid or expired token".to_string())
    })?;
    if !claims.is_admin() {
        return Err(crate::state::ApiError::Forbidden(
            "admin role required".to_string(),
        ));
    }
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AuthConfig;

    fn config(secret: &str) -> AuthConfig {
        AuthConfig {
            secret: secret.to_string(),
            ephemeral: false,
        }
    }

    #[test]
    fn mint_and_validate_roundtrip() {
        let auth = config("test-secret");
        let token = mint_token(&auth, "uuid-1", "Steve", "user", 900).unwrap();
        let claims = validate_token(&auth, &token).unwrap();
        assert_eq!(claims.sub, "uuid-1");
        assert_eq!(claims.username, "Steve");
        assert_eq!(claims.role, "user");
        assert!(!claims.is_admin());
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let token = mint_token(&config("secret-a"), "u", "Steve", "user", 900).unwrap();
        assert!(validate_token(&config("secret-b"), &token).is_err());
    }

    #[test]
    fn expired_token_is_rejected() {
        let auth = config("test-secret");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        let claims = JwtClaims {
            sub: "u".into(),
            username: "Steve".into(),
            role: "user".into(),
            iat: now - 3600,
            exp: now - 3600,
        };
        let token = jsonwebtoken::encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(auth.secret.as_bytes()),
        )
        .unwrap();
        assert!(validate_token(&auth, &token).is_err());
    }

    #[test]
    fn admin_role_is_detected_from_claims() {
        let auth = config("test-secret");
        let admin = mint_token(&auth, "uuid-1", "Notch", "admin", 900).unwrap();
        assert!(validate_token(&auth, &admin).unwrap().is_admin());
        let user = mint_token(&auth, "uuid-1", "Steve", "user", 900).unwrap();
        assert!(!validate_token(&auth, &user).unwrap().is_admin());
    }

    #[test]
    fn malformed_authorization_headers_are_not_tokens() {
        let req = Request::builder()
            .header(axum::http::header::AUTHORIZATION, "Basic dXNlcjpwYXNz")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(bearer_token(&req), None);

        let req = Request::builder()
            .header(axum::http::header::AUTHORIZATION, "Bearer    ")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(bearer_token(&req), None);
    }
}
