//! Settings screen — renderer, memory, Java path, channels. Backed by the
//! persisted [`LauncherConfig`](crate::config::LauncherConfig).

use super::super::theme;
use super::home;
use crate::config::LauncherConfig;
use eframe::egui;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RendererMode {
    #[default]
    Vulkan,
    OpenGL,
    Auto,
}

impl RendererMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RendererMode::Vulkan => "vulkan",
            RendererMode::OpenGL => "opengl",
            RendererMode::Auto => "auto",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RendererMode::Vulkan => "Vulkan ⚡ (Recommended)",
            RendererMode::OpenGL => "OpenGL",
            RendererMode::Auto => "Auto",
        }
    }
}

impl std::str::FromStr for RendererMode {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value.to_ascii_lowercase().as_str() {
            "opengl" | "gl" => RendererMode::OpenGL,
            "auto" => RendererMode::Auto,
            _ => RendererMode::Vulkan,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub renderer_mode: RendererMode,
    pub max_ram_mb: u64,
    /// Empty means "auto-detect".
    pub java_path: String,
    pub auto_update: bool,
    pub snapshot_channel: bool,
    pub data_dir: String,
    pub dirty: bool,
    pub notice: Option<String>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            renderer_mode: RendererMode::default(),
            max_ram_mb: home::ram_cap_mb() * 3 / 4,
            java_path: String::new(),
            auto_update: true,
            snapshot_channel: false,
            data_dir: String::new(),
            dirty: false,
            notice: None,
        }
    }
}

impl SettingsState {
    pub fn from_config(config: &LauncherConfig, data_dir: &Path) -> Self {
        Self {
            renderer_mode: config.renderer.parse().unwrap_or_default(),
            max_ram_mb: config.ram_mb.unwrap_or_else(|| home::ram_cap_mb() * 3 / 4),
            java_path: config.java_path.clone().unwrap_or_default(),
            auto_update: config.auto_update,
            snapshot_channel: config.snapshots,
            data_dir: data_dir.to_string_lossy().to_string(),
            dirty: false,
            notice: None,
        }
    }

    /// Fold the edited values back into the persisted config.
    pub fn apply_to(&self, config: &mut LauncherConfig) {
        config.renderer = self.renderer_mode.as_str().to_string();
        config.ram_mb = Some(self.max_ram_mb);
        config.java_path = {
            let trimmed = self.java_path.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };
        config.auto_update = self.auto_update;
        config.snapshots = self.snapshot_channel;
    }

    /// A Java path that does not exist is the most common settings mistake, so
    /// surface it before it silently breaks a launch.
    pub fn java_path_status(&self) -> Option<Result<(), String>> {
        let path = self.java_path.trim();
        if path.is_empty() {
            return None;
        }
        Some(if Path::new(path).is_file() {
            Ok(())
        } else {
            Err(format!("No file at {path}"))
        })
    }
}

