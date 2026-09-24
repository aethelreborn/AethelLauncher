//! Asset index and object management.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetIndex {
    pub objects: BTreeMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// Download asset objects from the index.
pub async fn download_assets(
    client: &reqwest::Client,
    index_url: &str,
    assets_dir: &Path,
) -> anyhow::Result<AssetIndex> {
    let resp = client.get(index_url).send().await?.error_for_status()?;
    let index: AssetIndex = resp.json().await?;

    for obj in index.objects.values() {
        let dest = assets_dir.join(&obj.hash[..2]).join(&obj.hash);
        if !dest.exists() {
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let asset_url = format!(
                "https://resources.download.minecraft.net/{}/{}",
                &obj.hash[..2],
                obj.hash
            );
            let asset_resp = client.get(&asset_url).send().await?.error_for_status()?;
            let content = asset_resp.bytes().await?;
            tokio::fs::write(&dest, content).await?;
        }
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_index_parsing() {
        let json = r#"{
            "objects": {
                "minecraft/textures/block/dirt.png": {
                    "hash": "aabbccdd",
                    "size": 1234
                }
            }
        }"#;
        let index: AssetIndex = serde_json::from_str(json).expect("parse asset index");
        assert_eq!(index.objects.len(), 1);
    }
}
