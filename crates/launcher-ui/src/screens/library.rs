//! Library screen — instance grid with create/play/delete.

use super::super::theme;
use eframe::egui;
use launcher_core::{Instance, InstanceStore};
use std::path::PathBuf;
use uuid::Uuid;

/// Action a single library panel frame wants the app to perform.
#[derive(Debug, Clone)]
pub enum LibraryAction {
    /// Launch an instance: jump to Home preloaded with its version + RAM.
    Play(Instance),
    Create(String, String),
}

#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    pub instances: Vec<Instance>,
    db_path: PathBuf,
    pub error: Option<String>,
    pub creating: bool,
    pub new_name: String,
    pub new_version: String,
    pub available_versions: Vec<String>,
    pub search: String,
}

impl LibraryState {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            ..Default::default()
        }
    }

    /// No-op, kept for the per-frame pump in `lib.rs`.
    pub fn poll_background(&mut self) {}

    pub fn load_from_disk(&mut self) {
        let store = match InstanceStore::open(&self.db_path) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(format!("Failed to open instance database: {}", e));
                return;
            }
        };
        match store.list() {
            Ok(list) => self.instances = list,
            Err(e) => self.error = Some(format!("Failed to load instances: {}", e)),
        }
    }

    pub fn create_instance(&mut self, name: String, version: String) {
        if name.trim().is_empty() || version.trim().is_empty() {
            self.error = Some("Instance needs a name and a Minecraft version.".to_string());
            return;
        }
        let instance = Instance {
            id: Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            mc_version: version.trim().to_string(),
            loader: "vanilla".to_string(),
            loader_version: String::new(),
            ram_mb: super::home::ram_cap_mb() * 3 / 4,
            java_path: None,
            renderer: "auto".to_string(),
            auth_mode: "offline".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            last_launched: None,
        };
        let mut store = match InstanceStore::open(&self.db_path) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(format!("Failed to open instance database: {}", e));
                return;
            }
        };
        if let Err(e) = store.create(&instance) {
            self.error = Some(format!("Failed to create instance: {}", e));
            return;
        }
        self.instances.push(instance);
        self.creating = false;
        self.new_name.clear();
    }

    /// Stamp `last_launched` and persist, so the Library stays ordered by
    /// most-recently-played.
    pub fn mark_launched(&mut self, id: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) else {
            return;
        };
        instance.last_launched = Some(now);
        let updated = instance.clone();

        match InstanceStore::open(&self.db_path) {
            Ok(mut store) => {
                if let Err(e) = store.create(&updated) {
                    self.error = Some(format!("Failed to update instance: {}", e));
                }
            }
            Err(e) => self.error = Some(format!("Failed to open instance database: {}", e)),
        }

        // Most-recently-played first; never-launched instances last.
        self.instances
            .sort_by(|a, b| b.last_launched.cmp(&a.last_launched));
    }

    pub fn delete_instance(&mut self, id: &str) {
        let mut store = match InstanceStore::open(&self.db_path) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(format!("Failed to open instance database: {}", e));
                return;
            }
        };
        match store.delete(id) {
            Ok(_) => self.instances.retain(|i| i.id != id),
            Err(e) => self.error = Some(format!("Failed to delete instance: {}", e)),
        }
    }
}

pub fn library_panel(ui: &mut egui::Ui, state: &mut LibraryState) -> Option<LibraryAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Library")
                .size(24.0)
                .color(theme::ACCENT_CYAN),
        );
        ui.add_space(12.0);
        let create_btn = egui::Button::new("+ New Instance")
            .fill(theme::ACCENT_VIOLET)
            .min_size(egui::vec2(120.0, 28.0));
        if ui.add(create_btn).clicked() {
            state.creating = !state.creating;
        }
        ui.add_space(24.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("Search instances...")
                .desired_width(180.0),
        );
    });
    ui.separator();

    let error_clone = state.error.clone();
    if let Some(err) = error_clone {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("⚠ ").color(theme::DANGER_RED));
            ui.label(egui::RichText::new(&err).color(theme::DANGER_RED));
            if ui.small_button("✕").clicked() {
                state.error.take();
            }
        });
        ui.separator();
    }

    // New instance dialog
    if state.creating {
        egui::Window::new("New Instance")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_name)
                            .hint_text("My World")
                            .desired_width(180.0),
                    );
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Version:");
                    egui::ComboBox::from_label("mc_version")
                        .width(180.0)
                        .selected_text(if state.new_version.is_empty() {
                            "Select..."
                        } else {
                            &state.new_version
                        })
                        .show_ui(ui, |ui| {
                            if state.available_versions.is_empty() {
                                ui.label("(fetch versions from Home first)");
                            }
                            for v in &state.available_versions {
                                ui.selectable_value(&mut state.new_version, v.clone(), v.clone());
                            }
                        });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        state.creating = false;
                    }
                    if ui
                        .button(egui::RichText::new("Create").color(theme::SUCCESS_GREEN))
                        .clicked()
                    {
                        let name = state.new_name.clone();
                        let version = state.new_version.clone();
                        action = Some(LibraryAction::Create(name, version));
                    }
                });
            });
    }

    let query = state.search.trim().to_lowercase();
    let filtered: Vec<Instance> = state
        .instances
        .iter()
        .filter(|i| query.is_empty() || i.name.to_lowercase().contains(&query))
        .cloned()
        .collect();

    if filtered.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("No instances.")
                        .size(20.0)
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Click \"+ New Instance\" to create one.")
                        .size(13.0)
                        .color(theme::BORDER),
                );
            });
        });
    } else {
        for inst in &filtered {
            theme::glass_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&inst.name)
                            .size(16.0)
                            .color(theme::TEXT_PRIMARY),
                    );
                    ui.label(
                        egui::RichText::new(format!("{} • {}", inst.mc_version, inst.loader))
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let del = egui::Button::new("✕")
                            .fill(theme::SURFACE_LIGHT)
                            .min_size(egui::vec2(28.0, 24.0));
                        if ui.add(del).clicked() {
                            state.delete_instance(&inst.id);
                        }
                        let play = egui::Button::new("▶")
                            .fill(theme::ACCENT_VIOLET)
                            .min_size(egui::vec2(36.0, 24.0));
                        if ui.add(play).clicked() {
                            action = Some(LibraryAction::Play(inst.clone()));
                        }
                    });
                });
                if let Some(last) = &inst.last_launched {
                    ui.label(
                        egui::RichText::new(format!("Last played: {}", last))
                            .size(11.0)
                            .color(theme::BORDER),
                    );
                }
            });
            ui.add_space(6.0);
        }
    }

    action
}
