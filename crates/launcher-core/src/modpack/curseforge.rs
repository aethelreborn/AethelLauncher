use super::types::{LoaderType, Modpack, ModpackFile, SearchParams};
use anyhow::Context;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{debug, warn};

const CURSEFORGE_API_BASE: &str = "https://api.curseforge.com/v1";

#[derive(Debug, Deserialize)]
struct CfApiResponse<T> {
    data: T,
    #[allow(dead_code)]
    #[serde(rename = "totalCount")]
    total_count: u64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CfSearchResult {
    #[serde(rename = "mods")]
    mods: Vec<CfEntry>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct CfEntry {
    #[serde(rename = "id")]
    project_id: u64,
    slug: String,
    name: String,
    summary: Option<String>,
    #[serde(rename = "downloadCount")]
    download_count: Option<u64>,
    #[serde(rename = "websiteUrl")]
    project_url: Option<String>,
    #[serde(rename = "iconUrl")]
    icon_url: Option<String>,
    #[serde(rename = "categories")]
    categories: Vec<CfCategory>,
    #[serde(rename = "latestFiles")]
    latest_files: Vec<CfFile>,
}

#[derive(Debug, Deserialize, Clone)]
struct CfCategory {
    name: String,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct CfFile {
    #[serde(rename = "id")]
    file_id: u64,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "fileSize")]
    size_bytes: u64,
    #[serde(rename = "downloadUrl")]
    download_url: String,
    #[serde(rename = "gameVersionIds")]
    game_version_ids: Vec<u64>,
}

pub async fn search_modpacks(
    client: &Client,
    params: &SearchParams,
    api_key: Option<&str>,
) -> anyhow::Result<Vec<Modpack>> {
    let limit = (params.limit.min(100) as u64).max(1);
    let mut url = format!(
        "{CURSEFORGE_API_BASE}/mods/search?gameId=432&searchFilter={}&pageSize={limit}",
        urlencoding::encode(&params.query),
    );

    if let Some(loader) = &params.loader {
        let loader_param = match loader {
            LoaderType::Fabric => "fabric",
            LoaderType::Forge => "forge",
            LoaderType::Quilt => "quilt",
            LoaderType::Vanilla => return Ok(Vec::new()),
        };
        url.push_str(&format!("&classId={loader_param}"));
    }

    debug!("Searching CurseForge: {}", url);

    let mut req = client.get(&url);
    if let Some(key) = api_key {
        req = req.header("x-api-key", key);
    }

    let resp: CfApiResponse<CfSearchResult> = req.send().await?.json().await?;

    let entries = match resp.data.mods.len() {
        0 => Vec::new(),
        n => {
            let mut chunks: Vec<_> = resp.data.mods.chunks(n.min(50)).collect();
            let first_chunk = chunks.remove(0);
            first_chunk.to_vec()
        }
    };

    let mut result = Vec::new();
    for entry in entries {
        if let Ok(modpack) = convert_cf_entry(client, entry, api_key).await {
            result.push(modpack);
        } else {
            warn!("Failed to process CurseForge entry");
        }
    }
    Ok(result)
}

pub async fn get_modpack_details(
    client: &Client,
    project_id: u64,
    api_key: Option<&str>,
) -> anyhow::Result<Modpack> {
    let url = format!("{CURSEFORGE_API_BASE}/mods/{project_id}");
    let mut req = client.get(&url);
    if let Some(key) = api_key {
        req = req.header("x-api-key", key);
    }

    let resp: CfApiResponse<CfEntry> = req.send().await?.json().await?;
    convert_cf_entry(client, resp.data, api_key).await
}

async fn convert_cf_entry(
    _client: &Client,
    entry: CfEntry,
    _api_key: Option<&str>,
) -> anyhow::Result<Modpack> {
    let primary_file = entry
        .latest_files
        .first()
        .context("No files found for CurseForge mod")?;

    let loader = infer_loader_from_categories(&entry.categories);
    let mc_version = infer_mc_version(primary_file)?;

    Ok(Modpack {
        id: format!("cf-{}", entry.project_id),
        name: entry.name,
        mc_versions: vec![mc_version],
        loader,
        loader_version: None,
        description: entry.summary,
        project_url: entry.project_url,
        icon_url: entry.icon_url,
        download_count: entry.download_count,
        files: vec![ModpackFile {
            name: primary_file.file_name.clone(),
            url: primary_file.download_url.clone(),
            size_bytes: primary_file.size_bytes,
            sha1: None,
            install_path: format!("modpacks/{}", primary_file.file_name),
        }],
        extra: HashMap::new(),
    })
}

fn infer_loader_from_categories(categories: &[CfCategory]) -> LoaderType {
    for cat in categories {
        let name = cat.name.to_lowercase();
        if name.contains("fabric") {
            return LoaderType::Fabric;
        }
        if name.contains("forge") {
            return LoaderType::Forge;
        }
        if name.contains("quilt") {
            return LoaderType::Quilt;
        }
    }
    LoaderType::Vanilla
}

fn infer_mc_version(_file: &CfFile) -> anyhow::Result<String> {
    Ok("unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modpack::types::LoaderType;

    #[test]
    fn test_infer_fabric_loader() {
        let cats = vec![CfCategory {
            name: "Fabric".to_string(),
        }];
        assert_eq!(infer_loader_from_categories(&cats), LoaderType::Fabric);
    }

    #[test]
    fn test_infer_forge_loader() {
        let cats = vec![CfCategory {
            name: "Forge".to_string(),
        }];
        assert_eq!(infer_loader_from_categories(&cats), LoaderType::Forge);
    }

    #[test]
    fn test_infer_vanilla_loader() {
        let cats = vec![CfCategory {
            name: "Adventure".to_string(),
        }];
        assert_eq!(infer_loader_from_categories(&cats), LoaderType::Vanilla);
    }
}
