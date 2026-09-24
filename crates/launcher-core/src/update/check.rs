//! Update checking — compares current version against latest published.
//!
//! See [15 · Updating & Distribution](../../../opencode-docs/15-updating-distribution.md).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCheckRequest {
    #[serde(rename = "current")]
    pub current_version: String,
    pub channel: String, // "stable" | "beta" | "nightly"
    pub install_id: String,
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCheckResponse {
    pub update_available: bool,
    pub latest_version: String,
    pub download_url: String,
    pub changelog: String,
    pub mandatory: bool,
}

/// Check for updates against the backend API.
pub async fn check_update(
    client: &reqwest::Client,
    backend_url: &str,
    current_version: &str,
    channel: &str,
    install_id: &str,
) -> anyhow::Result<UpdateCheckResponse> {
    let resp = client
        .post(format!("{}/api/v1/launcher/update-check", backend_url))
        .json(&serde_json::json!({
            "current": current_version,
            "channel": channel,
            "install_id": install_id
        }))
        .send()
        .await?
        .error_for_status()?;

    Ok(resp.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_check_request_serialization() {
        let req = UpdateCheckRequest {
            current_version: "1.0.0".to_string(),
            channel: "stable".to_string(),
            install_id: "test-install".to_string(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("\"current\":\"1.0.0\""));
        assert!(json.contains("\"channel\":\"stable\""));
    }
}
