//! Mods screen — lists the jars actually present in the instance's `mods/`.

use super::super::theme;
use eframe::egui;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct ModFile {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ModsState {
    /// Folder being browsed. Empty until seeded with the instance's mods dir.
    pub path: String,
    pub mods: Vec<ModFile>,
    pub error: Option<String>,
    /// Whether `mods` reflects `path` yet.
    loaded: bool,
}

impl ModsState {
    /// Point the screen at a folder, reloading if it changed.
    pub fn point_at(&mut self, dir: &Path) {
        let display = dir.to_string_lossy().to_string();
        if self.path != display {
            self.path = display;
            self.loaded = false;
        }
        if !self.loaded {
            self.refresh();
        }
    }

    pub fn refresh(&mut self) {
        self.loaded = true;
        self.mods.clear();
        self.error = None;

        let dir = PathBuf::from(self.path.trim());
        if !dir.is_dir() {
            self.error = Some(format!("{} does not exist yet.", dir.display()));
            return;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                self.error = Some(format!("Could not read {}: {e}", dir.display()));
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            // A directory can legitimately be called `something.jar`, so check
            // the file type rather than trusting the extension.
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("jar") {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            self.mods.push(ModFile { name, size });
        }
        self.mods
            .sort_by_key(|mod_file| mod_file.name.to_lowercase());
    }
}

pub fn mods_panel(
    ui: &mut egui::Ui,
    state: &mut ModsState,
    default_mods_dir: &Path,
    game_dir: &Path,
) {
    // Follow the selected instance unless the user typed a custom folder.
    if state.path.is_empty() {
        state.point_at(default_mods_dir);
    }

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Mods")
                .size(24.0)
                .color(theme::ACCENT_CYAN),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!("{} jar(s)", state.mods.len()))
                .size(12.0)
                .color(theme::TEXT_SECONDARY),
        );
        if ui.small_button("⟳ Refresh").clicked() {
            state.refresh();
        }
    });
    ui.separator();
    ui.add_space(8.0);

    theme::glass_card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Folder").color(theme::TEXT_SECONDARY));
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.path)
                    .desired_width(420.0)
                    .hint_text("path to a mods folder"),
            );
            if response.changed() {
                state.loaded = false;
            }
            if ui.small_button("Go").clicked() {
                state.refresh();
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("Game directory: {}", game_dir.display()))
                .size(11.0)
                .color(theme::BORDER),
        );
        ui.label(
            egui::RichText::new(
                "Aethel's managed performance mods live here too — anything you drop in is kept.",
            )
            .size(11.0)
            .color(theme::BORDER),
        );
    });

    ui.add_space(12.0);

    if let Some(err) = &state.error {
        ui.label(egui::RichText::new(format!("⚠ {err}")).color(theme::ORANGE));
        ui.add_space(8.0);
    }

    if state.mods.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("No mods installed.")
                        .size(18.0)
                        .color(theme::TEXT_SECONDARY),
                );
                ui.label(
                    egui::RichText::new(
                        "Drop .jar files into the folder above, then press Refresh.",
                    )
                    .size(12.0)
                    .color(theme::BORDER),
                );
            });
        });
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("mods_list")
        .show(ui, |ui| {
            for m in &state.mods {
                theme::glass_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&m.name)
                                .size(14.0)
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(human_size(m.size))
                                    .size(11.0)
                                    .color(theme::TEXT_SECONDARY),
                            );
                        });
                    });
                });
                ui.add_space(4.0);
            }
        });
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_lists_only_jars() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sodium.jar"), b"jar").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"text").unwrap();
        std::fs::create_dir(dir.path().join("sub.jar")).unwrap();

        let mut state = ModsState {
            path: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        state.refresh();

        assert_eq!(state.mods.len(), 1);
        assert_eq!(state.mods[0].name, "sodium.jar");
        assert!(state.error.is_none());
    }

    #[test]
    fn missing_folder_reports_an_error() {
        let mut state = ModsState {
            path: "/definitely/not/a/dir".to_string(),
            ..Default::default()
        };
        state.refresh();
        assert!(state.mods.is_empty());
        assert!(state.error.is_some());
    }

    #[test]
    fn human_size_formats_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }
}
