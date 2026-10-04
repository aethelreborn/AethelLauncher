use super::super::branding;
use super::super::fonts;
use super::super::launch::{LaunchController, LaunchRequest, LaunchStatus};
use super::super::theme;
use super::settings::RendererMode;
use eframe::egui;
use launcher_core::auth::Account;
use launcher_core::perf::{mods as perf_mods, GraphicsPreset, PerfConfig, Renderer};
use launcher_core::{Instance, VersionCache};
use std::path::PathBuf;
use sysinfo::System;

pub fn system_ram_mb() -> u64 {
    use std::sync::OnceLock;
    static RAM: OnceLock<u64> = OnceLock::new();
    *RAM.get_or_init(|| {
        let mut sys = System::new();
        sys.refresh_memory();
        sys.total_memory() / 1024 / 1024
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuickFilter {
    All,
    Release,
    Snapshot,
    Latest,
}

pub fn ram_cap_mb() -> u64 {
    system_ram_mb().clamp(1024, 16384)
}

#[derive(Debug, Clone)]
pub struct LaunchEnv {
    pub shared_dir: PathBuf,
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
    pub pending_instance: Option<PendingInstance>,
    pub perf: PerfConfig,
    pub env: LaunchEnv,
    rx: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
}

#[derive(Debug, Clone)]
pub struct PendingInstance {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: String,
    pub ram_mb: u64,
    pub renderer: String,
    pub java_path: Option<String>,
}

pub fn pending_from(inst: &Instance) -> PendingInstance {
    PendingInstance {
        id: inst.id.clone(),
        name: inst.name.clone(),
        mc_version: inst.mc_version.clone(),
        loader: inst.loader.clone(),
        ram_mb: inst.ram_mb.max(1024),
        renderer: inst.renderer.clone(),
        java_path: inst.java_path.clone(),
    }
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
            perf: PerfConfig::recommended(),
            env: LaunchEnv::default(),
            rx: None,
        }
    }
}

impl AppState {
    pub fn new(cache: VersionCache) -> Self {
        let mut state = Self {
            cache: Some(cache),
            ..Default::default()
        };
        state.reload_versions();
        state
    }

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
            && self.pending_instance.is_none()
        {
            self.selected_version = self.visible_versions[0].clone();
        }
    }

    pub fn select_instance(&mut self, id: Option<&str>, instances: &[Instance]) {
        self.pending_instance = id
            .and_then(|id| instances.iter().find(|i| i.id == id))
            .map(pending_from);
        if let Some(instance) = &self.pending_instance {
            self.selected_version = instance.mc_version.clone();
        }
    }

    pub fn game_dir(&self) -> PathBuf {
        let folder = match &self.pending_instance {
            Some(instance) => sanitize_folder(&instance.id),
            None => sanitize_folder(&self.selected_version),
        };
        self.env.instances_dir.join(folder).join("minecraft")
    }

    pub fn launch_request(&self, auth: &Account) -> LaunchRequest {
        let (version_id, ram_mb, renderer, java_path) = match &self.pending_instance {
            Some(instance) => {
                let java = match &instance.java_path {
                    Some(path) if !path.trim().is_empty() => Some(PathBuf::from(path.trim())),
                    _ => self.env.java_path.clone(),
                };
                (
                    instance.mc_version.clone(),
                    instance.ram_mb.max(1024),
                    instance.renderer.clone(),
                    java,
                )
            }
            None => (
                self.selected_version.clone(),
                self.ram_mb,
                self.renderer_mode.as_str().to_string(),
                self.env.java_path.clone(),
            ),
        };

        LaunchRequest {
            version_id,
            game_dir: self.game_dir(),
            shared_dir: self.env.shared_dir.clone(),
            ram_mb,
            renderer,
            auth: auth.clone(),
            java_path,
            performance: self.perf,
        }
    }
}

fn looks_like_release(id: &str) -> bool {
    let mut parts = id.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    !first.is_empty() && first.chars().all(|c| c.is_ascii_digit())
}

