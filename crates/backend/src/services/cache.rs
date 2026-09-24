//! In-memory caching with moka.

use moka::future::Cache as MokaCache;
use std::sync::Arc;

pub type ManifestCache = Arc<MokaCache<(String, String), serde_json::Value>>;

pub fn new_manifest_cache() -> ManifestCache {
    Arc::new(MokaCache::builder()
        .time_to_live(std::time::Duration::from_secs(300)) // 5 min
        .build())
}
