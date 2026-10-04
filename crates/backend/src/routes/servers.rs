use axum::Json;
pub async fn get_servers() -> Json<Vec<u8>> { Json(vec![]) }
