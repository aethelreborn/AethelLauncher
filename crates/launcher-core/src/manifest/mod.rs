//! Manifest module — Mojang versions, Fabric meta, Java runtime.

pub mod fabric;
pub mod java_runtime;
pub mod mojang;

pub use fabric::*;
pub use java_runtime::*;
pub use mojang::*;

/// Resolved version info after fetching + parsing.
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub version_json: VersionJson,
    /// Set when a Fabric loader was resolved for this version.
    pub fabric_loader: Option<FabricLoader>,
    pub java_component: Option<JavaComponent>,
}

/// Fetch available Minecraft versions from Mojang.
pub async fn fetch_available_versions(client: &reqwest::Client) -> anyhow::Result<Vec<String>> {
    let manifest = fetch_version_manifest(client).await?;
    Ok(manifest
        .versions
        .iter()
        .filter(|v| v.type_ == "release")
        .map(|v| v.id.clone())
        .collect())
}

/// Find a specific version's JSON by ID.
pub async fn fetch_version_by_id(
    client: &reqwest::Client,
    version_id: &str,
) -> anyhow::Result<VersionJson> {
    let manifest = fetch_version_manifest(client).await?;
    let version = manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| anyhow::anyhow!("Version not found: {}", version_id))?;

    fetch_version_json(client, &version.url).await
}
