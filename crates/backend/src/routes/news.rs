use crate::state::{ApiError, AppState};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct NewsItem {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub url: Option<String>,
    pub date: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateNewsRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub url: Option<String>,
}

pub async fn list_news(State(state): State<AppState>) -> Result<Json<Vec<NewsItem>>, ApiError> {
    let Some(pool) = &state.pool else {
        tracing::warn!("news requested without a database — serving []");
        return Ok(Json(Vec::new()));
    };

    let rows = sqlx::query_as::<_, (Uuid, String, String, Option<String>, chrono::NaiveDateTime)>(
        "select id, title, body, url, published_at at time zone 'UTC'
           from public.news
          order by published_at desc
          limit 50",
    )
    .fetch_all(pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, title, body, url, published)| NewsItem {
                id,
                title,
                body,
                url,
                date: published.format("%Y-%m-%d").to_string(),
            })
            .collect(),
    ))
}

pub async fn create_news(
    State(state): State<AppState>,
    Json(body): Json<CreateNewsRequest>,
) -> Result<(StatusCode, Json<NewsItem>), ApiError> {
    let pool = state.db()?;
    let title = body.title.trim().to_string();
    if title.is_empty() || title.len() > 200 {
        return Err(ApiError::BadRequest(
            "title must be 1-200 characters".to_string(),
        ));
    }
    if body.body.len() > 10_000 {
        return Err(ApiError::BadRequest("body is too long".to_string()));
    }

    let row: (Uuid, String, String, Option<String>, chrono::NaiveDateTime) = sqlx::query_as(
        "insert into public.news (title, body, url)
         values ($1, $2, $3)
         returning id, title, body, url, published_at at time zone 'UTC'",
    )
    .bind(&title)
    .bind(body.body.trim())
    .bind(body.url.as_deref().filter(|u| !u.trim().is_empty()))
    .fetch_one(pool)
    .await?;

    tracing::info!(title = %row.1, "news published");
    Ok((
        StatusCode::CREATED,
        Json(NewsItem {
            id: row.0,
            title: row.1,
            body: row.2,
            url: row.3,
            date: row.4.format("%Y-%m-%d").to_string(),
        }),
    ))
}

pub async fn delete_news(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let pool = state.db()?;
    let result = sqlx::query("delete from public.news where id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("news item {id} not found")));
    }
    tracing::info!(id = %id, "news deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn news_item_serialises_for_the_launcher() {
        let item = NewsItem {
            id: Uuid::nil(),
            title: "Launch".into(),
            body: "We shipped.".into(),
            url: Some("https://example.com".into()),
            date: "2026-09-30".into(),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["title"], "Launch");
        assert_eq!(json["date"], "2026-09-30");
        assert_eq!(json["url"], "https://example.com");
    }

    #[test]
    fn create_request_defaults_body_and_url() {
        let req: CreateNewsRequest = serde_json::from_str(r#"{"title":"Hello"}"#).unwrap();
        assert_eq!(req.title, "Hello");
        assert_eq!(req.body, "");
        assert_eq!(req.url, None);
    }
}
