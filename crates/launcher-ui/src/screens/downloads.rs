//! Downloads screen — live install progress for the current launch.

use super::super::launch::{LaunchController, LaunchStatus};
use super::super::theme;
use eframe::egui;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct DownloadsState {
    /// Show every console line, not just stages and errors.
    pub show_all_lines: bool,
}

pub fn downloads_panel(
    ui: &mut egui::Ui,
    state: &mut DownloadsState,
    controller: &LaunchController,
    shared_dir: &Path,
) {
    ui.label(
        egui::RichText::new("Downloads")
            .size(24.0)
            .color(theme::ACCENT_CYAN),
    );
    ui.separator();
    ui.add_space(8.0);

    theme::glass_card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Status").color(theme::TEXT_SECONDARY));
            ui.label(egui::RichText::new(controller.status.label()).color(
                match controller.status {
                    LaunchStatus::Failed => theme::DANGER_RED,
                    LaunchStatus::Running => theme::SUCCESS_GREEN,
                    LaunchStatus::Preparing => theme::ACCENT_CYAN,
                    _ => theme::TEXT_PRIMARY,
                },
            ));
            if !controller.phase.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(&controller.phase).color(theme::TEXT_SECONDARY));
            }
        });

        ui.add_space(8.0);

        match controller.progress_fraction() {
            Some(fraction) => {
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(format!(
                            "{} {}/{}",
                            controller.progress_label,
                            controller.files_done,
                            controller.files_total
                        ))
                        .desired_width(ui.available_width().min(520.0)),
                );
            }
            None if controller.is_active() => {
                ui.add(
                    egui::ProgressBar::new(0.0)
                        .animate(true)
                        .text(controller.phase.clone())
                        .desired_width(ui.available_width().min(520.0)),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new("Nothing is downloading right now.")
                        .color(theme::TEXT_SECONDARY),
                );
            }
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Cache").color(theme::TEXT_SECONDARY));
            ui.label(
                egui::RichText::new(shared_dir.to_string_lossy().to_string())
                    .size(11.0)
                    .color(theme::BORDER),
            );
        });
        ui.label(
            egui::RichText::new(
                "Libraries, assets and runtimes are shared between instances and reused across launches.",
            )
            .size(11.0)
            .color(theme::BORDER),
        );
    });

    ui.add_space(12.0);

    ui.horizontal(|ui| {
        ui.checkbox(&mut state.show_all_lines, "Show progress lines");
        if !controller.console.is_empty() && ui.small_button("Copy log").clicked() {
            ui.ctx().copy_text(controller.console.join("\n"));
        }
    });

    let lines: Vec<&String> = controller
        .console
        .iter()
        .filter(|line| {
            state.show_all_lines || line.starts_with("[stage]") || line.starts_with("[error]")
        })
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("downloads_log")
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if lines.is_empty() {
                ui.label(
                    egui::RichText::new("No activity yet. Press Play on the Home screen.")
                        .color(theme::TEXT_SECONDARY),
                );
            }
            ui.style_mut().override_font_id = Some(egui::FontId::monospace(11.0));
            for line in lines {
                ui.label(line.as_str());
            }
        });
}
