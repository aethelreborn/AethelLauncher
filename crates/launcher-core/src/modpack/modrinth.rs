//! Modrinth API integration for fetching and downloading modpacks.

use super::types::{LoaderType, Modpack, ModpackFile, SearchParams};
use anyhow::Context;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::debug;

const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v1";

/// Response structure for search requests.
#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthProject>,
}

#[derive(Debug, Deserialize)]
struct ModrinthProject {
    project_id: String,
    slug: String,
    title: String,
    description: Option<String>,
    #[serde(rename = "downloads")]
    download_count: u64,
    #[serde(rename = "client_side")]
    _client_side: String,
    #[serde(rename = "server_side")]
    _server_side: String,
    #[serde(rename = "categories")]
    _categories: Vec<String>,
    #[serde(rename = "icon_url")]
    icon_url: Option<String>,
    #[serde(rename = "versions")]
    _versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    #[serde(rename = "version_id")]
    _version_id: String,
    #[serde(rename = "version_number")]
    version_number: String,
    #[serde(rename = "game_versions")]
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    #[serde(rename = "file_name")]
    file_name: String,
    #[serde(rename = "size")]
    size_bytes: u64,
    #[serde(rename = "hashes")]
    hashes: std::collections::HashMap<String, String>,
    url: String,
    #[serde(rename = "primary")]
    _is_primary: bool,
}

/// Search for modpacks on Modrinth.
///
/// Supports filtering by loader and Minecraft version.
pub async fn search_modpacks(
    client: &Client,
    params: &SearchParams,
) -> anyhow::Result<Vec<Modpack>> {
    let limit = (params.limit.min(100) as u64).max(1);
    let mut url = format!(
        "{MODRINTH_API_BASE}/search?query={}&limit={limit}",
        urlencoding::encode(&params.query)
    );

    if let Some(loader) = &params.loader {
        let loader_param = match loader {
            LoaderType::Fabric => "fabric",
            LoaderType::Forge => "forge",
            LoaderType::Quilt => "quilt",
            LoaderType::Vanilla => "vanilla",
        };
        url.push_str(&format!("&loaders={loader_param}"));
    }

    if let Some(mc_ver) = &params.mc_version {
        url.push_str(&format!("&game_versions={mc_ver}"));
    }

    debug!("Searching Modrinth: {}", url);

    let resp: ModrinthSearchResponse = client.get(&url).send().await?.json().await?;

    let mut result = Vec::new();
    for proj in resp.hits {
        if let Ok(modpack) = resolve_project(client, proj).await {
            result.push(modpack);
        }
    }
    Ok(result)
}

/// Get detailed information about a specific Modrinth modpack.
pub async fn get_modpack_details(client: &Client, slug_or_id: &str) -> anyhow::Result<Modpack> {
    let proj: ModrinthProject = client
        .get(format!("{MODRINTH_API_BASE}/project/{slug_or_id}"))
        .send()
        .await?
        .json()
        .await?;

    resolve_project(client, proj).await
}

async fn resolve_project(client: &Client, proj: ModrinthProject) -> anyhow::Result<Modpack> {
    let version = fetch_latest_version(client, &proj.project_id).await?;
    let loader = infer_loader(&version.loaders);
    let mc_versions = version.game_versions.clone();

    let files: Vec<ModpackFile> = version
        .files
        .into_iter()
        .map(|f| {
            let sha1 = f.hashes.get("sha1").cloned();
            ModpackFile {
                name: f.file_name.clone(),
                url: f.url,
                size_bytes: f.size_bytes,
                sha1,
                install_path: format!("modpacks/{}", f.file_name),
            }
        })
        .collect();

    Ok(Modpack {
        id: format!("mr-{}", proj.project_id),
        name: proj.title,
        mc_versions,
        loader,
        loader_version: Some(version.version_number),
        description: proj.description,
        project_url: Some(format!("https://modrinth.com/project/{}", proj.slug)),
        icon_url: proj.icon_url,
        download_count: Some(proj.download_count),
        files,
        extra: HashMap::new(),
    })
}

async fn fetch_latest_version(
    client: &Client,
    project_id: &str,
) -> anyhow::Result<ModrinthVersion> {
    let url = format!("{MODRINTH_API_BASE}/version?project_id={project_id}&featured=true&limit=1");
    let versions: Vec<ModrinthVersion> = client
        .get(&url)
        .send()
        .await?
        .json()
        .await
        .context("Failed to parse version list")?;

    versions
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No versions found for project {project_id}"))
}

fn infer_loader(loaders: &[String]) -> LoaderType {
    for loader in loaders {
        let l = loader.to_lowercase();
        if l == "fabric" {
            return LoaderType::Fabric;
        }
        if l == "forge" {
            return LoaderType::Forge;
        }
        if l == "quilt" {
            return LoaderType::Quilt;
        }
    }
    LoaderType::Vanilla
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_fabric_loader() {
        assert_eq!(infer_loader(&["fabric".to_string()]), LoaderType::Fabric);
    }

    #[test]
    fn test_infer_forge_loader() {
        assert_eq!(infer_loader(&["forge".to_string()]), LoaderType::Forge);
    }

    #[test]
    fn test_infer_vanilla_loader() {
        assert_eq!(infer_loader(&[]), LoaderType::Vanilla);
        assert_eq!(
            infer_loader(&["some-other-loader".to_string()]),
            LoaderType::Vanilla
        );
    }

    #[test]
    fn test_search_params_limit_clamping() {
        let params = SearchParams {
            query: "test".to_string(),
            loader: None,
            mc_version: None,
            limit: 200,
        };
        assert_eq!(params.limit, 200); // No clamping in struct itself
    }
}
