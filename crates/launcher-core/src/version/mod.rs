//! Minecraft version management — caching, filtering, resolution.

pub mod cache;
pub mod filter;
pub mod resolver;

pub use cache::VersionCache;
pub use filter::VersionFilter;
pub use resolver::VersionResolver;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single version entry as returned by the Mojang manifest.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: VersionType,
    pub url: String,
    pub time: DateTime<Utc>,
    #[serde(rename = "releaseTime", default)]
    pub release_time: Option<DateTime<Utc>>,
}

impl VersionInfo {
    pub fn is_release(&self) -> bool {
        self.type_ == VersionType::Release
    }

    pub fn is_snapshot(&self) -> bool {
        self.type_ == VersionType::Snapshot
    }

    /// Year this version was released, or current year if unknown.
    pub fn release_year(&self) -> u32 {
        use chrono::Datelike;
        self.release_time
            .or(Some(self.time))
            .map(|dt| dt.year() as u32)
            .unwrap_or_else(|| Utc::now().year() as u32)
    }

    /// Display label for the UI, e.g. `"1.21.1 • Release"`.
    pub fn display_label(&self) -> String {
        let type_tag = match self.type_ {
            VersionType::Release => "",
            VersionType::Snapshot => " (Snapshot)",
        };
        format!("{}{}", self.id, type_tag)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum VersionType {
    Release,
    Snapshot,
}

impl std::fmt::Display for VersionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionType::Release => write!(f, "Release"),
            VersionType::Snapshot => write!(f, "Snapshot"),
        }
    }
}
