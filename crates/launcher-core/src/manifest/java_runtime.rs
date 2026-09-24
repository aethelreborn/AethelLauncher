//! Mojang Java runtime manifest parsing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntimeManifest {
    pub components: BTreeMap<String, JavaComponent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaComponent {
    pub major: u32,
    #[serde(rename = "architecture")]
    pub arch: String,
    pub operating_system: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Fetch the Java runtime manifest.
pub async fn fetch_java_manifest(client: &reqwest::Client) -> anyhow::Result<JavaRuntimeManifest> {
    let resp = client
        .get("https://launchermeta.mojang.com/runtime/all.json")
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json().await?)
}

/// Pick the best matching JRE component.
pub fn select_jre<'a>(
    manifest: &'a JavaRuntimeManifest,
    required_major: u32,
    current_os: &str,
    current_arch: &str,
) -> Option<&'a JavaComponent> {
    manifest.components.values().find(|c| {
        c.major == required_major && c.operating_system == current_os && c.arch == current_arch
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_java_manifest_structure() {
        let json = r#"{
            "components": {
                "legacy": {
                    "major": 8,
                    "architecture": "x64",
                    "operatingSystem": "linux",
                    "url": "https://example.com/jre8.tar.gz",
                    "sha256": "abc123",
                    "size": 100000000
                },
                "gamma": {
                    "major": 17,
                    "architecture": "x64",
                    "operatingSystem": "linux",
                    "url": "https://example.com/jre17.tar.gz",
                    "sha256": "def456",
                    "size": 200000000
                }
            }
        }"#;
        let manifest: JavaRuntimeManifest =
            serde_json::from_str(json).expect("parse java manifest");
        assert_eq!(manifest.components.len(), 2);
        let gamma = select_jre(&manifest, 17, "linux", "x64");
        assert!(gamma.is_some());
        assert_eq!(gamma.unwrap().major, 17);
    }
}
