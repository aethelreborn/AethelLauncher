use axum::Json;
pub async fn get_telemetry() -> Json<Vec<u8>> { Json(vec![]) }
