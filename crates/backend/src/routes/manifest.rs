
use axum::{extract::Path, Json};
use moka::future::Cache;
use std::sync::Arc;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct BundleManifest {
    pub manifest_version: u8,
    pub mc_version: String,
    pub bundle_name: String,
    pub bundle_version: String,
    pub renderer_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<BundleArchive>,
    pub files: Vec<BundleFile>,
    pub options_gen: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct BundleArchive {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub rewrite_root: String,
}

#[derive(Debug, Serialize)]
pub struct BundleFile {
    pub rel: String,
    pub sha256: String,
    pub size: u64,
    pub class: String,
}

type ManifestCache = Arc<Cache<(String, String), BundleManifest>>;

pub async fn get_manifest(
    Path((mc_version, bundle)): Path<(String, String)>,
    cache: axum::extract::State<ManifestCache>,
) -> Json<BundleManifest> {
    let key = (mc_version.clone(), bundle.clone());

    if let Some(manifest) = cache.get(&key) {
        return Json(manifest);
    }

    let manifest = BundleManifest {
        manifest_version: 1,
        mc_version,
        bundle_name: bundle,
        bundle_version: "1.0.0".to_string(),
        renderer_mode: "vulkan".to_string(),
        signature: None,
        archive: None,
        files: vec![
            BundleFile {
                rel: "mods/sodium-fabric-0.9.0+mc1.21.11.jar".to_string(),
                sha256: "abc123def456".to_string(),
                size: 1000000,
                class: "renderer".to_string(),
            },
            BundleFile {
                rel: "mods/lithium-fabric-0.16.0.jar".to_string(),
                sha256: "def456abc123".to_string(),
                size: 500000,
                class: "perf".to_string(),
            },
        ],
        options_gen: {
            let mut m = BTreeMap::new();
            m.insert("gfxApi".to_string(), "VULKAN".to_string());
            m.insert("renderDistance".to_string(), "12".to_string());
            m
        },
    };

    cache.insert(key, manifest.clone()).await;
    Json(manifest)
}
