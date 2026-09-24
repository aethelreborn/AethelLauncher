//! Splash screen — brand mark plus boot checks.

use super::super::{branding, theme};
use eframe::egui;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum SplashPhase {
    #[default]
    Checking,
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SplashState {
    pub phase: SplashPhase,
    pub message: String,
}

pub fn splash_panel(ui: &mut egui::Ui, state: &mut SplashState) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.18);

        let _ = branding::logo(ui, 132.0);

        ui.add_space(theme::SPACE_4);
        ui.label(
            egui::RichText::new("AETHEL")
                .size(34.0)
                .strong()
                .color(theme::TEXT),
        );
        ui.label(
            egui::RichText::new("LAUNCHER")
                .size(15.0)
                .color(theme::ACCENT_2),
        );

        ui.add_space(theme::SPACE_6);

        match state.phase {
            SplashPhase::Error => {
                ui.label(
                    egui::RichText::new("Boot checks failed")
                        .size(14.0)
                        .color(theme::DANGER),
                );
            }
            _ => {
                ui.spinner();
            }
        }

        if !state.message.is_empty() {
            ui.add_space(theme::SPACE_2);
            ui.label(
                egui::RichText::new(&state.message)
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
        }

        ui.add_space(theme::SPACE_6);
        ui.label(
            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(11.0)
                .color(theme::TEXT_DIM),
        );
    });
}
