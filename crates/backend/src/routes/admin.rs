//! Admin endpoints (JWT + role=admin required).

use axum::{response::StatusCode, Json};

pub async fn create_news() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED // Requires admin auth layer
}

pub async fn refresh_bundles() -> Json<serde_json::Value> {
    // TODO: trigger manifest refresh
    Json(serde_json::json!({"refreshed": []}))
}
