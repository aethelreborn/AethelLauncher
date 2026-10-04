use super::super::theme;
use eframe::egui;
use launcher_core::auth::{offline_uuid, validate_username, Account};
use launcher_core::platform::PlatformSession;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

#[derive(Debug, Clone, PartialEq, Default)]
pub enum AuthTab {
    #[default]
    Offline,
    Platform,
    Microsoft,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccountAction {
    SaveOffline(String),
    PlatformSignIn {
        email: String,
        password: String,
    },
    PlatformRegister {
        email: String,
        username: String,
        password: String,
    },
    PlatformSignOut,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccountEvent {
    SignedIn,
    SignedOut,
}

pub enum PlatformEvent {
    Done(Result<PlatformSession, String>),
    SignedOut,
}

#[derive(Debug, Default)]
pub struct AccountState {
    pub active_tab: AuthTab,
    pub username_input: String,
    pub saved_username: String,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub platform_email: String,
    pub platform_password: String,
    pub platform_username: String,
    pub platform_register: bool,
    pub platform_busy: bool,
    pub platform_error: Option<String>,
    pub platform_notice: Option<String>,
    pub session_username: Option<String>,
    pub session_role: Option<String>,
    rx: Option<Receiver<PlatformEvent>>,
}

impl AccountState {
    pub fn from_username(username: &str) -> Self {
        Self {
            active_tab: AuthTab::Offline,
            username_input: username.to_string(),
            saved_username: username.to_string(),
            ..Default::default()
        }
    }

    pub fn set_session(&mut self, session: &PlatformSession) {
        self.session_username = Some(session.username.clone());
        self.session_role = Some(session.role.clone());
        self.platform_email.clear();
        self.platform_password.clear();
    }

    pub fn begin_platform_job(&mut self) -> Sender<PlatformEvent> {
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.platform_busy = true;
        self.platform_error = None;
        self.platform_notice = None;
        tx
    }

    pub fn poll(&mut self) -> Option<AccountEvent> {
        let Some(rx) = &self.rx else { return None };
        let mut result = None;
        loop {
            match rx.try_recv() {
                Ok(PlatformEvent::Done(Ok(session))) => {
                    self.platform_busy = false;
                    self.platform_password.clear();
                    self.platform_notice = Some(if self.platform_register {
                        format!("Account created — signed in as {}", session.username)
                    } else {
                        format!("Signed in as {}", session.username)
                    });
                    self.session_username = Some(session.username.clone());
                    self.session_role = Some(session.role.clone());
                    result = Some(AccountEvent::SignedIn);
                }
                Ok(PlatformEvent::Done(Err(message))) => {
                    self.platform_busy = false;
                    self.platform_error = Some(message);
                }
                Ok(PlatformEvent::SignedOut) => {
                    self.platform_busy = false;
                    self.session_username = None;
                    self.session_role = None;
                    self.platform_notice = Some("Signed out".to_string());
                    result = Some(AccountEvent::SignedOut);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.platform_busy = false;
                    self.platform_error = Some("sign-in failed — is the backend running?".into());
                    self.rx = None;
                    break;
                }
            }
        }
        result
    }
}

pub fn offline_account(username: &str) -> Result<Account, String> {
    validate_username(username)?;
    Ok(Account::Offline {
        name: username.to_string(),
        uuid: offline_uuid(username),
    })
}

pub fn account_panel(ui: &mut egui::Ui, state: &mut AccountState) -> Option<AccountAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        theme::screen_title(ui, "Account", theme::TEXT);
        ui.add_space(theme::SPACE_3);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(role) = &state.session_role {
                theme::badge(ui, role, theme::ACCENT);
            }
        });
    });
    ui.separator();

    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.active_tab, AuthTab::Offline, "Offline");
        ui.selectable_value(&mut state.active_tab, AuthTab::Platform, "Platform");
        ui.selectable_value(&mut state.active_tab, AuthTab::Microsoft, "Microsoft");
    });
    ui.add_space(10.0);

    match state.active_tab {
        AuthTab::Offline => offline_panel(ui, state, &mut action),
        AuthTab::Platform => platform_panel(ui, state, &mut action),
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

fn platform_panel(ui: &mut egui::Ui, state: &mut AccountState, action: &mut Option<AccountAction>) {
    if let Some(username) = state.session_username.clone() {
        theme::glass_card(ui, |ui| {
            ui.label(
                egui::RichText::new("Platform account")
                    .size(16.0)
                    .color(theme::TEXT_PRIMARY),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "Signed in as {username}{}",
                    state
                        .session_role
                        .as_deref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default()
                ))
                .size(13.0)
                .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Your cosmetics, credits and news follow this account across devices.",
                )
                .size(11.0)
                .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(10.0);
            let sign_out = ui
                .add_enabled(
                    !state.platform_busy,
                    egui::Button::new(
                        egui::RichText::new(if state.platform_busy {
                            "Signing out…"
                        } else {
                            "Sign out"
                        })
                        .color(theme::DANGER_RED),
                    ),
                )
                .clicked();
            if sign_out {
                *action = Some(AccountAction::PlatformSignOut);
            }
        });
    } else {
        theme::glass_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Aethel platform")
                        .size(16.0)
                        .color(theme::TEXT_PRIMARY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(state.platform_register, "Create account")
                        .clicked()
                    {
                        state.platform_register = true;
                        state.platform_error = None;
                    }
                    if ui
                        .selectable_label(!state.platform_register, "Sign in")
                        .clicked()
                    {
                        state.platform_register = false;
                        state.platform_error = None;
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Syncs cosmetics, credits and news. Optional — the launcher plays offline \
                     without it.",
                )
                .size(11.0)
                .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Email").color(theme::TEXT_SECONDARY));
                ui.add(
                    egui::TextEdit::singleline(&mut state.platform_email)
                        .desired_width(240.0)
                        .hint_text("you@example.com"),
                );
            });
            ui.add_space(6.0);
            if state.platform_register {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Username").color(theme::TEXT_SECONDARY));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.platform_username)
                            .desired_width(240.0)
                            .hint_text("3-16 characters"),
                    );
                });
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Password").color(theme::TEXT_SECONDARY));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.platform_password)
                        .desired_width(240.0)
                        .hint_text("8+ characters")
                        .password(true),
                );
                let submitted =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                let ready = state.platform_email.contains('@')
                    && state.platform_password.len() >= 8
                    && (!state.platform_register || !state.platform_username.trim().is_empty());
                let label = if state.platform_register {
                    "Create account"
                } else {
                    "Sign in"
                };
                let clicked = ui
                    .add_enabled(
                        ready && !state.platform_busy,
                        egui::Button::new(egui::RichText::new(label).color(theme::SUCCESS_GREEN)),
                    )
                    .clicked();

                if (clicked || submitted) && ready && !state.platform_busy {
                    *action = if state.platform_register {
                        Some(AccountAction::PlatformRegister {
                            email: state.platform_email.trim().to_string(),
                            username: state.platform_username.trim().to_string(),
                            password: state.platform_password.clone(),
                        })
                    } else {
                        Some(AccountAction::PlatformSignIn {
                            email: state.platform_email.trim().to_string(),
                            password: state.platform_password.clone(),
                        })
                    };
                }
            });
        });
    }

    ui.add_space(10.0);

    if state.platform_busy {
        ui.label(
            egui::RichText::new("Working…")
                .size(12.0)
                .color(theme::TEXT_SECONDARY),
        );
    }
    if let Some(err) = &state.platform_error {
        ui.label(egui::RichText::new(format!("⚠ {err}")).color(theme::DANGER_RED));
    }
    if let Some(notice) = &state.platform_notice {
        ui.label(egui::RichText::new(notice).color(theme::SUCCESS_GREEN));
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
            egui::RichText::new(
                "Not wired up yet — the device-code flow and token exchange land in a later \
                 milestone. Until then, use an Offline profile to play.",
            )
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
        assert_eq!(state.session_username, None);
    }

    #[test]
    fn platform_job_reports_sign_in_and_sign_out() {
        let mut state = AccountState::default();
        let tx = state.begin_platform_job();
        assert!(state.platform_busy);

        let session = PlatformSession {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            username: "Steve".into(),
            role: "user".into(),
        };
        tx.send(PlatformEvent::Done(Ok(session))).unwrap();
        assert_eq!(state.poll(), Some(AccountEvent::SignedIn));
        assert!(!state.platform_busy);
        assert_eq!(state.session_username.as_deref(), Some("Steve"));
        assert!(state.platform_password.is_empty());

        let tx = state.begin_platform_job();
        tx.send(PlatformEvent::SignedOut).unwrap();
        assert_eq!(state.poll(), Some(AccountEvent::SignedOut));
        assert_eq!(state.session_username, None);
    }

    #[test]
    fn platform_job_failure_keeps_the_error() {
        let mut state = AccountState {
            platform_password: "secret123".into(),
            ..Default::default()
        };
        let tx = state.begin_platform_job();
        tx.send(PlatformEvent::Done(Err("invalid email or password".into())))
            .unwrap();
        assert_eq!(state.poll(), None, "failure is not a session event");
        assert!(!state.platform_busy);
        assert_eq!(
            state.platform_error.as_deref(),
            Some("invalid email or password")
        );
        assert_eq!(state.session_username, None);
    }

    #[test]
    fn set_session_seeds_from_persisted_state() {
        let mut state = AccountState::from_username("Alex");
        let session = PlatformSession {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            username: "Notch".into(),
            role: "admin".into(),
        };
        state.set_session(&session);
        assert_eq!(state.session_username.as_deref(), Some("Notch"));
        assert_eq!(state.session_role.as_deref(), Some("admin"));
    }
}
