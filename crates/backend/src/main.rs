mod middleware;
mod routes;
mod services;
mod state;
mod validate;

use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post};
use axum::Router;
use state::{AppState, AuthConfig};
use tower_http::cors::CorsLayer;
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
                Ok(()) => tracing::info!("schema ready"),
                Err(e) => {
                    tracing::error!("migration failed: {e:#}");
                }
            }
            Some(pool)
        }
        _ => {
            tracing::warn!(
                "DATABASE_URL is not set — cosmetics and auth endpoints will be limited. \
                 Set it to your Postgres connection string to enable them."
            );
            None
        }
    };

    let state = AppState::new(pool, auth_config_from_env()?);
    let app = app(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("aethel-backend listening on {addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

fn auth_config_from_env() -> anyhow::Result<AuthConfig> {
    match std::env::var("JWT_SECRET") {
        Ok(secret) if !secret.trim().is_empty() => Ok(AuthConfig {
            secret,
            ephemeral: false,
        }),
        _ => {
            use rand::RngCore;
            let mut bytes = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            tracing::warn!(
                "JWT_SECRET is not set — using an ephemeral secret; \
                 issued tokens become invalid when the server restarts"
            );
            Ok(AuthConfig {
                secret: hex::encode(bytes),
                ephemeral: true,
            })
        }
    }
}

fn app(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(routes::health::health))
        .route("/api/v1/health", get(routes::health::health))
        .route("/api/v1/cosmetics", get(routes::cosmetics::list_cosmetics))
        .route("/api/v1/news", get(routes::news::list_news))
        .route("/api/v1/auth/register", post(routes::auth::register))
        .route("/api/v1/auth/login", post(routes::auth::login))
        .route("/api/v1/auth/refresh", post(routes::auth::refresh))
        .route("/api/v1/auth/logout", post(routes::auth::logout));

    let authed = Router::new()
        .route("/api/v1/me", get(routes::me::get_me))
        .route(
            "/api/v1/cosmetics/mine",
            get(routes::cosmetics::get_inventory),
        )
        .route(
            "/api/v1/cosmetics/:slug/purchase",
            post(routes::cosmetics::purchase),
        )
        .route(
            "/api/v1/cosmetics/:slug/equip",
            post(routes::cosmetics::equip),
        )
        .route(
            "/api/v1/cosmetics/:slug/unequip",
            post(routes::cosmetics::unequip),
        )
        .route_layer(from_fn_with_state(
            state.clone(),
            middleware::auth::require_jwt,
        ));

    let admin = Router::new()
        .route("/api/v1/admin/news", post(routes::news::create_news))
        .route("/api/v1/admin/news/:id", delete(routes::news::delete_news))
        .route_layer(from_fn_with_state(
            state.clone(),
            middleware::auth::require_admin,
        ));

    public
        .merge(authed)
        .merge(admin)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_state() -> AppState {
        AppState::new(
            None,
            AuthConfig {
                secret: "test-secret".into(),
                ephemeral: false,
            },
        )
    }

    fn mint(sub: &str, username: &str, role: &str) -> String {
        middleware::auth::mint_token(&test_state().auth, sub, username, role, 900).unwrap()
    }

    async fn body_json(res: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }

    #[tokio::test]
    async fn health_is_public() {
        let res = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let json = body_json(res).await;
        assert_eq!(json["status"], "ok");
        assert_eq!(json["database"], "unconfigured");
    }

    #[tokio::test]
    async fn news_serves_empty_feed_without_database() {
        let res = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/news")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await, serde_json::json!([]));
    }

    #[tokio::test]
    async fn mine_requires_a_token() {
        let res = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/cosmetics/mine")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let res = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/cosmetics/mine")
                    .header("authorization", "Bearer not-a-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn mine_passes_auth_and_fails_on_missing_database() {
        let res = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/cosmetics/mine")
                    .header(
                        "authorization",
                        format!("Bearer {}", mint("p1", "Steve", "user")),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn admin_route_rejects_users_and_anonymous() {
        let anonymous = app(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/news")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

        let user = app(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/news")
                    .header(
                        "authorization",
                        format!("Bearer {}", mint("p1", "Steve", "user")),
                    )
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(user.status(), StatusCode::FORBIDDEN);

        let admin = app(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/news")
                    .header(
                        "authorization",
                        format!("Bearer {}", mint("p1", "Notch", "admin")),
                    )
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn auth_register_without_database_is_a_clear_503() {
        let res = app(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/register")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"email":"a@b.co","username":"Steve","password":"password123"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body_json(res).await["error"], "no_database");
    }

    #[tokio::test]
    async fn legacy_username_inventory_route_is_gone() {
        let res = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/cosmetics/Steve/inventory")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
