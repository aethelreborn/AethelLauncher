use crate::middleware::auth::JwtClaims;
use crate::state::{ApiError, AppState};
use axum::extract::{Extension, Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Cosmetic {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub kind: String,
    pub rarity: String,
    pub asset_url: String,
    pub tint_hex: String,
    pub price_cents: i32,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryResponse {
    pub username: String,
    pub balance_cents: i32,
    pub owned: Vec<String>,
    pub equipped: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct PurchaseRequest {
    pub request_id: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct PlayerRequest {}

#[derive(Debug, Serialize)]
pub struct PurchaseResponse {
    pub status: String,
    pub balance_cents: i32,
    pub cosmetic_id: Option<Uuid>,
    pub cosmetic_slug: String,
    pub inventory: InventoryResponse,
}

pub async fn list_cosmetics(
    State(state): State<AppState>,
) -> Result<Json<Vec<Cosmetic>>, ApiError> {
    let pool = state.db()?;
    let rows = sqlx::query_as::<_, Cosmetic>(
        "select id, slug, name, kind, rarity, asset_url, tint_hex, price_cents, sort_order
           from public.cosmetics
          where is_public
            and (available_until is null or available_until > now())
          order by sort_order asc, name asc",
    )
    .fetch_all(pool)
    .await?;

    Ok(Json(rows))
}

pub async fn get_inventory(
    State(state): State<AppState>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<Json<InventoryResponse>, ApiError> {
    let pool = state.db()?;
    Ok(Json(load_inventory(pool, &claims.username).await?))
}

pub async fn purchase(
    State(state): State<AppState>,
    Extension(claims): Extension<JwtClaims>,
    Path(slug): Path<String>,
    Json(body): Json<PurchaseRequest>,
) -> Result<Json<PurchaseResponse>, ApiError> {
    let pool = state.db()?;
    if body.request_id.trim().is_empty() {
        return Err(ApiError::BadRequest("request_id is required".to_string()));
    }

    let cosmetic_id = cosmetic_id_by_slug(pool, &slug).await?;

    let row: (String, Option<i32>, Option<Uuid>) = sqlx::query_as(
        "select status, balance_cents, cosmetic_id
           from public.purchase_cosmetic($1, $2, $3)",
    )
    .bind(&claims.username)
    .bind(cosmetic_id)
    .bind(&body.request_id)
    .fetch_one(pool)
    .await?;

    let (status, balance, returned_id) = row;
    let balance_cents = balance.unwrap_or(0);

    if status == "insufficient_funds" {
        return Err(ApiError::InsufficientFunds { balance_cents });
    }
    if status == "not_found" {
        return Err(ApiError::NotFound(format!("cosmetic `{slug}` not found")));
    }

    let inventory = load_inventory(pool, &claims.username).await?;
    Ok(Json(PurchaseResponse {
        status,
        balance_cents,
        cosmetic_id: returned_id,
        cosmetic_slug: slug,
        inventory,
    }))
}

pub async fn equip(
    State(state): State<AppState>,
    Extension(claims): Extension<JwtClaims>,
    Path(slug): Path<String>,
    Json(_body): Json<PlayerRequest>,
) -> Result<Json<InventoryResponse>, ApiError> {
    let pool = state.db()?;
    let (cosmetic_id, kind, price) = cosmetic_kind_and_price(pool, &slug).await?;
    let player_id = ensure_profile(pool, &claims.username).await?;

    let owned: Option<Uuid> = sqlx::query_scalar(
        "select cosmetic_id from public.inventory where player_id = $1 and cosmetic_id = $2",
    )
    .bind(player_id)
    .bind(cosmetic_id)
    .fetch_optional(pool)
    .await?;

    if owned.is_none() {
        if price != 0 {
            return Err(ApiError::BadRequest(format!("you do not own `{slug}`")));
        }
        sqlx::query(
            "insert into public.inventory (player_id, cosmetic_id)
             values ($1, $2) on conflict do nothing",
        )
        .bind(player_id)
        .bind(cosmetic_id)
        .execute(pool)
        .await?;
    }

    sqlx::query(
        "insert into public.equipped (player_id, kind, cosmetic_id)
         values ($1, $2, $3)
         on conflict (player_id, kind)
         do update set cosmetic_id = excluded.cosmetic_id, equipped_at = now()",
    )
    .bind(player_id)
    .bind(&kind)
    .bind(cosmetic_id)
    .execute(pool)
    .await?;

    Ok(Json(load_inventory(pool, &claims.username).await?))
}

pub async fn unequip(
    State(state): State<AppState>,
    Extension(claims): Extension<JwtClaims>,
    Path(slug): Path<String>,
    Json(_body): Json<PlayerRequest>,
) -> Result<Json<InventoryResponse>, ApiError> {
    let pool = state.db()?;
    let (_, kind, _) = cosmetic_kind_and_price(pool, &slug).await?;
    let player_id = ensure_profile(pool, &claims.username).await?;

    sqlx::query("delete from public.equipped where player_id = $1 and kind = $2")
        .bind(player_id)
        .bind(&kind)
        .execute(pool)
        .await?;

    Ok(Json(load_inventory(pool, &claims.username).await?))
}

async fn ensure_profile(pool: &PgPool, username: &str) -> Result<Uuid, ApiError> {
    let (id,): (Uuid,) = sqlx::query_as(
        "insert into public.profiles (username) values ($1)
         on conflict (username_ci) do update set updated_at = now()
         returning id",
    )
    .bind(username.trim())
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "insert into public.wallets (player_id) values ($1) on conflict (player_id) do nothing",
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(id)
}

async fn cosmetic_id_by_slug(pool: &PgPool, slug: &str) -> Result<Uuid, ApiError> {
    sqlx::query_scalar::<_, Uuid>("select id from public.cosmetics where slug = $1")
        .bind(slug)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("cosmetic `{slug}` not found")))
}

async fn cosmetic_kind_and_price(
    pool: &PgPool,
    slug: &str,
) -> Result<(Uuid, String, i32), ApiError> {
    sqlx::query_as::<_, (Uuid, String, i32)>(
        "select id, kind, price_cents from public.cosmetics where slug = $1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("cosmetic `{slug}` not found")))
}

async fn load_inventory(pool: &PgPool, username: &str) -> Result<InventoryResponse, ApiError> {
    let player_id: Option<Uuid> =
        sqlx::query_scalar("select id from public.profiles where username_ci = lower($1)")
            .bind(username.trim())
            .fetch_optional(pool)
            .await?;

    let Some(player_id) = player_id else {
        return Ok(InventoryResponse {
            username: username.trim().to_string(),
            balance_cents: 0,
            owned: Vec::new(),
            equipped: BTreeMap::new(),
        });
    };

    let balance_cents: Option<i32> =
        sqlx::query_scalar("select balance_cents from public.wallets where player_id = $1")
            .bind(player_id)
            .fetch_optional(pool)
            .await?;

    let owned: Vec<String> = sqlx::query_scalar(
        "select c.slug
           from public.inventory i
           join public.cosmetics c on c.id = i.cosmetic_id
          where i.player_id = $1
          order by c.sort_order",
    )
    .bind(player_id)
    .fetch_all(pool)
    .await?;

    let equipped_rows: Vec<(String, String)> = sqlx::query_as(
        "select e.kind, c.slug
           from public.equipped e
           join public.cosmetics c on c.id = e.cosmetic_id
          where e.player_id = $1",
    )
    .bind(player_id)
    .fetch_all(pool)
    .await?;

    Ok(InventoryResponse {
        username: username.trim().to_string(),
        balance_cents: balance_cents.unwrap_or(0),
        owned,
        equipped: equipped_rows.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::validate_username;

    #[test]
    fn username_validation_matches_minecraft_rules() {
        assert!(validate_username("Steve").is_ok());
        assert!(validate_username("aethel_01").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("has space").is_err());
        assert!(validate_username("emoji🎉").is_err());
        assert!(validate_username("seventeen_chars_!").is_err());
    }

    #[test]
    fn inventory_serialises_equipped_by_kind_with_slugs() {
        let response = InventoryResponse {
            username: "Steve".to_string(),
            balance_cents: 500,
            owned: vec!["cape-aethel".to_string()],
            equipped: BTreeMap::from([("cape".to_string(), "cape-aethel".to_string())]),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["equipped"]["cape"], "cape-aethel");
        assert_eq!(json["owned"][0], "cape-aethel");
        assert_eq!(json["balance_cents"], 500);
    }

    #[test]
    fn legacy_bodies_with_username_still_deserialize() {
        let purchase: PurchaseRequest =
            serde_json::from_str(r#"{"username":"Steve","request_id":"abc"}"#).unwrap();
        assert_eq!(purchase.request_id, "abc");

        let equip: PlayerRequest = serde_json::from_str(r#"{"username":"Steve"}"#).unwrap();
        let _ = equip;
        assert!(serde_json::from_str::<PlayerRequest>("{}").is_ok());
    }
}
