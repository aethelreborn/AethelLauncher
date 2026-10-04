use super::super::theme;
use eframe::egui;
use launcher_core::telemetry::crash::{parse_crash_report, CrashReport};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CrashEntry {
    pub path: PathBuf,
    pub file_name: String,
    pub instance: String,
    pub modified: Option<std::time::SystemTime>,
}

#[derive(Debug, Default)]
pub struct CrashViewerState {
    pub entries: Vec<CrashEntry>,
    pub selected: Option<usize>,
    raw: Option<String>,
    parsed: Option<CrashReport>,
    pub error: Option<String>,
}

impl CrashViewerState {
    pub fn scan(&mut self, instances_dir: &Path) {
        let mut found = Vec::new();
        if let Ok(instances) = std::fs::read_dir(instances_dir) {
            for instance in instances.flatten() {
                let crash_dir = instance.path().join("crash-reports");
                let Ok(files) = std::fs::read_dir(&crash_dir) else {
                    continue;
                };
                for file in files.flatten() {
                    let name = file.file_name().to_string_lossy().to_string();
                    if !(name.starts_with("crash-") && name.ends_with(".txt")) {
                        continue;
                    }
                    found.push(CrashEntry {
                        modified: file.metadata().ok().and_then(|m| m.modified().ok()),
                        path: file.path(),
                        file_name: name,
                        instance: instance.file_name().to_string_lossy().to_string(),
                    });
                }
            }
        }
        found.sort_by_key(|entry| std::cmp::Reverse(entry.modified));

        let selection_held = self.selected.is_some_and(|i| i < found.len());
        self.entries = found;
        if !selection_held {
            self.selected = if self.entries.is_empty() {
                None
            } else {
                Some(0)
            };
            self.load_selected();
        }
    }

    pub fn load_selected(&mut self) {
        self.raw = None;
        self.parsed = None;
        self.error = None;
        let Some(index) = self.selected else { return };
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        match std::fs::read_to_string(&entry.path) {
            Ok(text) => {
                self.parsed = parse_crash_report(&entry.path).ok();
                self.raw = Some(text);
            }
            Err(e) => self.error = Some(format!("Could not read {}: {e}", entry.file_name)),
        }
    }
}

pub fn crash_viewer_panel(ui: &mut egui::Ui, state: &mut CrashViewerState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Crashes")
                .size(24.0)
                .strong()
                .color(theme::TEXT),
        );
        ui.label(
            egui::RichText::new(format!("{} report(s)", state.entries.len()))
                .size(12.0)
                .color(theme::TEXT_DIM),
        );
    });
    ui.separator();

    if state.entries.is_empty() {
        theme::empty_state(
            ui,
            "No crash reports",
            "Reports written by failing instances will show up here.",
        );
        return;
    }

    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(theme::BG_RAISE)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.set_width(300.0);
                ui.set_min_height(ui.available_height());
                let mut clicked = None;
                for (index, entry) in state.entries.iter().enumerate() {
                    let selected = state.selected == Some(index);
                    let label =
                        egui::RichText::new(format!("{}\n{}", entry.instance, entry.file_name))
                            .size(12.0)
                            .color(if selected {
                                theme::TEXT
                            } else {
                                theme::TEXT_DIM
                            });
                    if ui
                        .add(egui::SelectableLabel::new(selected, label))
                        .clicked()
                    {
                        clicked = Some(index);
                    }
                }
                if let Some(index) = clicked {
                    state.selected = Some(index);
                    state.load_selected();
                }
            });
        ui.add_space(theme::SPACE_3);

        egui::Frame::new()
            .fill(theme::BG_RAISE)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_min_width(240.0);
                if let Some(error) = &state.error {
                    ui.colored_label(theme::DANGER, error);
                    return;
                }
                if let Some(parsed) = &state.parsed {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Minecraft ")
                                .size(12.0)
                                .color(theme::TEXT_DIM),
                        );
                        ui.label(
                            egui::RichText::new(&parsed.mc_version)
                                .size(12.0)
                                .color(theme::TEXT),
                        );
                        ui.label(
                            egui::RichText::new(format!("  launcher {}", parsed.bundle_version))
                                .size(12.0)
                                .color(theme::TEXT_DIM),
                        );
                    });
                    ui.add_space(theme::SPACE_2);
                }
                let raw = state.raw.as_deref().unwrap_or("(empty report)");
                egui::ScrollArea::vertical()
                    .id_salt("crash_raw")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(raw)
                                    .size(11.0)
                                    .monospace()
                                    .color(theme::TEXT_DIM),
                            )
                            .wrap(),
                        );
                    });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aethel-crash-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_with_mtime(path: &Path, contents: &str, mtime: std::time::SystemTime) {
        std::fs::write(path, contents).unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(mtime).unwrap();
    }

    fn days_after_epoch(days: u64) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(days * 86_400)
    }

    #[test]
    fn scan_finds_reports_across_instances_and_sorts_newest_first() {
        let root = temp_dir("scan");
        let a = root.join("alpha").join("crash-reports");
        let b = root.join("beta").join("crash-reports");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        write_with_mtime(
            &a.join("crash-2026-09-01_10.00.00-client.txt"),
            "old",
            days_after_epoch(20_676),
        );
        write_with_mtime(
            &a.join("crash-2026-09-30_10.00.00-client.txt"),
            "new",
            days_after_epoch(20_705),
        );
        write_with_mtime(
            &b.join("crash-2026-09-15_10.00.00-server.txt"),
            "mid",
            days_after_epoch(20_690),
        );
        std::fs::write(a.join("crash-2026-01-01-client.log"), "nope").unwrap();
        std::fs::write(a.join("latest.log"), "nope").unwrap();

        let mut state = CrashViewerState::default();
        state.scan(&root);
        assert_eq!(state.entries.len(), 3);
        assert_eq!(state.selected, Some(0));
        assert!(state.entries[0].file_name.contains("2026-09-30"));
        let instances: Vec<&str> = state.entries.iter().map(|e| e.instance.as_str()).collect();
        assert_eq!(instances, vec!["alpha", "beta", "alpha"]);
        assert!(state.raw.is_some(), "first report auto-loaded");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_on_missing_dir_is_empty_and_keeps_no_selection() {
        let root = std::env::temp_dir().join("aethel-crash-test-missing");
        let _ = std::fs::remove_dir_all(&root);

        let mut state = CrashViewerState::default();
        state.scan(&root);
        assert!(state.entries.is_empty());
        assert_eq!(state.selected, None);
        assert!(state.raw.is_none());
    }

    #[test]
    fn scan_loads_selected_content() {
        let root = temp_dir("content");
        let dir = root.join("gamma").join("crash-reports");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("crash-2026-09-30-client.txt"),
            "---- Minecraft Crash Report ----",
        )
        .unwrap();

        let mut state = CrashViewerState::default();
        state.scan(&root);
        assert_eq!(state.selected, Some(0));
        assert_eq!(
            state.raw.as_deref(),
            Some("---- Minecraft Crash Report ----")
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
