//! News screen.

use super::super::theme;
use eframe::egui;

#[derive(Debug, Clone, Default)]
pub struct NewsState {}

pub fn news_panel(_ui: &mut egui::Ui, _state: &mut NewsState) {
    _ui.heading(egui::RichText::new("News").color(theme::TEXT_SECONDARY));
    _ui.label("Coming soon...");
}