pub fn settings_panel(ui: &mut egui::Ui, state: &mut SettingsState) -> bool {
    if state.data_dir.is_empty() {
        state.data_dir = launcher_core::CoreHandle::home_dir()
            .to_string_lossy()
            .to_string();
    }
    let cap = home::ram_cap_mb();
    if state.max_ram_mb > cap {
        state.max_ram_mb = cap;
    }

    ui.label(
        egui::RichText::new("Settings")
            .size(24.0)
            .color(theme::ACCENT_CYAN),
    );
    ui.separator();
    ui.add_space(8.0);

    // --- Rendering --------------------------------------------------------
    theme::glass_card(ui, |ui| {
        ui.label(
            egui::RichText::new("Rendering")
                .size(16.0)
                .color(theme::TEXT_PRIMARY),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Backend").color(theme::TEXT_SECONDARY));
            let before = state.renderer_mode;
            egui::ComboBox::from_id_salt("settings_renderer")
                .width(220.0)
                .selected_text(state.renderer_mode.label())
                .show_ui(ui, |ui| {
                    for mode in [
                        RendererMode::Vulkan,
                        RendererMode::OpenGL,
                        RendererMode::Auto,
                    ] {
                        ui.selectable_value(&mut state.renderer_mode, mode, mode.label());
                    }
                });
            if state.renderer_mode != before {
                state.dirty = true;
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Passed to the game as -Daethel.renderer. Vulkan is preferred; pick OpenGL \
                 if the game crashes on startup.",
            )
            .size(11.0)
            .color(theme::BORDER),
        );
        ui.add_space(4.0);
        let renderer = state
            .renderer_mode
            .as_str()
            .parse::<launcher_core::perf::Renderer>()
            .unwrap_or_default();
        ui.label(
            egui::RichText::new(format!(
                "Performance pack: {}",
                launcher_core::perf::mods::pack_label(renderer)
            ))
            .size(11.0)
            .color(theme::ACCENT_CYAN),
        );
        ui.label(
            egui::RichText::new(
                "Sodium only runs on OpenGL, so the Vulkan path installs VulkanMod instead.",
            )
            .size(11.0)
            .color(theme::BORDER),
        );
    });

    ui.add_space(10.0);

    // --- Memory -----------------------------------------------------------
    theme::glass_card(ui, |ui| {
        ui.label(
            egui::RichText::new("Memory")
                .size(16.0)
                .color(theme::TEXT_PRIMARY),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Allocate").color(theme::TEXT_SECONDARY));
            let before = state.max_ram_mb;
            ui.add(
                egui::Slider::new(&mut state.max_ram_mb, 1024..=cap)
                    .logarithmic(true)
                    .text("MB"),
            );
            if state.max_ram_mb != before {
                state.dirty = true;
            }
            ui.label(
                egui::RichText::new(format!("of {} MB total", home::system_ram_mb()))
                    .color(theme::TEXT_SECONDARY),
            );
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Capped to your installed memory so you can't over-allocate.")
                .size(11.0)
                .color(theme::BORDER),
        );
    });

    ui.add_space(10.0);

    // --- Java -------------------------------------------------------------
    theme::glass_card(ui, |ui| {
        ui.label(
            egui::RichText::new("Java")
                .size(16.0)
                .color(theme::TEXT_PRIMARY),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Path").color(theme::TEXT_SECONDARY));
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.java_path)
                    .desired_width(360.0)
                    .hint_text("Auto-detect (leave empty)"),
            );
            if response.changed() {
                state.dirty = true;
            }
        });
        match state.java_path_status() {
            Some(Ok(())) => {
                ui.label(
                    egui::RichText::new("✓ Path exists")
                        .size(11.0)
                        .color(theme::SUCCESS_GREEN),
                );
            }
            Some(Err(message)) => {
                ui.label(
                    egui::RichText::new(format!("⚠ {message}"))
                        .size(11.0)
                        .color(theme::DANGER_RED),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new("Left empty, Aethel picks the best Java it can find.")
                        .size(11.0)
                        .color(theme::BORDER),
                );
            }
        }
    });

    ui.add_space(10.0);

    // --- General ----------------------------------------------------------
    theme::glass_card(ui, |ui| {
        ui.label(
            egui::RichText::new("General")
                .size(16.0)
                .color(theme::TEXT_PRIMARY),
        );
        ui.add_space(6.0);
        if ui
            .checkbox(
                &mut state.auto_update,
                "Check for launcher updates on startup",
            )
            .changed()
        {
            state.dirty = true;
        }
        if ui
            .checkbox(
                &mut state.snapshot_channel,
                "Include snapshot builds in the version list",
            )
            .changed()
        {
            state.dirty = true;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Data dir").color(theme::TEXT_SECONDARY));
            ui.label(
                egui::RichText::new(&state.data_dir)
                    .size(11.0)
                    .color(theme::BORDER),
            );
        });
    });

    ui.add_space(16.0);

    let mut save = false;
    ui.horizontal(|ui| {
        ui.add_enabled(
            state.dirty,
            egui::Button::new(egui::RichText::new("Save settings").color(theme::SUCCESS_GREEN))
                .min_size(egui::vec2(140.0, 30.0)),
        )
        .clicked()
        .then(|| save = true);

        if let Some(notice) = &state.notice {
            ui.label(egui::RichText::new(notice).color(theme::SUCCESS_GREEN));
        }
    });

    save
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_mode_round_trips() {
        for mode in [
            RendererMode::Vulkan,
            RendererMode::OpenGL,
            RendererMode::Auto,
        ] {
            assert_eq!(mode.as_str().parse::<RendererMode>().unwrap(), mode);
        }
        // Unknown values fall back to the recommended default.
        assert_eq!(
            "garbage".parse::<RendererMode>().unwrap(),
            RendererMode::Vulkan
        );
    }

    #[test]
    fn apply_to_persists_edited_values() {
        let mut config = LauncherConfig::default();
        let state = SettingsState {
            renderer_mode: RendererMode::OpenGL,
            max_ram_mb: 3072,
            java_path: "  ".to_string(),
            snapshot_channel: true,
            ..Default::default()
        };
        state.apply_to(&mut config);

        assert_eq!(config.renderer, "opengl");
        assert_eq!(config.ram_mb, Some(3072));
        assert_eq!(config.java_path, None);
        assert!(config.snapshots);
    }

    #[test]
    fn java_path_status_flags_missing_files() {
        let mut state = SettingsState::default();
        assert!(state.java_path_status().is_none());
        state.java_path = "/definitely/not/here/java".to_string();
        assert!(matches!(state.java_path_status(), Some(Err(_))));
    }
}
