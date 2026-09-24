//! User profile endpoint.

use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub username: String,
    pub email: Option<String>,
    pub wallet_balance: u64,
}

#[derive(Clone)]
pub struct AppState {
    pub users: Arc<Mutex<Vec<UserRecord>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub wallet_balance: u64,
}

pub async fn get_me(State(state): State<AppState>) -> Json<MeResponse> {
    // In production, extract from JWT claims
    Json(MeResponse {
        username: "anonymous".to_string(),
        email: None,
        wallet_balance: 0,
    })
}

pub async fn update_me(
    State(_state): State<AppState>,
    _Json(_req): Json<serde_json::Value>,
) -> Json<MeResponse> {
    Json(MeResponse {
        username: "anonymous".to_string(),
        email: None,
        wallet_balance: 0,
    })
}
