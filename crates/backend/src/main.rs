//! Aethel backend — cosmetics platform API.
//!
//! Boots with or without a database: `DATABASE_URL` enables the public
//! cosmetics catalogue, and without it the server still serves health so the
//! launcher can run offline-first.

mod routes;
mod services;
mod state;

use state::AppState;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn,sqlx=warn".into()),
        )
        .init();

    let pool = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => {
            let pool = services::supabase::create_pool(&url).await?;
            tracing::info!("connected to Postgres");
            match services::supabase::migrate(&pool).await {
                Ok(()) => tracing::info!("cosmetics schema ready"),
                Err(e) => {
                    tracing::error!("migration failed: {e:#}");
                    // Keep serving: reads may still work and it is better than
                    // crashing the deployment loop.
                }
            }
            Some(pool)
        }
        _ => {
            tracing::warn!(
                "DATABASE_URL is not set — the cosmetics catalogue will return 503. \
                 Set it to your Supabase connection string to enable it."
            );
            None
        }
    };

    let state = AppState::new(pool);

    let app = routes::public_router()
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("aethel-backend listening on {addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
