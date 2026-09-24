//! Home screen — version picker, renderer preset, memory, and the real launch
//! controls (Play / Stop with progress + console).

use super::super::branding;
use super::super::launch::{LaunchController, LaunchRequest, LaunchStatus};
use super::super::theme;
use super::settings::RendererMode;
use eframe::egui;
use launcher_core::auth::Account;
use launcher_core::perf::{mods as perf_mods, GraphicsPreset, PerfConfig, Renderer};
use launcher_core::VersionCache;
use std::path::PathBuf;
use sysinfo::System;

/// Total RAM of this machine in MB (cached after first probe).
pub fn system_ram_mb() -> u64 {
    use std::sync::OnceLock;
    static RAM: OnceLock<u64> = OnceLock::new();
    *RAM.get_or_init(|| System::new_all().total_memory() / 1024 / 1024)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuickFilter {
    All,
    Release,
    Snapshot,
    Latest,
}

/// Safe upper bound for the game RAM slider: never above total system RAM
/// (capped at 16384 MB for safety with large machines).
pub fn ram_cap_mb() -> u64 {
    system_ram_mb().clamp(1024, 16384)
}

/// Filesystem locations a launch needs.
#[derive(Debug, Clone)]
pub struct LaunchEnv {
    /// Shared content root (`libraries/`, `assets/`, `versions/`, `runtimes/`).
    pub shared_dir: PathBuf,
    /// Where per-instance game directories live.
    pub instances_dir: PathBuf,
    pub java_path: Option<PathBuf>,
}

impl Default for LaunchEnv {
    fn default() -> Self {
        let base = launcher_core::CoreHandle::home_dir();
        Self {
            shared_dir: base.clone(),
            instances_dir: base.join("instances"),
            java_path: None,
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub selected_version: String,
    pub available_versions: Vec<String>,
    pub visible_versions: Vec<String>,
    pub renderer_mode: RendererMode,
    pub ram_mb: u64,
    pub active_filter: QuickFilter,
    pub search_query: String,
    pub last_error: Option<String>,
    pub cache: Option<VersionCache>,
    pub fetching: bool,
    pub show_console: bool,
    /// Set when Play is pressed from the Library, so that instance's own game
    /// directory and RAM are used instead of the ad-hoc version directory.
    pub pending_instance: Option<PendingInstance>,
    /// Fabric + performance mods + graphics preset.
    pub perf: PerfConfig,
    pub env: LaunchEnv,
    rx: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
}

#[derive(Debug, Clone)]
pub struct PendingInstance {
    pub id: String,
    pub name: String,
    pub ram_mb: u64,
    pub renderer: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            selected_version: String::new(),
            available_versions: Vec::new(),
            visible_versions: Vec::new(),
            renderer_mode: RendererMode::default(),
            ram_mb: (system_ram_mb() * 3 / 4).max(1024),
            active_filter: QuickFilter::All,
            search_query: String::new(),
            last_error: None,
            cache: None,
            fetching: false,
            show_console: false,
            pending_instance: None,
            // Optimisation is on out of the box — that is the whole point.
            perf: PerfConfig::recommended(),
            env: LaunchEnv::default(),
            rx: None,
        }
    }
}

impl AppState {
    /// Build state with a version cache and kick off a background fetch of the
    /// Mojang manifest. Results arrive asynchronously through `poll_fetch`.
    pub fn new(cache: VersionCache) -> Self {
        let mut state = Self {
            cache: Some(cache),
            ..Default::default()
        };
        state.reload_versions();
        state
    }

    /// (Re)start the background version-manifest fetch.
    pub fn reload_versions(&mut self) {
        let Some(cache) = self.cache.clone() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.fetching = true;
        self.last_error = None;

        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                let client = reqwest::Client::new();
                rt.block_on(cache.fetch(&client))
                    .map(|v| v.into_iter().map(|vi| vi.id).collect::<Vec<_>>())
                    .map_err(|e| e.to_string())
            }))
            .unwrap_or_else(|_| Err("version fetch panicked".to_string()));
            let _ = tx.send(result);
        });
    }

    /// Poll the background fetch thread. Call once per UI frame.
    pub fn poll_fetch(&mut self) {
        let Some(rx) = &self.rx else { return };
        match rx.try_recv() {
            Ok(Ok(versions)) => {
                if versions.is_empty() {
                    self.last_error = Some("Mojang returned an empty version list.".to_string());
                }
                self.available_versions = versions;
                self.fetching = false;
                self.rebuild_visible();
                self.rx = None;
            }
            Ok(Err(e)) => {
                self.fetching = false;
                self.last_error = Some(e);
                self.rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.fetching = false;
                self.rx = None;
            }
        }
    }

    pub fn rebuild_visible(&mut self) {
        let search = self.search_query.trim().to_lowercase();
        // The manifest is ordered newest-first, so "Latest" is the first entry.
        let latest = self.available_versions.first().cloned();
        self.visible_versions = self
            .available_versions
            .iter()
            .filter(|v| {
                let is_snapshot = !looks_like_release(v);
                let matches_filter = match self.active_filter {
                    QuickFilter::All => true,
                    QuickFilter::Release => !is_snapshot,
                    QuickFilter::Snapshot => is_snapshot,
                    QuickFilter::Latest => latest.as_deref() == Some(v.as_str()),
                };
                matches_filter && (search.is_empty() || v.to_lowercase().contains(&search))
            })
            .cloned()
            .collect();

        if !self.visible_versions.contains(&self.selected_version)
            && !self.visible_versions.is_empty()
        {
            self.selected_version = self.visible_versions[0].clone();
        }
    }

    /// Game directory for the next launch: the pending instance's own folder,
    /// or a folder named after the selected version.
    pub fn game_dir(&self) -> PathBuf {
        let folder = match &self.pending_instance {
            Some(instance) => sanitize_folder(&instance.id),
            None => sanitize_folder(&self.selected_version),
        };
        self.env.instances_dir.join(folder).join("minecraft")
    }

    /// Assemble a launch request from the current UI state.
    pub fn launch_request(&self, auth: &Account) -> LaunchRequest {
        let (ram_mb, renderer) = match &self.pending_instance {
            Some(instance) => (instance.ram_mb.max(1024), instance.renderer.clone()),
            None => (self.ram_mb, self.renderer_mode.as_str().to_string()),
        };

        LaunchRequest {
            version_id: self.selected_version.clone(),
            game_dir: self.game_dir(),
            shared_dir: self.env.shared_dir.clone(),
            ram_mb,
            renderer,
            auth: auth.clone(),
            java_path: self.env.java_path.clone(),
            performance: self.perf,
        }
    }
}

