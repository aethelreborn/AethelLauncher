//! Health check endpoint.

use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub uptime_s: u64,
    /// `ok` | `unconfigured` | `error`
    pub database: &'static str,
}

/// `GET /api/v1/health`
///
/// Always returns 200 so a load balancer keeps the instance up; the `database`
/// field carries the real state (the launcher shows an "offline" badge rather
/// than failing).
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let database = match &state.pool {
        None => "unconfigured",
        Some(pool) => match crate::services::supabase::health(pool).await {
            Ok(()) => "ok",
            Err(e) => {
                tracing::warn!("database health check failed: {e}");
                "error"
            }
        },
    };

    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_s: state.uptime_s(),
        database,
    })
}
