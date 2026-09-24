//! Shared application state.

use sqlx::PgPool;
use std::time::Instant;

/// Cheap to clone (both fields are Arc-like) — axum clones it per request.
#[derive(Clone)]
pub struct AppState {
    /// `None` when `DATABASE_URL` is unset. The server still boots so `/health`
    /// and the public catalogue work, while the catalogue returns a clear 503
    /// instead of panicking when the database is unavailable.
    pub pool: Option<PgPool>,
    pub started_at: Instant,
}

impl AppState {
    pub fn new(pool: Option<PgPool>) -> Self {
        Self {
            pool,
            started_at: Instant::now(),
        }
    }

    pub fn uptime_s(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Borrow the pool or fail with the standard "database not configured"
    /// error, so every DB-backed handler reports the same thing.
    pub fn db(&self) -> Result<&PgPool, ApiError> {
        self.pool.as_ref().ok_or(ApiError::NoDatabase)
    }
}

/// API error type with a stable JSON body.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("cosmetics database is not configured on this server")]
    NoDatabase,

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("insufficient funds")]
    InsufficientFunds { balance_cents: i32 },

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;
        use axum::Json;

        let (status, code) = match &self {
            ApiError::NoDatabase => (StatusCode::SERVICE_UNAVAILABLE, "no_database"),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::InsufficientFunds { .. } => {
                (StatusCode::PAYMENT_REQUIRED, "insufficient_funds")
            }
            ApiError::Database(e) => {
                tracing::error!("database error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "database_error")
            }
        };

        let mut body = serde_json::json!({
            "error": code,
            "message": self.to_string(),
        });
        if let ApiError::InsufficientFunds { balance_cents } = self {
            body["balance_cents"] = serde_json::json!(balance_cents);
        }

        (status, Json(body)).into_response()
    }
}
