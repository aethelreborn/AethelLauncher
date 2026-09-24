//! Route modules.
//!
//! Only modules that are actually mounted in `main.rs` are declared here. The
//! remaining stub files in this directory (`me.rs`, `admin.rs`, `telemetry.rs`,
//! `manifest.rs`, `update.rs`, `news.rs`, `servers.rs`, `versions.rs`) are not
//! compiled yet — they land when their backing data does, so the running binary
//! never exposes an endpoint that returns a fake payload.

use crate::state::AppState;
use axum::routing::get;
use axum::Router;

#[allow(dead_code)]
pub mod cosmetics;
pub mod health;

/// Build only the unauthenticated route surface.
///
/// Identity-bearing cosmetics handlers are deliberately excluded until a
/// verified subject-to-profile binding and authorization tests exist.
pub(crate) fn public_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .route("/api/v1/health", get(health::health))
        .route("/api/v1/cosmetics", get(cosmetics::list_cosmetics))
}
