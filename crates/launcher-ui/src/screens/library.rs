use super::super::{fonts, theme};
use eframe::egui;
use launcher_core::{Instance, InstanceStore};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum LibraryAction {
    Play(Instance),
    Create(String, String),
    Saved(Instance),
    OpenMods(Instance),
}

#[derive(Default)]
pub struct LibraryState {
    pub instances: Vec<Instance>,
    db_path: PathBuf,
    pub instances_dir: PathBuf,
    store: Option<InstanceStore>,
    pub error: Option<String>,
    pub creating: bool,
    pub new_name: String,
    pub new_version: String,
    pub available_versions: Vec<String>,
    pub search: String,
    pub pending_delete: Option<String>,
    pub editing: Option<Instance>,
    pub edit_name: String,
    pub edit_version: String,
    pub edit_ram_mb: u64,
    pub edit_java: String,
    pub edit_renderer: String,
    pub edit_notice: Option<String>,
}

impl std::fmt::Debug for LibraryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryState")
            .field("instances", &self.instances.len())
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl LibraryState {
    pub fn new(db_path: PathBuf, instances_dir: PathBuf) -> Self {
        Self {
            db_path,
            instances_dir,
            ..Default::default()
        }
    }

    fn store(&mut self) -> Result<&mut InstanceStore, String> {
        if self.store.is_none() {
            match InstanceStore::open(&self.db_path) {
                Ok(s) => self.store = Some(s),
                Err(e) => return Err(format!("Failed to open instance database: {e}")),
            }
        }
        self.store
            .as_mut()
            .ok_or_else(|| "instance database unavailable".to_string())
    }

    pub fn poll_background(&mut self) {}

    pub fn load_from_disk(&mut self) {
        let list = self.store().and_then(|store| {
            store
                .list()
                .map_err(|e| format!("Failed to load instances: {e}"))
        });
        match list {
            Ok(list) => self.instances = list,
            Err(e) => self.error = Some(e),
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
        let result = self.store().and_then(|store| {
            store
                .create(&instance)
                .map_err(|e| format!("Failed to create instance: {e}"))
        });
        match result {
            Ok(()) => {
                self.instances.push(instance);
                self.creating = false;
                self.new_name.clear();
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn mark_launched(&mut self, id: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) else {
            return;
        };
        instance.last_launched = Some(now);
        let updated = instance.clone();

        let result = self.store().and_then(|store| {
            store
                .create(&updated)
                .map_err(|e| format!("Failed to update instance: {e}"))
        });
        if let Err(e) = result {
            self.error = Some(e);
        }

        self.instances
            .sort_by(|a, b| b.last_launched.cmp(&a.last_launched));
    }

    pub fn delete_instance(&mut self, id: &str) {
        let result = self.store().and_then(|store| {
            store
                .delete(id)
                .map(|_| ())
                .map_err(|e| format!("Failed to delete instance: {e}"))
        });
        match result {
            Ok(()) => {
                self.instances.retain(|i| i.id != id);
                if self.editing.as_ref().map(|e| e.id == id) == Some(true) {
                    self.editing = None;
                }
                self.pending_delete = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn begin_edit(&mut self, id: &str) {
        let Some(inst) = self.instances.iter().find(|i| i.id == id).cloned() else {
            return;
        };
        self.edit_name = inst.name.clone();
        self.edit_version = inst.mc_version.clone();
        self.edit_ram_mb = inst.ram_mb;
        self.edit_java = inst.java_path.clone().unwrap_or_default();
        self.edit_renderer = inst.renderer.clone();
        self.edit_notice = None;
        self.pending_delete = None;
        self.editing = Some(inst);
    }

    pub fn save_edited(&mut self) -> Option<Instance> {
        let base = self.editing.clone()?;
        if self.edit_name.trim().is_empty() || self.edit_version.trim().is_empty() {
            self.error = Some("Instance needs a name and a Minecraft version.".to_string());
            return None;
        }
        let java = self.edit_java.trim();
        let inst = Instance {
            name: self.edit_name.trim().to_string(),
            mc_version: self.edit_version.trim().to_string(),
            ram_mb: self.edit_ram_mb.max(1024),
            java_path: if java.is_empty() {
                None
            } else {
                Some(java.to_string())
            },
            renderer: self.edit_renderer.clone(),
            ..base
        };
        let result = self.store().and_then(|store| {
            store
                .create(&inst)
                .map_err(|e| format!("Failed to save instance: {e}"))
        });
        match result {
            Ok(()) => {
                if let Some(slot) = self.instances.iter_mut().find(|i| i.id == inst.id) {
                    *slot = inst.clone();
                }
                self.editing = None;
                Some(inst)
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    pub fn game_dir_for(&self, id: &str) -> PathBuf {
        self.instances_dir
            .join(super::home::sanitize_folder(id))
            .join("minecraft")
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
        if theme::icon_button(
            ui,
            fonts::ICON_ADD,
            "New instance",
            theme::ButtonKind::Primary,
            true,
        )
        .clicked()
        {
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                    state.error.take();
                }
            });
        });
        ui.separator();
    }

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

    if state.editing.is_some() {
        let base_id = state
            .editing
            .as_ref()
            .map(|i| i.id.clone())
            .unwrap_or_default();
        let game_dir = state.game_dir_for(&base_id);
        let mods_dir = game_dir.join("mods");
        let packs_dir = game_dir.join("resourcepacks");

        let mut edit_done: Option<&str> = None;

        egui::Window::new("Edit instance")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(egui::TextEdit::singleline(&mut state.edit_name).desired_width(200.0));
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Version:");
                    let mut versions = state.available_versions.clone();
                    if !versions.contains(&state.edit_version) && !state.edit_version.is_empty() {
                        versions.insert(0, state.edit_version.clone());
                    }
                    egui::ComboBox::from_id_salt("edit_version")
                        .width(160.0)
                        .selected_text(&state.edit_version)
                        .show_ui(ui, |ui| {
                            if versions.is_empty() {
                                ui.label("(fetch versions from Home first)");
                            }
                            for v in &versions {
                                ui.selectable_value(&mut state.edit_version, v.clone(), v.clone());
                            }
                        });
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Memory:");
                    let cap = super::home::ram_cap_mb();
                    if state.edit_ram_mb > cap {
                        state.edit_ram_mb = cap;
                    }
                    ui.add(
                        egui::Slider::new(&mut state.edit_ram_mb, 1024..=cap)
                            .logarithmic(true)
                            .text("MB"),
                    );
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Renderer:");
                    egui::ComboBox::from_id_salt("edit_renderer")
                        .width(140.0)
                        .selected_text(&state.edit_renderer)
                        .show_ui(ui, |ui| {
                            for v in ["auto", "opengl", "vulkan"] {
                                ui.selectable_value(&mut state.edit_renderer, v.to_string(), v);
                            }
                        });
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Java:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.edit_java)
                            .hint_text("bundled (auto)")
                            .desired_width(220.0),
                    );
                    if theme::icon_button(
                        ui,
                        fonts::ICON_FOLDER_OPEN,
                        "Browse…",
                        theme::ButtonKind::Secondary,
                        true,
                    )
                    .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            state.edit_java = path.to_string_lossy().to_string();
                        }
                    }
                    if !state.edit_java.is_empty()
                        && theme::icon_button(
                            ui,
                            fonts::ICON_CLOSE,
                            "Auto",
                            theme::ButtonKind::Ghost,
                            true,
                        )
                        .clicked()
                    {
                        state.edit_java.clear();
                    }
                });

                ui.add_space(6.0);
                theme::caption(
                    ui,
                    format!(
                        "Loader: {} {}",
                        state
                            .editing
                            .as_ref()
                            .map(|i| i.loader.clone())
                            .unwrap_or_default(),
                        state
                            .editing
                            .as_ref()
                            .map(|i| i.loader_version.clone())
                            .unwrap_or_default()
                    ),
                );

                ui.add_space(8.0);
                ui.separator();
                ui.label(
                    egui::RichText::new("Content")
                        .size(13.0)
                        .strong()
                        .color(theme::TEXT_SECONDARY),
                );
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Mods").color(theme::TEXT));
                    if theme::icon_button(
                        ui,
                        fonts::ICON_UPLOAD,
                        "Add mod…",
                        theme::ButtonKind::Secondary,
                        true,
                    )
                    .clicked()
                    {
                        let picked = rfd::FileDialog::new()
                            .add_filter("JAR files", &["jar"])
                            .pick_files();
                        if let Some(paths) = picked {
                            state.edit_notice = match super::mods::copy_into(&mods_dir, &paths) {
                                Ok(n) => Some(format!("Added {n} mod file(s).")),
                                Err(e) => Some(format!("⚠ {e}")),
                            };
                        }
                    }
                    if theme::icon_button(
                        ui,
                        fonts::ICON_FOLDER_OPEN,
                        "Open in Mods screen",
                        theme::ButtonKind::Ghost,
                        true,
                    )
                    .clicked()
                    {
                        edit_done = Some("open_mods");
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Resource packs").color(theme::TEXT));
                    if theme::icon_button(
                        ui,
                        fonts::ICON_UPLOAD,
                        "Add pack…",
                        theme::ButtonKind::Secondary,
                        true,
                    )
                    .clicked()
                    {
                        let picked = rfd::FileDialog::new()
                            .add_filter("Zip files", &["zip"])
                            .pick_files();
                        if let Some(paths) = picked {
                            state.edit_notice = match super::mods::copy_into(&packs_dir, &paths) {
                                Ok(n) => Some(format!("Added {n} resource pack(s).")),
                                Err(e) => Some(format!("⚠ {e}")),
                            };
                        }
                    }
                });
                theme::caption(ui, format!("Mods folder: {}", mods_dir.display()));
                theme::caption(ui, format!("Resource packs: {}", packs_dir.display()));

                if let Some(notice) = &state.edit_notice {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(notice).size(12.0).color(
                        if notice.starts_with('⚠') {
                            theme::ORANGE
                        } else {
                            theme::SUCCESS
                        },
                    ));
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let can_save = {
                        let name = !state.edit_name.trim().is_empty();
                        let version = !state.edit_version.trim().is_empty();
                        name && version
                    };
                    if ui.button("Cancel").clicked() {
                        edit_done = Some("cancel");
                    }
                    ui.add_space(8.0);
                    if theme::icon_button(
                        ui,
                        fonts::ICON_CHECK,
                        "Save",
                        theme::ButtonKind::Primary,
                        can_save,
                    )
                    .clicked()
                    {
                        edit_done = Some("save");
                    }
                });
            });

        match edit_done {
            Some("save") => {
                if let Some(inst) = state.save_edited() {
                    action = Some(LibraryAction::Saved(inst));
                }
            }
            Some("cancel") => state.editing = None,
            Some("open_mods") => {
                let inst = state
                    .instances
                    .iter()
                    .find(|i| i.id == base_id)
                    .cloned()
                    .or_else(|| state.editing.clone());
                if let Some(inst) = inst {
                    action = Some(LibraryAction::OpenMods(inst));
                }
                state.editing = None;
            }
            _ => {}
        }
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
                    egui::RichText::new("Press New instance to create one.")
                        .size(13.0)
                        .color(theme::BORDER),
                );
            });
        });
    } else {
        for inst in &filtered {
            let armed = state.pending_delete.as_deref() == Some(inst.id.as_str());
            theme::glass_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&inst.name).size(16.0).color(if armed {
                        theme::DANGER
                    } else {
                        theme::TEXT_PRIMARY
                    }));
                    ui.label(
                        egui::RichText::new(format!("{} • {}", inst.mc_version, inst.loader))
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let tip = if armed {
                            "Click again to delete"
                        } else {
                            "Delete instance"
                        };
                        if theme::icon_only_button(
                            ui,
                            fonts::ICON_DELETE,
                            tip,
                            if armed {
                                theme::ButtonKind::Danger
                            } else {
                                theme::ButtonKind::Secondary
                            },
                            egui::vec2(30.0, 26.0),
                            true,
                        )
                        .clicked()
                        {
                            if armed {
                                state.delete_instance(&inst.id);
                            } else {
                                state.pending_delete = Some(inst.id.clone());
                            }
                        }
                        if theme::icon_only_button(
                            ui,
                            fonts::ICON_EDIT,
                            "Edit instance",
                            theme::ButtonKind::Secondary,
                            egui::vec2(30.0, 26.0),
                            true,
                        )
                        .clicked()
                        {
                            state.begin_edit(&inst.id);
                        }
                        if theme::icon_only_button(
                            ui,
                            fonts::ICON_PLAY,
                            "Play",
                            theme::ButtonKind::Primary,
                            egui::vec2(36.0, 26.0),
                            true,
                        )
                        .clicked()
                        {
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
