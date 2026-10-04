use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleManifest {
    pub manifest_version: u8,
    pub mc_version: String,
    pub bundle_name: String,
    pub bundle_version: String,
    pub renderer_mode: RendererMode,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub archive: Option<BundleArchive>,
    #[serde(default)]
    pub files: Vec<BundleFile>,
    #[serde(default)]
    pub options_gen: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RendererMode {
    Vulkan,
    Opengl,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BundleArchive {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub rewrite_root: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BundleFile {
    pub rel: String,
    pub sha256: String,
    pub size: u64,
    pub class: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ManagedState {
    pub last_verified: Option<String>,
    pub files: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_manifest_parsing() {
        let json = r#"{
            "manifestVersion": 1,
            "mcVersion": "1.21.11",
            "bundleName": "aethel-perf-vulkan",
            "bundleVersion": "3.2.0",
            "rendererMode": "vulkan",
            "signature": null,
            "archive": null,
            "files": [
                {"rel": "mods/sodium-0.9.0.jar", "sha256": "abc123", "size": 100000, "class": "renderer"},
                {"rel": "mods/lithium-0.16.0.jar", "sha256": "def456", "size": 50000, "class": "perf"}
            ],
            "optionsGen": {"gfxApi": "VULKAN", "renderDistance": "12"}
        }"#;
        let manifest: BundleManifest = serde_json::from_str(json).expect("parse bundle manifest");
        assert_eq!(manifest.mc_version, "1.21.11");
        assert_eq!(manifest.renderer_mode, RendererMode::Vulkan);
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(
            manifest.options_gen.get("gfxApi"),
            Some(&"VULKAN".to_string())
        );
    }
}
