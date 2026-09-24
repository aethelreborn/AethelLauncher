//! JWT authentication middleware.

use axum::{
    extract::{Request, State},
    http::{HeaderName, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub role: Option<String>,
    pub exp: usize,
}

pub struct AuthState {
    pub jwks_url: String,
    pub secret: String,
}

/// Extract JWT from Authorization header.
pub async fn require_jwt(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(HeaderName::from_static("authorization"))
        .and_then(|h| h.to_str().ok())
        .filter(|h| h.starts_with("Bearer "))
        .map(|h| &h[7..])?;

    let token = decode::<JwtClaims>(
        auth_header,
        &DecodingKey::from_secret(state.secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(token.claims);
    Ok(next.run(req).await)
}

/// Require admin role.
pub async fn require_admin(
    State(_state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let claims = req
        .extensions()
        .get::<JwtClaims>()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if claims.role.as_deref() != Some("admin") {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(req).await)
}
