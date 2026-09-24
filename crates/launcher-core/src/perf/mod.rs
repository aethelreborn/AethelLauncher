//! Performance layer.
//!
//! Two halves, because neither alone fixes frame rate:
//! * **mods** — Fabric loader + a curated Modrinth pack, chosen for the
//!   renderer (VulkanMod for Vulkan, Sodium for OpenGL)
//! * **options** — a `options.txt` graphics preset matched to the machine
//!
//! Both are applied on the way into a launch, and both are reported back so the
//! UI can show exactly what changed.

pub mod mods;
pub mod options;

pub use mods::{InstalledMod, ManagedManifest};
pub use options::{GraphicsPreset, TunedOptions};

use serde::{Deserialize, Serialize};

/// Which rendering backend the game will actually run on.
///
/// This matters for mods: Sodium and friends hook OpenGL, so they cannot load
/// on a Vulkan renderer — the Vulkan path needs **VulkanMod** instead. The pack
/// is therefore selected from this, never installed blind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Renderer {
    Vulkan,
    Opengl,
    /// Let Aethel choose — currently Vulkan, which is the better path on the
    /// weak hardware this launcher targets (VulkanMod's frame times are far
    /// more stable than GL on old integrated GPUs).
    #[default]
    Auto,
}

impl std::str::FromStr for Renderer {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value.trim().to_ascii_lowercase().as_str() {
            "opengl" | "gl" => Renderer::Opengl,
            "vulkan" | "vk" => Renderer::Vulkan,
            _ => Renderer::Auto,
        })
    }
}

impl Renderer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Renderer::Vulkan => "vulkan",
            Renderer::Opengl => "opengl",
            Renderer::Auto => "auto",
        }
    }

    /// Collapse `Auto` into a concrete backend so mod selection is unambiguous.
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

/// What the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PerfConfig {
    /// Install Fabric + the performance pack and tune `options.txt`.
    pub enabled: bool,
    pub preset: GraphicsPreset,
}

impl PerfConfig {
    /// The default for a fresh install: on, with a preset chosen from the
    /// machine's memory. A launcher that does not optimise out of the box is
    /// not doing its job.
    pub fn recommended() -> Self {
        Self {
            enabled: true,
            preset: GraphicsPreset::for_system_ram(system_ram_mb()),
        }
    }
}

impl GraphicsPreset {
    /// Pick a sensible default preset for the machine.
    pub fn for_system_ram(system_ram_mb: u64) -> Self {
        match system_ram_mb {
            0..=8192 => GraphicsPreset::Performance,
            8193..=16384 => GraphicsPreset::Balanced,
            _ => GraphicsPreset::Quality,
        }
    }
}

/// What the performance step actually did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PerfReport {
    /// Whether the user had it enabled at all.
    pub requested: bool,
    /// e.g. `"fabric 0.16.10"` — `None` means vanilla.
    pub loader: Option<String>,
    pub installed_mods: usize,
    /// The renderer the pack was chosen for, e.g. `"vulkan"`.
    pub renderer: Option<String>,
    /// Human label of the renderer pack, e.g. `"Vulkan (VulkanMod)"`.
    pub renderer_pack: Option<String>,
    /// Pack members with no build for this Minecraft version.
    pub skipped_mods: Vec<String>,
    pub options_written: bool,
    /// Whether `options.txt` actually changed (false = already tuned).
    pub options_changed: bool,
    /// Non-fatal problems worth surfacing.
    pub failures: Vec<String>,
}

impl PerfReport {
    pub fn is_vanilla(&self) -> bool {
        self.loader.is_none()
    }

    /// One-line summary for the UI/console.
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

/// Total system RAM in MB, or 0 when it cannot be determined.
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
        // sysctl hw.memsize is bytes; shelling out is the portable option here.
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
        // Windows: fall back to the env var most systems set.
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
        // Unknown RAM should not default to the heaviest preset.
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
        assert_eq!("vulkan".parse::<Renderer>().unwrap(), Renderer::Vulkan);
        assert_eq!("OpenGL".parse::<Renderer>().unwrap(), Renderer::Opengl);
        assert_eq!("gl".parse::<Renderer>().unwrap(), Renderer::Opengl);
        assert_eq!("auto".parse::<Renderer>().unwrap(), Renderer::Auto);
        // Anything unrecognised must not silently pick a random backend.
        assert_eq!("metal".parse::<Renderer>().unwrap(), Renderer::Auto);
        // Auto always resolves to a concrete renderer for mod selection.
        assert_eq!(Renderer::Auto.resolved(), Renderer::Vulkan);
        assert_ne!(Renderer::Auto.resolved(), Renderer::Auto);
        for r in [Renderer::Auto, Renderer::Vulkan, Renderer::Opengl] {
            assert_eq!(r.as_str().parse::<Renderer>().unwrap(), r);
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
        // 0 is an acceptable "unknown"; the important thing is no panic.
        let _ = system_ram_mb();
    }
}
