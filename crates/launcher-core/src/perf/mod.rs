pub mod mods;
pub mod options;

pub use mods::{InstalledMod, ManagedManifest};
pub use options::{GraphicsPreset, TunedOptions};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Renderer {
    Vulkan,
    Opengl,
    #[default]
    Auto,
}

impl Renderer {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "opengl" | "gl" => Renderer::Opengl,
            "vulkan" | "vk" => Renderer::Vulkan,
            _ => Renderer::Auto,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Renderer::Vulkan => "vulkan",
            Renderer::Opengl => "opengl",
            Renderer::Auto => "auto",
        }
    }

    pub fn resolved(&self) -> Renderer {
        match self {
            Renderer::Auto => Renderer::Vulkan,
            other => *other,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Renderer::Vulkan => "Vulkan (VulkanMod)",
            Renderer::Opengl => "OpenGL (Sodium)",
            Renderer::Auto => "Auto → Vulkan (VulkanMod)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PerfConfig {
    pub enabled: bool,
    pub preset: GraphicsPreset,
}

impl PerfConfig {
    pub fn recommended() -> Self {
        Self {
            enabled: true,
            preset: GraphicsPreset::for_system_ram(system_ram_mb()),
        }
    }
}

impl GraphicsPreset {
    pub fn for_system_ram(system_ram_mb: u64) -> Self {
        match system_ram_mb {
            0..=8192 => GraphicsPreset::Performance,
            8193..=16384 => GraphicsPreset::Balanced,
            _ => GraphicsPreset::Quality,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PerfReport {
    pub requested: bool,
    pub loader: Option<String>,
    pub installed_mods: usize,
    pub renderer: Option<String>,
    pub renderer_pack: Option<String>,
    pub skipped_mods: Vec<String>,
    pub options_written: bool,
    pub options_changed: bool,
    pub failures: Vec<String>,
}

impl PerfReport {
    pub fn is_vanilla(&self) -> bool {
        self.loader.is_none()
    }

    pub fn summary(&self) -> String {
        if !self.requested {
            return "Performance pack disabled (vanilla)".to_string();
        }
        if self.is_vanilla() {
            return "Vanilla launch — Fabric was unavailable".to_string();
        }
        format!(
            "{} • {} mods{}{}",
            self.loader.clone().unwrap_or_default(),
            self.installed_mods,
            match &self.renderer_pack {
                Some(pack) => format!(" • {pack}"),
                None => String::new(),
            },
            if self.skipped_mods.is_empty() {
                String::new()
            } else {
                format!(" • {} unavailable", self.skipped_mods.len())
            }
        )
    }
}

pub fn system_ram_mb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    if let Some(kb) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb.parse::<u64>() {
                            return kb / 1024;
                        }
                    }
                }
            }
        }
        0
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
        {
            if let Ok(text) = String::from_utf8(output.stdout) {
                if let Ok(bytes) = text.trim().parse::<u64>() {
                    return bytes / 1024 / 1024;
                }
            }
        }
        0
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        std::env::var("NUMBER_OF_PROCESSORS")
            .ok()
            .map(|_| 0)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_tracks_system_memory() {
        assert_eq!(
            GraphicsPreset::for_system_ram(4096),
            GraphicsPreset::Performance
        );
        assert_eq!(
            GraphicsPreset::for_system_ram(8192),
            GraphicsPreset::Performance
        );
        assert_eq!(
            GraphicsPreset::for_system_ram(16384),
            GraphicsPreset::Balanced
        );
        assert_eq!(
            GraphicsPreset::for_system_ram(65536),
            GraphicsPreset::Quality
        );
        assert_eq!(
            GraphicsPreset::for_system_ram(0),
            GraphicsPreset::Performance
        );
    }

    #[test]
    fn recommended_config_enables_the_pack() {
        let config = PerfConfig::recommended();
        assert!(config.enabled, "optimisation should be on by default");
    }

    #[test]
    fn report_summaries_read_sensibly() {
        let disabled = PerfReport::default();
        assert_eq!(disabled.summary(), "Performance pack disabled (vanilla)");

        let vanilla = PerfReport {
            requested: true,
            ..Default::default()
        };
        assert!(vanilla.summary().contains("Vanilla"));

        let full = PerfReport {
            requested: true,
            loader: Some("fabric 0.16.10".into()),
            installed_mods: 20,
            skipped_mods: vec!["LazyDFU".into()],
            ..Default::default()
        };
        let summary = full.summary();
        assert!(summary.contains("fabric 0.16.10"));
        assert!(summary.contains("20 mods"));
        assert!(summary.contains("1 unavailable"));
    }

    #[test]
    fn renderer_parsing_and_resolution() {
        assert_eq!(Renderer::parse("vulkan"), Renderer::Vulkan);
        assert_eq!(Renderer::parse("OpenGL"), Renderer::Opengl);
        assert_eq!(Renderer::parse("gl"), Renderer::Opengl);
        assert_eq!(Renderer::parse("auto"), Renderer::Auto);
        assert_eq!(Renderer::parse("metal"), Renderer::Auto);
        assert_eq!(Renderer::Auto.resolved(), Renderer::Vulkan);
        assert_ne!(Renderer::Auto.resolved(), Renderer::Auto);
        for r in [Renderer::Auto, Renderer::Vulkan, Renderer::Opengl] {
            assert_eq!(Renderer::parse(r.as_str()), r);
        }
    }

    #[test]
    fn summary_names_the_renderer_pack() {
        let report = PerfReport {
            requested: true,
            loader: Some("fabric 0.19.5".into()),
            installed_mods: 12,
            renderer: Some("vulkan".into()),
            renderer_pack: Some("Vulkan (VulkanMod)".into()),
            ..Default::default()
        };
        let summary = report.summary();
        assert!(summary.contains("VulkanMod"), "got: {summary}");
    }

    #[test]
    fn system_ram_probe_does_not_panic() {
        let _ = system_ram_mb();
    }
}
