//! Launcher UI — egui-based interface for Aethel Launcher.

pub mod branding;
pub mod config;
pub mod launch;
pub mod screens;
pub mod state;
pub mod theme;

use eframe::egui;
use screens::account::AccountAction;
use screens::home::PendingInstance;
use screens::library::LibraryAction;
use screens::store::StoreAction;
use state::{AppModel, Screen};
use std::time::Duration;
use theme::{ButtonKind, MotionPrefs};

pub struct AethelApp {
    model: AppModel,
    /// Cached so we only flip the style when the user changes it.
    reduced_motion: bool,
}

impl AethelApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Style is installed once. Re-installing every frame would clone the
        // whole style tree 60×/s for no visual difference.
        theme::install(&cc.egui_ctx);

        let model = AppModel::new();
        let reduced_motion = model.config.reduced_motion;
        theme::set_motion_prefs(MotionPrefs {
            reduced: reduced_motion,
        });

        Self {
            model,
            reduced_motion,
        }
    }
}

impl eframe::App for AethelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain async results (version fetching, launch progress, store fetch).
        self.model.home.poll_fetch();
        self.model.library.poll_background();
        self.model.launch.poll();
        self.model.store.poll();

        if self.model.config.reduced_motion != self.reduced_motion {
            self.reduced_motion = self.model.config.reduced_motion;
            theme::set_motion_prefs(MotionPrefs {
                reduced: self.reduced_motion,
            });
        }

        // Give the Library screen the full version list once it is available.
        if self.model.library.available_versions.is_empty()
            && !self.model.home.available_versions.is_empty()
        {
            self.model.library.available_versions = self.model.home.available_versions.clone();
        }

        let active = self.model.launch.is_active();
        let busy = active || self.model.home.fetching || self.model.store.loading;

        self.top_bar(ctx, active);
        self.status_bar(ctx, active);

        // Clone so the central-panel match does not hold a borrow of the field.
        let screen = self.model.active_screen.clone();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(20, 16)),
            )
            .show(ctx, |ui| match screen {
                Screen::Home => {
                    let request =
                        screens::home::home_panel(ui, &mut self.model.home, &mut self.model.launch);
                    self.handle_home_request(request);
                }
                Screen::Library => {
                    let action = screens::library::library_panel(ui, &mut self.model.library);
                    self.handle_library_action(action);
                }
                Screen::Store => {
                    let action = screens::store::store_panel(
                        ui,
                        &mut self.model.store,
                        self.model.auth.username(),
                    );
                    self.handle_store_action(action);
                }
                Screen::Mods => {
                    let game_dir = self.model.home.game_dir();
                    let mods_dir = game_dir.join("mods");
                    screens::mods::mods_panel(ui, &mut self.model.mods, &mods_dir, &game_dir);
                }
                Screen::Downloads => screens::downloads::downloads_panel(
                    ui,
                    &mut self.model.downloads,
                    &self.model.launch,
                    &self.model.data_dir,
                ),
                Screen::Settings => {
                    if screens::settings::settings_panel(ui, &mut self.model.settings) {
                        self.model.apply_settings();
                    }
                }
                Screen::Account => {
                    if let Some(AccountAction::SaveOffline(name)) =
                        screens::account::account_panel(ui, &mut self.model.account)
                    {
                        self.model.set_username(&name);
                    }
                }
                Screen::News => screens::news::news_panel(ui, &mut self.model.news),
                Screen::CrashViewer => {
                    screens::crash_viewer::crash_viewer_panel(ui, &mut self.model.crash_viewer)
                }
                Screen::Splash => screens::splash::splash_panel(ui, &mut self.model.splash),
            });

        // Only repaint continuously while something is actually animating or in
        // flight — an idle launcher should not burn a laptop's CPU.
        if busy || self.model.store.has_pending_animation() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

impl AethelApp {
    fn top_bar(&mut self, ctx: &egui::Context, active: bool) {
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_RAISE)
                    .inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    branding::wordmark(ui, 26.0, 15.0);
                    ui.add_space(theme::SPACE_4);
                    ui.separator();
                    ui.add_space(theme::SPACE_2);

