use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LoaderType {
    Fabric,
    Forge,
    Quilt,
    #[default]
    Vanilla,
}

impl std::fmt::Display for LoaderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderType::Fabric => write!(f, "Fabric"),
            LoaderType::Forge => write!(f, "Forge"),
            LoaderType::Quilt => write!(f, "Quilt"),
            LoaderType::Vanilla => write!(f, "Vanilla"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModpackFile {
    pub name: String,
    pub url: String,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    pub install_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Modpack {
    pub id: String,
    pub name: String,
    pub mc_versions: Vec<String>,
    pub loader: LoaderType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_count: Option<u64>,
    pub files: Vec<ModpackFile>,
    #[serde(default)]
    pub extra: HashMap<String, String>,
}

impl Modpack {
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size_bytes).sum()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    pub query: String,
    pub loader: Option<LoaderType>,
    pub mc_version: Option<String>,
    pub limit: u32,
}