pub fn sanitize_folder(name: &str) -> String {
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HomeRequest {
    #[default]
    None,
    Launch,
    InstancePicked(String),
    EditInstance(String),
    OpenLibrary,
}

pub fn home_panel(
    ui: &mut egui::Ui,
    state: &mut AppState,
    controller: &mut LaunchController,
    instances: &[Instance],
) -> HomeRequest {
    let mut request = HomeRequest::None;
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
        if theme::icon_only_button(
            ui,
            fonts::ICON_REFRESH,
            "Reload version list",
            theme::ButtonKind::Secondary,
            egui::vec2(30.0, 26.0),
            !state.fetching,
        )
        .clicked()
        {
            state.reload_versions();
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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

    let banner = controller
        .error
        .clone()
        .or_else(|| state.last_error.clone());
    if let Some(err) = banner {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("⚠ ").color(theme::DANGER_RED));
            ui.label(egui::RichText::new(&err).color(theme::DANGER_RED));
            if theme::icon_only_button(
                ui,
                fonts::ICON_CLOSE,
                "Dismiss",
                theme::ButtonKind::Ghost,
                egui::vec2(26.0, 22.0),
                true,
            )
            .clicked()
            {
                state.last_error = None;
                controller.error = None;
            }
        });
        ui.separator();
    }

    ui.vertical_centered(|ui| {
        egui::ScrollArea::vertical()
            .id_salt("home_config")
            .max_height(240.0)
            .show(ui, |ui| {
                theme::glass_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Instance").color(theme::TEXT_SECONDARY));
                        let enabled = !controller.is_active();
                        let before = state.pending_instance.as_ref().map(|p| p.id.clone());
                        let mut picked = before.clone();
                        ui.add_enabled_ui(enabled, |ui| {
                            egui::ComboBox::from_id_salt("home_instance")
                                .width(240.0)
                                .selected_text(match &state.pending_instance {
                                    Some(p) => format!("{} · {}", p.name, p.mc_version),
                                    None => "Not selected…".to_string(),
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut picked, None, "Not selected…");
                                    for inst in instances {
                                        ui.selectable_value(
                                            &mut picked,
                                            Some(inst.id.clone()),
                                            format!("{} · {}", inst.name, inst.mc_version),
                                        );
                                    }
                                });
                        });
                        if picked != before {
                            state.select_instance(picked.as_deref(), instances);
                            request = HomeRequest::InstancePicked(picked.unwrap_or_default());
                        }

                        if let Some(id) = state.pending_instance.as_ref().map(|p| p.id.clone()) {
                            if theme::icon_only_button(
                                ui,
                                fonts::ICON_EDIT,
                                "Edit this instance",
                                theme::ButtonKind::Secondary,
                                egui::vec2(32.0, 30.0),
                                enabled,
                            )
                            .clicked()
                            {
                                request = HomeRequest::EditInstance(id);
                            }
                        }
                        if theme::icon_only_button(
                            ui,
                            fonts::ICON_ADD,
                            "New instance",
                            theme::ButtonKind::Ghost,
                            egui::vec2(32.0, 30.0),
                            true,
                        )
                        .clicked()
                        {
                            request = HomeRequest::OpenLibrary;
                        }
                    });

                    ui.add_space(8.0);
                    if let Some(p) = &state.pending_instance {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&p.name)
                                    .size(15.0)
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            let java = match &p.java_path {
                                Some(j) if !j.trim().is_empty() => j.clone(),
                                _ => "auto Java".to_string(),
                            };
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · {} · {} MB · {} · {}",
                                    p.mc_version, p.loader, p.ram_mb, p.renderer, java
                                ))
                                .size(12.0)
                                .color(theme::TEXT_SECONDARY),
                            );
                        });
                        theme::caption(
                            ui,
                            "Play uses this instance's settings — edit them in Library.",
                        );
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(
                                    "No instance selected — create one in Library to enable Play.",
                                )
                                .size(12.0)
                                .color(theme::ORANGE),
                            );
                            if theme::icon_button(
                                ui,
                                fonts::ICON_ADD,
                                "New instance",
                                theme::ButtonKind::Secondary,
                                !controller.is_active(),
                            )
                            .clicked()
                            {
                                request = HomeRequest::OpenLibrary;
                            }
                        });
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        let mut enabled = state.perf.enabled;
                        if ui
                            .checkbox(&mut enabled, "Aethel Performance")
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
                        let renderer = Renderer::parse(state.renderer_mode.as_str());
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
                });
            });

        ui.add_space(16.0);

        let active = controller.is_active();
        let can_launch = state.pending_instance.is_some() && !active;

        let button_label = if active {
            theme::icon_label(fonts::ICON_STOP, "Stop", 26.0, 18.0)
        } else {
            theme::icon_label(fonts::ICON_PLAY, "Play", 26.0, 18.0)
        };
        let response = theme::play_button(ui, button_label, active, can_launch || active);

        if active {
            if response.clicked() {
                controller.stop();
                controller.push_note("Stop requested…");
            }
        } else if response.clicked() {
            request = HomeRequest::Launch;
        }

        if let Some(pending) = &state.pending_instance {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("{}  ·  {}", pending.name, pending.mc_version))
                    .size(13.0)
                    .color(theme::TEXT),
            );
        } else if !active {
            ui.add_space(6.0);
            let hint = if instances.is_empty() {
                "No instances yet — create one in Library to enable Play."
            } else {
                "Select an instance above to enable Play."
            };
            ui.label(egui::RichText::new(hint).size(12.0).color(theme::ORANGE));
        }

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
                let mut line = format!("Elapsed: {}s", started.elapsed().as_secs());
                if controller.in_game {
                    if let Some(fps) = controller.game_fps {
                        line.push_str(&format!(" · {fps:.0} FPS"));
                    }
                }
                ui.label(egui::RichText::new(line).size(11.0).color(theme::BORDER));
            }
        } else if controller.status != LaunchStatus::Idle {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("Last session: {}", controller.status.label()))
                    .size(12.0)
                    .color(theme::TEXT_SECONDARY),
            );
        }

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

        theme::glass_card(ui, |ui| {
            let (instance, version, ram, renderer) = match &state.pending_instance {
                Some(p) => (
                    p.name.clone(),
                    p.mc_version.clone(),
                    format!("{} MB", p.ram_mb),
                    p.renderer.clone(),
                ),
                None => (
                    "None".to_string(),
                    if state.selected_version.is_empty() {
                        "—".to_string()
                    } else {
                        state.selected_version.clone()
                    },
                    format!("{} MB", state.ram_mb),
                    Renderer::parse(state.renderer_mode.as_str())
                        .resolved()
                        .as_str()
                        .to_string(),
                ),
            };
            let stats: Vec<(&str, String)> = vec![
                ("Instance", instance),
                ("Version", version),
                ("RAM", ram),
                ("Renderer", renderer),
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
            mc_version: "1.21.4".to_string(),
            loader: "vanilla".to_string(),
            ram_mb: 2048,
            renderer: "opengl".to_string(),
            java_path: None,
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
        assert_eq!(request.version_id, "1.21.4");
    }

    fn sample_instance(id: &str, name: &str, version: &str) -> Instance {
        Instance {
            id: id.to_string(),
            name: name.to_string(),
            mc_version: version.to_string(),
            loader: "fabric".to_string(),
            loader_version: "0.16.9".to_string(),
            ram_mb: 4096,
            java_path: Some("/opt/jdk/bin/java".to_string()),
            renderer: "vulkan".to_string(),
            auth_mode: "offline".to_string(),
            created_at: "2026-01-01T00:00:00+00:00".to_string(),
            last_launched: None,
        }
    }

    #[test]
    fn select_instance_drives_pending_and_version() {
        let instances = vec![sample_instance("a1", "Alpha", "1.21.4")];
        let mut state = AppState {
            selected_version: "1.20.1".to_string(),
            ..Default::default()
        };

        state.select_instance(Some("a1"), &instances);
        let pending = state.pending_instance.as_ref().expect("selected");
        assert_eq!(pending.name, "Alpha");
        assert_eq!(pending.mc_version, "1.21.4");
        assert_eq!(state.selected_version, "1.21.4");

        state.select_instance(None, &instances);
        assert!(state.pending_instance.is_none());
        state.select_instance(Some("missing"), &instances);
        assert!(state.pending_instance.is_none());
    }

    #[test]
    fn launch_request_prefers_instance_version_and_java() {
        let instances = vec![sample_instance("a1", "Alpha", "1.21.4")];
        let mut state = AppState {
            selected_version: "1.20.1".to_string(),
            ..Default::default()
        };
        state.select_instance(Some("a1"), &instances);

        let request = state.launch_request(&Account::Offline {
            name: "Steve".to_string(),
            uuid: launcher_core::auth::offline_uuid("Steve"),
        });
        assert_eq!(request.version_id, "1.21.4");
        assert_eq!(request.ram_mb, 4096);
        assert_eq!(request.renderer, "vulkan");
        assert_eq!(request.java_path, Some(PathBuf::from("/opt/jdk/bin/java")));
    }
}
