use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
    pub username: String,
    pub ram_mb: Option<u64>,
    pub renderer: String,
    pub java_path: Option<String>,
    pub auto_update: bool,
    pub snapshots: bool,
    pub reduced_motion: bool,
    pub performance_pack: bool,
    pub graphics_preset: String,
    pub selected_instance: Option<String>,
    pub curseforge_api_key: String,
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
            selected_instance: None,
            curseforge_api_key: String::new(),
        }
    }
}

impl LauncherConfig {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join(CONFIG_FILE)
    }

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
            selected_instance: Some("inst-1".to_string()),
            ..Default::default()
        };
        cfg.save(dir.path()).unwrap();

        let loaded = LauncherConfig::load(dir.path());
        assert_eq!(loaded.username, "Steve");
        assert_eq!(loaded.ram_mb, Some(2048));
        assert_eq!(loaded.selected_instance.as_deref(), Some("inst-1"));

        std::fs::write(LauncherConfig::path(dir.path()), b"{ not json").unwrap();
        let fallback = LauncherConfig::load(dir.path());
        assert_eq!(fallback.username, "");
    }
}
