//! Shared types for modpack support across CurseForge, Modrinth, and local sources.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Minecraft game loader type for the modpack.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
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

/// A single downloadable file in a modpack.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModpackFile {
    /// Human-readable filename.
    pub name: String,
    /// Download URL.
    pub url: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// SHA-1 checksum if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    /// Relative path within the instance directory.
    pub install_path: String,
}

/// A modpack that can be installed from various sources.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Modpack {
    /// Unique identifier (e.g., "cf-12345" or "mr-abcde").
    pub id: String,
    /// Display name.
    pub name: String,
    /// Supported Minecraft versions.
    pub mc_versions: Vec<String>,
    /// Preferred loader.
    pub loader: LoaderType,
    /// Loader version if specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    /// Short description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// URL to project homepage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_url: Option<String>,
    /// URL to cover image/logo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// Download count (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_count: Option<u64>,
    /// Files to download and install.
    pub files: Vec<ModpackFile>,
    /// Arbitrary metadata.
    #[serde(default)]
    pub extra: HashMap<String, String>,
}

impl Modpack {
    /// Returns true if the modpack has any files to download.
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    /// Get total size of all files in bytes.
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size_bytes).sum()
    }
}

/// Search parameters for querying modpack APIs.
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    /// Search query string.
    pub query: String,
    /// Filter by loader type.
    pub loader: Option<LoaderType>,
    /// Filter by Minecraft version.
    pub mc_version: Option<String>,
    /// Maximum results (default: 20, max: 100).
    pub limit: u32,
}
