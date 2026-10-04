use axum::Json;
pub async fn get_shop() -> Json<Vec<()>> { Json(vec![]) }
pub async fn buy_item() -> Json<serde_json::Value> { Json(serde_json::json!({"success": true})) }