                    for (screen, label) in [
                        (Screen::Home, "Home"),
                        (Screen::Library, "Library"),
                        (Screen::Store, "Cosmetics"),
                        (Screen::Mods, "Mods"),
                        (Screen::Downloads, "Downloads"),
                        (Screen::Settings, "Settings"),
                        (Screen::Account, "Account"),
                    ] {
                        let selected = self.model.active_screen == screen;
                        if theme::pill(ui, label, selected).clicked() {
                            self.enter(screen);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if active { "■  STOP" } else { "▶  PLAY" };
                        let kind = if active {
                            ButtonKind::Danger
                        } else {
                            ButtonKind::Primary
                        };
                        let enabled = active || !self.model.home.selected_version.is_empty();
                        let size = egui::vec2(112.0, 34.0);
                        if theme::button_sized(ui, label, kind, size, enabled).clicked() {
                            if active {
                                self.model.launch.stop();
                                self.model.launch.push_note("Stop requested…");
                            } else {
                                let request = self.model.home.launch_request(&self.model.auth);
                                self.record_instance_launch();
                                self.model.launch.start(request);
                                self.model.active_screen = Screen::Home;
                            }
                        }

                        ui.add_space(theme::SPACE_2);

                        // Profile chip.
                        theme::badge(ui, self.model.auth.username(), theme::ACCENT);
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context, active: bool) {
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_RAISE)
                    .inner_margin(egui::Margin::symmetric(16, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let launch_status = self.model.launch.status.label();
                    ui.label(egui::RichText::new(launch_status).size(11.0).color(
                        if self.model.launch.status == launch::LaunchStatus::Failed {
                            theme::DANGER
                        } else if active {
                            theme::ACCENT_2
                        } else {
                            theme::TEXT_DIM
                        },
                    ));

                    ui.separator();

                    // Backend health, from cache — never blocks the frame.
                    let (dot, text, color) = match self.model.store.connection() {
                        screens::store::Connection::Online => {
                            ("●", "Cosmetics online", theme::SUCCESS)
                        }
                        screens::store::Connection::Offline => {
                            ("●", "Cosmetics offline", theme::WARN)
                        }
                        screens::store::Connection::Unknown => ("●", "Cosmetics…", theme::TEXT_DIM),
                    };
                    ui.label(egui::RichText::new(dot).size(10.0).color(color));
                    ui.label(egui::RichText::new(text).size(11.0).color(theme::TEXT_DIM));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut reduced = self.model.config.reduced_motion;
                        if ui
                            .checkbox(&mut reduced, "Reduced motion")
                            .on_hover_text("Stop all animations (also disables the Play glow)")
                            .changed()
                        {
                            self.model.config.reduced_motion = reduced;
                            self.model.save_config();
                        }
                        ui.label(
                            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                        );
                    });
                });
            });
    }

    /// Navigate, refreshing whatever the destination needs.
    fn enter(&mut self, screen: Screen) {
        if screen == Screen::Store && !self.model.store.is_loaded() && !self.model.store.loading {
            self.model.store.refresh();
        }
        self.model.active_screen = screen;
    }

    fn handle_home_request(&mut self, request: screens::home::HomeRequest) {
        match request {
            screens::home::HomeRequest::None => {}
            screens::home::HomeRequest::Launch => {
                let request = self.model.home.launch_request(&self.model.auth);
                self.record_instance_launch();
                self.model.launch.start(request);
            }
        }
    }

    fn handle_library_action(&mut self, action: Option<LibraryAction>) {
        match action {
            Some(LibraryAction::Play(instance)) => {
                let id = instance.id.clone();
                self.model.home.selected_version = instance.mc_version.clone();
                self.model.home.pending_instance = Some(PendingInstance {
                    id: instance.id.clone(),
                    name: instance.name.clone(),
                    ram_mb: instance.ram_mb.max(1024),
                    renderer: instance.renderer.clone(),
                });
                self.model.home.last_error = None;
                self.model.library.mark_launched(&id);
                self.model.active_screen = Screen::Home;
            }
            Some(LibraryAction::Create(name, version)) => {
                self.model.library.create_instance(name, version);
            }
            None => {}
        }
    }

    fn handle_store_action(&mut self, action: Option<StoreAction>) {
        match action {
            Some(StoreAction::Equip(id)) => self.model.store.equip(&id),
            Some(StoreAction::Unequip(id)) => self.model.store.unequip(&id),
            Some(StoreAction::Refresh) => self.model.store.refresh(),
            None => {}
        }
    }

    /// Mark the pending instance as just-launched (drives Library ordering).
    fn record_instance_launch(&mut self) {
        if let Some(instance) = &self.model.home.pending_instance {
            let id = instance.id.clone();
            self.model.library.mark_launched(&id);
        }
    }
}
