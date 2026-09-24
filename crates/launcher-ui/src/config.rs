//! Launcher config persistence (`<data dir>/config.json`).
//!
//! Small enough that a single JSON file is the right store — it is read once at
//! startup and written whenever the user changes an account or setting.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
    /// Offline username. Empty means "not set up yet".
    pub username: String,
    /// Heap size in MB; `None` uses the RAM-derived default.
    pub ram_mb: Option<u64>,
    /// `"vulkan" | "opengl" | "auto"`.
    pub renderer: String,
    /// Renderer ids the user has explicitly chosen.
    pub java_path: Option<String>,
    pub auto_update: bool,
    /// Include snapshots in the version picker.
    pub snapshots: bool,
    /// Disable all animations (accessibility, and helps on weak GPUs).
    pub reduced_motion: bool,
    /// Install Fabric + the performance mod pack before launching.
    pub performance_pack: bool,
    /// `performance | balanced | quality`
    pub graphics_preset: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            ram_mb: None,
            renderer: "vulkan".to_string(),
            java_path: None,
            auto_update: true,
            snapshots: false,
            reduced_motion: false,
            performance_pack: true,
            graphics_preset: "performance".to_string(),
        }
    }
}

impl LauncherConfig {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join(CONFIG_FILE)
    }

    /// Load config, falling back to defaults on any error (missing/corrupt).
    pub fn load(data_dir: &Path) -> Self {
        let path = Self::path(data_dir);
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                tracing::warn!(
                    "config at {} is corrupt ({e}); using defaults",
                    path.display()
                );
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, data_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let path = Self::path(data_dir);
        let raw = serde_json::to_vec_pretty(self)?;
        // Write to a temp file then rename so a crash cannot truncate config.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, raw)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults_on_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = LauncherConfig {
            username: "Steve".to_string(),
            ram_mb: Some(2048),
            ..Default::default()
        };
        cfg.save(dir.path()).unwrap();

        let loaded = LauncherConfig::load(dir.path());
        assert_eq!(loaded.username, "Steve");
        assert_eq!(loaded.ram_mb, Some(2048));

        std::fs::write(LauncherConfig::path(dir.path()), b"{ not json").unwrap();
        let fallback = LauncherConfig::load(dir.path());
        assert_eq!(fallback.username, "");
    }
}
