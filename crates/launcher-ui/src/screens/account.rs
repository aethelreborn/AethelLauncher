//! Account screen — offline profile plus the Microsoft sign-in entry point.

use super::super::theme;
use eframe::egui;
use launcher_core::auth::{microsoft, offline_uuid, validate_username, Account};

#[derive(Debug, Clone, PartialEq, Default)]
pub enum AuthTab {
    #[default]
    Offline,
    Microsoft,
}

/// What the panel wants the app to do after a frame.
#[derive(Debug, Clone, PartialEq)]
pub enum AccountAction {
    /// Persist and activate the offline profile.
    SaveOffline(String),
}

#[derive(Debug, Clone, Default)]
pub struct AccountState {
    pub active_tab: AuthTab,
    pub username_input: String,
    /// The username currently in effect (empty when none is set).
    pub saved_username: String,
    pub error: Option<String>,
    pub notice: Option<String>,
}

impl AccountState {
    /// Seed the panel from the persisted username.
    pub fn from_username(username: &str) -> Self {
        Self {
            active_tab: AuthTab::Offline,
            username_input: username.to_string(),
            saved_username: username.to_string(),
            error: None,
            notice: None,
        }
    }
}

/// Build the offline account for a username, or a human-readable error.
pub fn offline_account(username: &str) -> Result<Account, String> {
    validate_username(username)?;
    Ok(Account::Offline {
        name: username.to_string(),
        uuid: offline_uuid(username),
    })
}

pub fn account_panel(ui: &mut egui::Ui, state: &mut AccountState) -> Option<AccountAction> {
    let mut action = None;

    ui.label(
        egui::RichText::new("Account")
            .size(24.0)
            .color(theme::ACCENT_CYAN),
    );
    ui.separator();

    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.active_tab, AuthTab::Offline, "Offline");
        ui.selectable_value(&mut state.active_tab, AuthTab::Microsoft, "Microsoft");
    });
    ui.add_space(10.0);

    match state.active_tab {
        AuthTab::Offline => offline_panel(ui, state, &mut action),
        AuthTab::Microsoft => microsoft_panel(ui, state),
    }

    action
}

fn offline_panel(ui: &mut egui::Ui, state: &mut AccountState, action: &mut Option<AccountAction>) {
    theme::glass_card(ui, |ui| {
        ui.label(
            egui::RichText::new("Offline profile")
                .size(16.0)
                .color(theme::TEXT_PRIMARY),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Plays single-player and offline servers. Your UUID is derived the same way \
                 the vanilla launcher does it, so worlds and servers see a stable identity.",
            )
            .size(11.0)
            .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Username").color(theme::TEXT_SECONDARY));
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.username_input)
                    .desired_width(220.0)
                    .hint_text("3-16 characters"),
            );
            let submitted = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            let can_save = !state.username_input.trim().is_empty()
                && state.username_input.trim() != state.saved_username;

            let save = ui
                .add_enabled(
                    can_save,
                    egui::Button::new(egui::RichText::new("Save").color(theme::SUCCESS_GREEN)),
                )
                .clicked();

            if (save || submitted) && can_save {
                let name = state.username_input.trim().to_string();
                match offline_account(&name) {
                    Ok(_) => {
                        state.error = None;
                        state.notice = Some(format!("Signed in as {name}"));
                        action.replace(AccountAction::SaveOffline(name));
                    }
                    Err(e) => {
                        state.notice = None;
                        state.error = Some(e);
                    }
                }
            }
        });

        // Live preview of the derived UUID — the offline identity is
        // deterministic, so showing it makes wrong usernames obvious.
        let preview = state.username_input.trim();
        if !preview.is_empty() {
            if let Ok(account) = offline_account(preview) {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!("UUID  {}", account.uuid_str()))
                        .size(11.0)
                        .color(theme::BORDER),
                );
            }
        }
    });

    ui.add_space(10.0);

    if let Some(err) = &state.error {
        ui.label(egui::RichText::new(format!("⚠ {err}")).color(theme::DANGER_RED));
    }
    if let Some(notice) = &state.notice {
        ui.label(egui::RichText::new(notice).color(theme::SUCCESS_GREEN));
    }

    if !state.saved_username.is_empty() {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!("Active profile: {}", state.saved_username))
                .size(12.0)
                .color(theme::ACCENT_CYAN),
        );
    }
}

fn microsoft_panel(ui: &mut egui::Ui, state: &AccountState) {
    theme::glass_card(ui, |ui| {
        ui.label(
            egui::RichText::new("Microsoft sign-in")
                .size(16.0)
                .color(theme::TEXT_PRIMARY),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(if microsoft::is_enabled() {
                "Microsoft sign-in is enabled, but the device-code flow is not wired up yet."
            } else {
                "Microsoft sign-in is unavailable in this build. Use an Offline profile to play."
            })
            .size(12.0)
            .color(theme::ORANGE),
        );
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(format!("Current profile: {}", state.saved_username))
                .size(12.0)
                .color(theme::TEXT_SECONDARY),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_account_validates_username() {
        assert!(offline_account("Steve").is_ok());
        assert!(offline_account("ab").is_err());
        assert!(offline_account("has space").is_err());
    }

    #[test]
    fn derived_uuid_is_stable_and_version3() {
        let a = offline_account("Steve").unwrap();
        let b = offline_account("Steve").unwrap();
        assert_eq!(a.uuid_str(), b.uuid_str());
        assert_eq!(
            offline_uuid("Steve").get_version(),
            Some(uuid::Version::Md5)
        );
    }

    #[test]
    fn panel_seeds_from_saved_username() {
        let state = AccountState::from_username("Alex");
        assert_eq!(state.username_input, "Alex");
        assert_eq!(state.saved_username, "Alex");
    }
}
