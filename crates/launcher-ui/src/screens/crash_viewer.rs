//! Crash viewer screen.

use super::super::theme;
use eframe::egui;

#[derive(Debug, Clone, Default)]
pub struct CrashViewerState {}

pub fn crash_viewer_panel(_ui: &mut egui::Ui, _state: &mut CrashViewerState) {
    _ui.heading(egui::RichText::new("Crashes").color(theme::TEXT_SECONDARY));
    _ui.label("Coming soon...");
}
