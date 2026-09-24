//! Launcher core engine.

pub mod auth;
pub mod bundle;
pub mod install;
pub mod instance;
pub mod launch;
pub mod manifest;
pub mod modpack;
pub mod perf;
pub mod platform;
pub mod telemetry;
pub mod update;
pub mod version;

pub use auth::*;
pub use bundle::manifest::*;
pub use instance::store::*;
pub use version::{VersionCache, VersionFilter, VersionResolver};

/// Library facade.
#[non_exhaustive]
pub struct CoreHandle {
    pub base_dir: std::path::PathBuf,
    pub cache_dir: std::path::PathBuf,
}

impl CoreHandle {
    pub fn home_dir() -> std::path::PathBuf {
        dirs::data_local_dir()
            .map(|d| d.join("Aethel"))
            .or_else(|| dirs::config_local_dir().map(|d| d.join("Aethel")))
            .unwrap_or_else(|| std::env::current_dir().unwrap().join(".aethel"))
    }

    pub fn new() -> Self {
        let home = Self::home_dir();
        std::fs::create_dir_all(&home).expect("failed to create aethel home dir");
        let cache = home.join("cache");
        std::fs::create_dir_all(&cache).expect("failed to create cache dir");
        Self {
            base_dir: home,
            cache_dir: cache,
        }
    }
}

impl Default for CoreHandle {
    fn default() -> Self {
        Self::new()
    }
}