/// Releases look like `1.21.4`; snapshots look like `25w14a` (or old betas).
fn looks_like_release(id: &str) -> bool {
    let mut parts = id.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    !first.is_empty() && first.chars().all(|c| c.is_ascii_digit())
}

fn sanitize_folder(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

/// What the Home screen wants the shell to do once the frame is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HomeRequest {
    #[default]
    None,
    /// Start a launch — the shell builds the request so it can also record it.
    Launch,
}

pub fn home_panel(
    ui: &mut egui::Ui,
    state: &mut AppState,
    controller: &mut LaunchController,
) -> HomeRequest {
    let mut request = HomeRequest::None;
    // Header
    ui.horizontal(|ui| {
        branding::wordmark(ui, 34.0, 20.0);
        ui.add_space(theme::SPACE_4);
        if state.fetching {
            ui.spinner();
            ui.label(egui::RichText::new("Fetching versions...").color(theme::TEXT_SECONDARY));
        } else {
            theme::badge(
                ui,
                &format!("{} versions", state.available_versions.len()),
                theme::ACCENT,
            );
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // The performance badge is the headline feature — show what it did
            // on the last launch, right where people look.
            if controller.status != LaunchStatus::Idle {
                if let Some(report) = &controller.last_perf {
                    let (label, color) = if report.is_vanilla() {
                        ("Vanilla".to_string(), theme::WARN)
                    } else {
                        (report.summary(), theme::SUCCESS)
                    };
                    theme::badge(ui, &label, color);
                }
            }
        });
    });
    ui.separator();

    // Error banner (shows the most recent problem to the user)
    let banner = controller
        .error
        .clone()
        .or_else(|| state.last_error.clone());
    if let Some(err) = banner {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("⚠ ").color(theme::DANGER_RED));
            ui.label(egui::RichText::new(&err).color(theme::DANGER_RED));
            if ui.small_button("✕").clicked() {
                state.last_error = None;
                controller.error = None;
            }
        });
        ui.separator();
    }

    ui.vertical_centered(|ui| {
        // --- Configuration card ------------------------------------------
        egui::ScrollArea::vertical()
            .id_salt("home_config")
            .max_height(240.0)
            .show(ui, |ui| {
                theme::glass_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Version").color(theme::TEXT_SECONDARY));
                        let enabled = !controller.is_active();
                        ui.add_enabled_ui(enabled, |ui| {
                            egui::ComboBox::from_id_salt("home_version")
                                .width(220.0)
                                .selected_text(if state.selected_version.is_empty() {
                                    "Select...".to_string()
                                } else {
                                    state.selected_version.clone()
                                })
                                .show_ui(ui, |ui| {
                                    if state.visible_versions.is_empty() {
                                        ui.label(
                                            egui::RichText::new(if state.fetching {
                                                "Fetching…"
                                            } else {
                                                "No versions match this filter"
                                            })
                                            .color(theme::TEXT_SECONDARY),
                                        );
                                    }
                                    for v in &state.visible_versions {
                                        ui.selectable_value(
                                            &mut state.selected_version,
                                            v.clone(),
                                            v.clone(),
                                        );
                                    }
                                });
                        });

                        ui.add_space(8.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut state.search_query)
                                .hint_text("Search versions…")
                                .desired_width(140.0),
                        );
                        if ui
                            .small_button("⟳")
                            .on_hover_text("Reload version list")
                            .clicked()
                        {
                            state.reload_versions();
                        }
                    });

                    let mut filter_changed = false;
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Filter:").color(theme::TEXT_SECONDARY));
                        for (filter, label) in [
                            (QuickFilter::All, "All"),
                            (QuickFilter::Release, "Releases"),
                            (QuickFilter::Snapshot, "Snapshots"),
                            (QuickFilter::Latest, "Latest"),
                        ] {
                            let active = state.active_filter == filter;
                            let btn = egui::Button::new(label).fill(if active {
                                theme::ACCENT_VIOLET
                            } else {
                                theme::SURFACE_LIGHT
                            });
                            if ui.add(btn).clicked() {
                                state.active_filter = filter;
                                filter_changed = true;
                            }
                        }
                    });
                    if filter_changed {
                        state.rebuild_visible();
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Renderer").color(theme::TEXT_SECONDARY));
                        egui::ComboBox::from_id_salt("home_renderer")
                            .width(180.0)
                            .selected_text(state.renderer_mode.label())
                            .show_ui(ui, |ui| {
                                for mode in [
                                    RendererMode::Vulkan,
                                    RendererMode::OpenGL,
                                    RendererMode::Auto,
                                ] {
                                    ui.selectable_value(
                                        &mut state.renderer_mode,
                                        mode,
                                        mode.label(),
                                    );
                                }
                            });
                    });

                    ui.add_space(8.0);
                    let cap = ram_cap_mb();
                    if state.ram_mb > cap {
                        state.ram_mb = cap;
                    }
                    let ram_full_mb = system_ram_mb();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Memory").color(theme::TEXT_SECONDARY));
                        ui.add(
                            egui::Slider::new(&mut state.ram_mb, 1024..=cap)
                                .logarithmic(true)
                                .text("MB"),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} MB / {} MB",
                                state.ram_mb, ram_full_mb
                            ))
                            .color(
                                if state.ram_mb > ram_full_mb / 2 {
                                    theme::ORANGE
                                } else {
                                    theme::TEXT_SECONDARY
                                },
                            ),
                        );
                    });

                    let mut clear_pending = false;
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // --- Performance ------------------------------------------
                    ui.horizontal(|ui| {
                        let mut enabled = state.perf.enabled;
                        if ui
                            .checkbox(&mut enabled, "⚡ Aethel Performance")
                            .on_hover_text(
                                "Installs the Fabric loader and the optimisation pack that \
                                 matches your renderer (VulkanMod for Vulkan, Sodium for \
                                 OpenGL), then tunes options.txt for your hardware.",
                            )
                            .changed()
                        {
                            state.perf.enabled = enabled;
                        }

                        if state.perf.enabled {
                            ui.add_space(theme::SPACE_3);
                            for preset in GraphicsPreset::all() {
                                let selected = state.perf.preset == preset;
                                if theme::pill(ui, preset.label(), selected).clicked() {
                                    state.perf.preset = preset;
                                }
                            }
                        }
                    });

                    if state.perf.enabled {
                        // Say exactly which pack will be installed: Sodium cannot run
                        // on a Vulkan renderer, so this is not cosmetic.
                        let renderer = state.renderer_mode.as_str().parse().unwrap_or_default();
                        theme::caption(
                            ui,
                            format!(
                                "{} — {} MB total RAM detected",
                                state.perf.preset.description(),
                                system_ram_mb()
                            ),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "Mod pack: {}",
                                perf_mods::pack_label(renderer)
                            ))
                            .size(11.0)
                            .color(theme::ACCENT_CYAN),
                        );
                    } else {
                        theme::caption(
                            ui,
                            "Off: launching vanilla with no mods and no options tuning.",
                        );
                    }

                    if let Some(instance) = &state.pending_instance {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Launching instance: {}",
                                    instance.name
                                ))
                                .size(12.0)
                                .color(theme::ACCENT_CYAN),
                            );
                            if ui.small_button("✕ clear").clicked() {
                                clear_pending = true;
                            }
                        });
                    }
                    if clear_pending {
                        state.pending_instance = None;
                    }
                });
            });

        ui.add_space(16.0);

        // --- Play / Stop --------------------------------------------------
        // Note: the shell builds the LaunchRequest on `HomeRequest::Launch` so
        // it can record the instance launch in the same place.
        let active = controller.is_active();
        let can_launch = !state.selected_version.is_empty() && !active;

        let button_text = if active { "■  STOP" } else { "▶  PLAY" };
        let response = theme::play_button(ui, button_text, active, can_launch || active);

        if active {
            if response.clicked() {
                controller.stop();
                controller.push_note("Stop requested…");
            }
        } else if response.clicked() {
            request = HomeRequest::Launch;
        }

        if !can_launch && !active {
            ui.label(
                egui::RichText::new("Pick a version above to enable Play.")
                    .size(12.0)
                    .color(theme::ORANGE),
            );
        }

        // --- Progress -----------------------------------------------------
        if active {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} — {}",
                    controller.status.label(),
                    controller.phase
                ))
                .color(theme::ACCENT_CYAN),
            );

            if let Some(fraction) = controller.progress_fraction() {
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(format!(
                            "{} {}/{}",
                            controller.progress_label,
                            controller.files_done,
                            controller.files_total
                        ))
                        .desired_width(360.0),
                );
            } else {
                ui.add(
                    egui::ProgressBar::new(0.0)
                        .animate(true)
                        .text(controller.phase.clone())
                        .desired_width(360.0),
                );
            }

            if let Some(started) = controller.started_at {
                ui.label(
                    egui::RichText::new(format!("Elapsed: {}s", started.elapsed().as_secs()))
                        .size(11.0)
                        .color(theme::BORDER),
                );
            }
        } else if controller.status != LaunchStatus::Idle {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("Last session: {}", controller.status.label()))
                    .size(12.0)
                    .color(theme::TEXT_SECONDARY),
            );
        }

        // --- Console ------------------------------------------------------
        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.show_console, "Show log");
            if !controller.console.is_empty() && ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(controller.console.join("\n"));
            }
        });

        if state.show_console {
            egui::ScrollArea::vertical()
                .id_salt("home_console")
                .max_height(220.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.style_mut().override_font_id = Some(egui::FontId::monospace(11.0));
                    for line in &controller.console {
                        ui.label(line);
                    }
                });
        }

        ui.add_space(16.0);

        // --- Stats row ----------------------------------------------------
        theme::glass_card(ui, |ui| {
            let stats: Vec<(&str, String)> = vec![
                ("Versions", state.available_versions.len().to_string()),
                ("Filtered", state.visible_versions.len().to_string()),
                ("RAM", format!("{} MB", state.ram_mb)),
                (
                    "Renderer",
                    state
                        .renderer_mode
                        .as_str()
                        .parse::<Renderer>()
                        .unwrap_or_default()
                        .resolved()
                        .as_str()
                        .to_string(),
                ),
                ("Status", controller.status.label()),
            ];
            ui.horizontal(|ui| {
                for (label, value) in &stats {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(value.as_str())
                                .size(15.0)
                                .color(theme::ACCENT_CYAN),
                        );
                        ui.label(
                            egui::RichText::new(*label)
                                .size(11.0)
                                .color(theme::TEXT_SECONDARY),
                        );
                    });
                    ui.add_space(18.0);
                }
            });
        });
    });

    request
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_are_distinguished_from_snapshots() {
        assert!(looks_like_release("1.21.4"));
        assert!(looks_like_release("26.2"));
        assert!(!looks_like_release("25w14a"));
        assert!(!looks_like_release("b1.7.3"));
    }

    #[test]
    fn folder_names_are_sanitised() {
        assert_eq!(sanitize_folder("1.21.4"), "1.21.4");
        assert_eq!(sanitize_folder("my instance/1"), "my-instance-1");
        assert_eq!(sanitize_folder(""), "default");
    }

    #[test]
    fn rebuild_visible_filters_releases_and_snapshots() {
        let mut state = AppState {
            available_versions: vec![
                "1.21.4".to_string(),
                "25w14a".to_string(),
                "1.20.1".to_string(),
            ],
            ..Default::default()
        };

        state.active_filter = QuickFilter::Release;
        state.rebuild_visible();
        assert_eq!(state.visible_versions, vec!["1.21.4", "1.20.1"]);

        state.active_filter = QuickFilter::Snapshot;
        state.rebuild_visible();
        assert_eq!(state.visible_versions, vec!["25w14a"]);

        state.active_filter = QuickFilter::Latest;
        state.rebuild_visible();
        assert_eq!(state.visible_versions, vec!["1.21.4"]);

        // Search narrows within the active filter.
        state.active_filter = QuickFilter::All;
        state.search_query = "1.20".to_string();
        state.rebuild_visible();
        assert_eq!(state.visible_versions, vec!["1.20.1"]);
    }

    #[test]
    fn game_dir_uses_instance_when_pending() {
        let mut state = AppState::default();
        state.env.instances_dir = PathBuf::from("/base/instances");
        state.selected_version = "1.21.4".to_string();
        assert_eq!(
            state.game_dir(),
            PathBuf::from("/base/instances/1.21.4/minecraft")
        );

        state.pending_instance = Some(PendingInstance {
            id: "abc-123".to_string(),
            name: "My World".to_string(),
            ram_mb: 2048,
            renderer: "opengl".to_string(),
        });
        assert_eq!(
            state.game_dir(),
            PathBuf::from("/base/instances/abc-123/minecraft")
        );
        let request = state.launch_request(&Account::Offline {
            name: "Steve".to_string(),
            uuid: launcher_core::auth::offline_uuid("Steve"),
        });
        assert_eq!(request.ram_mb, 2048);
        assert_eq!(request.renderer, "opengl");
    }
}
