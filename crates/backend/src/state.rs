use sqlx::PgPool;
use std::time::Instant;

#[derive(Clone)]
pub struct AuthConfig {
    pub secret: String,
    pub ephemeral: bool,
}

pub const ACCESS_TTL_SECS: i64 = 15 * 60;
pub const REFRESH_TTL_DAYS: i64 = 30;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<PgPool>,
    pub started_at: Instant,
    pub auth: AuthConfig,
}

impl AppState {
    pub fn new(pool: Option<PgPool>, auth: AuthConfig) -> Self {
        Self {
            pool,
            started_at: Instant::now(),
            auth,
        }
    }

    pub fn uptime_s(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn db(&self) -> Result<&PgPool, ApiError> {
        self.pool.as_ref().ok_or(ApiError::NoDatabase)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("cosmetics database is not configured on this server")]
    NoDatabase,

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    Conflict(String),

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
            ApiError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
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
