use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::{VersionInfo, VersionType};
use crate::manifest::mojang::fetch_version_manifest;
use crate::manifest::mojang::VersionManifest;

const CACHE_TTL: Duration = Duration::from_secs(60 * 60);
const CACHE_FILE: &str = "mojang_version_manifest.json";

#[derive(Clone, Debug)]
pub struct VersionCache {
    inner: Arc<Mutex<CacheInner>>,
    base_dir: PathBuf,
}

struct CacheInner {
    manifest: Option<VersionManifest>,
    fetched_at: Instant,
}

impl std::fmt::Debug for CacheInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheInner")
            .field(
                "manifest",
                &if self.manifest.is_some() {
                    "Some(...)"
                } else {
                    "None"
                },
            )
            .field("fetched_at", &self.fetched_at)
            .finish()
    }
}

impl VersionCache {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CacheInner {
                manifest: None,
                fetched_at: Instant::now()
                    .checked_sub(CACHE_TTL)
                    .unwrap_or_else(Instant::now),
            })),
            base_dir,
        }
    }

    pub async fn fetch(&self, client: &reqwest::Client) -> anyhow::Result<Vec<VersionInfo>> {
        let needs_fetch = {
            let inner = self.inner.lock();
            inner.manifest.is_none() || inner.fetched_at.elapsed() > CACHE_TTL
        };

        if !needs_fetch {
            let manifest = self.inner.lock().manifest.clone().unwrap();
            return Ok(to_version_infos(&manifest));
        }

        let manifest = match self.load_disk() {
            Ok(m) => m,
            Err(_) => {
                let fetched = fetch_version_manifest(client).await?;
                let _ = self.save_disk(&fetched);
                fetched
            }
        };

        let mut inner = self.inner.lock();
        inner.manifest = Some(manifest.clone());
        inner.fetched_at = Instant::now();
        Ok(to_version_infos(&manifest))
    }

    pub fn snapshot(&self) -> Option<Vec<VersionInfo>> {
        self.inner.lock().manifest.as_ref().map(to_version_infos)
    }

    fn load_disk(&self) -> anyhow::Result<VersionManifest> {
        let path = self.base_dir.join(CACHE_FILE);
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    fn save_disk(&self, manifest: &VersionManifest) -> anyhow::Result<()> {
        let path = self.base_dir.join(CACHE_FILE);
        let raw = serde_json::to_string_pretty(manifest)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}

fn to_version_infos(manifest: &VersionManifest) -> Vec<VersionInfo> {
    manifest
        .versions
        .iter()
        .map(|v| VersionInfo {
            id: v.id.clone(),
            type_: match v.type_.as_str() {
                "snapshot" => VersionType::Snapshot,
                _ => VersionType::Release,
            },
            url: v.url.clone(),
            time: v.time,
            release_time: v.release_time,
        })
        .collect()
}
