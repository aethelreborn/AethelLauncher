//! Update check endpoint.

use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct UpdateCheckRequest {
    pub current: String,
    pub channel: String,
    pub install_id: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateCheckResponse {
    pub update_available: bool,
    pub latest_version: String,
    pub download_url: Option<String>,
    pub changelog: String,
    pub mandatory: bool,
}

pub async fn update_check(
    Json(req): Json<UpdateCheckRequest>,
) -> Json<UpdateCheckResponse> {
    // In production, compare against Supabase-stored latest version
    let latest = env!("CARGO_PKG_VERSION").to_string();
    let update_available = req.current != latest;
    
    Json(UpdateCheckResponse {
        update_available,
        latest_version: latest,
        download_url: if update_available {
            Some(format!(
                "https://github.com/aethelreborn/AethelLauncher/releases/download/v{}/linux-x64.tar.gz",
                latest
            ))
        } else {
            None
        },
        changelog: "Initial release".to_string(),
        mandatory: false,
    })
}
