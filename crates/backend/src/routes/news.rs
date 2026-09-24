use axum::Json;
pub async fn get_news() -> Json<Vec<u8>> { Json(vec![]) }
