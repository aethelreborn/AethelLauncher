use crate::middleware::auth::JwtClaims;
use crate::state::{ApiError, AppState};
use axum::extract::{Extension, State};
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub username: String,
    pub email: Option<String>,
    pub role: String,
    pub wallet_balance: u32,
}

pub async fn get_me(
    State(state): State<AppState>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<MeResponse>, ApiError> {
    let pool = state.db()?;
    let profile_id = claims
        .sub
        .parse::<uuid::Uuid>()
        .map_err(|_| ApiError::Unauthorized("token subject is not a profile id".to_string()))?;

    let (email, balance): (Option<String>, i32) = sqlx::query_as(
        "select u.email, coalesce(w.balance_cents, 0)
           from public.profiles p
           left join public.auth_users u on u.profile_id = p.id
           left join public.wallets w on w.player_id = p.id
          where p.id = $1",
    )
    .bind(profile_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("profile no longer exists".to_string()))?;

    Ok(Json(MeResponse {
        username: claims.username.clone(),
        email,
        role: claims.role.clone(),
        wallet_balance: balance.max(0) as u32,
    }))
}
