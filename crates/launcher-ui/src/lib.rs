pub mod branding;
pub mod config;
pub mod cosmetics_assets;
pub mod fonts;
pub mod launch;
pub mod screens;
pub mod state;
pub mod theme;

use eframe::egui;
use screens::account::{AccountAction, AccountEvent, PlatformEvent};
use screens::library::LibraryAction;
use screens::store::StoreAction;
use state::{AppModel, Screen};
use std::time::Duration;
use theme::{ButtonKind, MotionPrefs};

pub struct AethelApp {
    model: AppModel,
    reduced_motion: bool,
}

impl AethelApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);

        let model = AppModel::new();
        let reduced_motion = model.config.reduced_motion;
        theme::set_motion_prefs(MotionPrefs {
            reduced: reduced_motion,
        });

        let mut model = model;
        if let Ok(name) = std::env::var("AETHEL_SCREEN") {
            let screen = match name.to_ascii_lowercase().as_str() {
                "home" => Some(Screen::Home),
                "library" => Some(Screen::Library),
                "store" | "cosmetics" => Some(Screen::Store),
                "mods" => Some(Screen::Mods),
                "downloads" => Some(Screen::Downloads),
                "settings" => Some(Screen::Settings),
                "account" => Some(Screen::Account),
                "news" => Some(Screen::News),
                "crash" | "crashviewer" => Some(Screen::CrashViewer),
                "splash" => Some(Screen::Splash),
                _ => None,
            };
            if let Some(screen) = screen {
                if screen == Screen::Store {
                    model.store.refresh();
                }
                if screen == Screen::Account {
                    if let Ok(tab) = std::env::var("AETHEL_ACCOUNT_TAB") {
                        model.account.active_tab = match tab.to_ascii_lowercase().as_str() {
                            "platform" => screens::account::AuthTab::Platform,
                            "microsoft" => screens::account::AuthTab::Microsoft,
                            _ => screens::account::AuthTab::Offline,
                        };
                    }
                }
                if screen == Screen::Mods {
                    if let Ok(tab) = std::env::var("AETHEL_MODS_TAB") {
                        model.mods.tab = match tab.to_ascii_lowercase().as_str() {
                            "browse" => screens::mods::ModsTab::Browse,
                            _ => screens::mods::ModsTab::Installed,
                        };
                    }
                    if let Ok(query) = std::env::var("AETHEL_MODS_QUERY") {
                        model.mods.query = query;
                    }
                }
                model.active_screen = screen;
            }
        }

        if std::env::var_os("AETHEL_LIBRARY_EDIT").is_some() {
            if let Some(id) = model.library.instances.first().map(|i| i.id.clone()) {
                model.library.begin_edit(&id);
            }
        }

        Self {
            model,
            reduced_motion,
        }
    }
}

impl eframe::App for AethelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.model.home.poll_fetch();
        self.model.library.poll_background();
        self.model.launch.poll();
        self.model.store.poll();
        self.model.mods.poll();
        if let Some(key) = self.model.mods.persist_cf_key.take() {
            self.model.config.curseforge_api_key = key;
            self.model.save_config();
        }

        match self.model.account.poll() {
            Some(AccountEvent::SignedIn) => self.model.store.refresh(),
            Some(AccountEvent::SignedOut) => self.model.store.forget_inventory(),
            None => {}
        }

        if self.model.launch.take_crash() {
            self.model
                .crash_viewer
                .scan(&self.model.data_dir.join("instances"));
            if !self.model.crash_viewer.entries.is_empty() {
                self.enter(Screen::CrashViewer);
            }
        }

        if self.model.config.reduced_motion != self.reduced_motion {
            self.reduced_motion = self.model.config.reduced_motion;
            theme::set_motion_prefs(MotionPrefs {
                reduced: self.reduced_motion,
            });
        }

        if self.model.library.available_versions.is_empty()
            && !self.model.home.available_versions.is_empty()
        {
            self.model.library.available_versions = self.model.home.available_versions.clone();
        }

        if let Some(id) = self
            .model
            .home
            .pending_instance
            .as_ref()
            .map(|p| p.id.clone())
        {
            if !self.model.library.instances.iter().any(|i| i.id == id) {
                self.model.home.pending_instance = None;
                if self.model.config.selected_instance.is_some() {
                    self.model.config.selected_instance = None;
                    self.model.save_config();
                }
            }
        }

        let active = self.model.launch.is_active();
        let active_preparing = self.model.launch.status == launch::LaunchStatus::Preparing;
        let active_running = self.model.launch.status == launch::LaunchStatus::Running;

        self.top_bar(ctx, active);
        self.nav_rail(ctx);

        let screen = self.model.active_screen.clone();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(20, 16)),
            )
            .show(ctx, |ui| match screen {
                Screen::Home => {
                    let request = screens::home::home_panel(
                        ui,
                        &mut self.model.home,
                        &mut self.model.launch,
                        &self.model.library.instances,
                    );
                    self.handle_home_request(request);
                }
                Screen::Library => {
                    let action = screens::library::library_panel(ui, &mut self.model.library);
                    self.handle_library_action(action);
                }
                Screen::Store => {
                    let action = screens::store::store_panel(ui, &mut self.model.store);
                    self.handle_store_action(action);
                }
                Screen::Mods => {
                    let game_dir = self.model.home.game_dir();
                    let mods_dir = game_dir.join("mods");
                    let (mc, loader) = match &self.model.home.pending_instance {
                        Some(p) => (Some(p.mc_version.as_str()), Some(p.loader.as_str())),
                        None if !self.model.home.selected_version.is_empty() => {
                            (Some(self.model.home.selected_version.as_str()), None)
                        }
                        None => (None, None),
                    };
                    screens::mods::mods_panel(
                        ui,
                        &mut self.model.mods,
                        &mods_dir,
                        &game_dir,
                        mc,
                        loader,
                    );
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
                    let action = screens::account::account_panel(ui, &mut self.model.account);
                    self.handle_account_action(action);
                }
                Screen::News => screens::news::news_panel(ui, &mut self.model.news),
                Screen::CrashViewer => {
                    screens::crash_viewer::crash_viewer_panel(ui, &mut self.model.crash_viewer)
                }
                Screen::Splash => screens::splash::splash_panel(ui, &mut self.model.splash),
            });

        if active_preparing
            || self.model.home.fetching
            || self.model.store.has_pending_animation()
            || self.model.mods.is_busy()
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if active_running {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }
}

impl AethelApp {
    fn nav_rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav_rail")
            .exact_width(64.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_RAISE)
                    .inner_margin(egui::Margin::symmetric(8, 12)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let _ = branding::logo(ui, 30.0);
                    ui.add_space(theme::SPACE_5);
                    ui.separator();
                    ui.add_space(theme::SPACE_3);

                    for (screen, icon, tip) in [
                        (Screen::Home, fonts::ICON_HOME, "Home"),
                        (Screen::Library, fonts::ICON_LIBRARY, "Library"),
                        (Screen::Store, fonts::ICON_STORE, "Cosmetics"),
                        (Screen::Mods, fonts::ICON_MODS, "Mods"),
                        (Screen::Downloads, fonts::ICON_DOWNLOADS, "Downloads"),
                        (Screen::News, fonts::ICON_NEWSPAPER, "News"),
                        (Screen::Settings, fonts::ICON_SETTINGS, "Settings"),
                        (Screen::Account, fonts::ICON_ACCOUNT, "Account"),
                        (Screen::CrashViewer, fonts::ICON_CRASH, "Crashes"),
                    ] {
                        let selected = self.model.active_screen == screen;
                        if rail_button(ui, icon, tip, selected).clicked() {
                            self.enter(screen);
                        }
                        ui.add_space(2.0);
                    }
                });
            });
    }

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
                    ui.label(
                        egui::RichText::new("Minecraft, organized.")
                            .size(12.0)
                            .color(theme::TEXT_DIM),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (icon, label) = if active {
                            (fonts::ICON_STOP, "Stop")
                        } else {
                            (fonts::ICON_PLAY, "Play")
                        };
                        let kind = if active {
                            ButtonKind::Danger
                        } else {
                            ButtonKind::Primary
                        };
                        let enabled = active || self.model.home.pending_instance.is_some();
                        if theme::icon_button(ui, icon, label, kind, enabled).clicked() {
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

                        theme::badge(ui, self.model.auth.username(), theme::ACCENT);
                    });
                });
            });
    }

    fn enter(&mut self, screen: Screen) {
        if screen == Screen::Store && !self.model.store.is_loaded() && !self.model.store.loading {
            self.model.store.refresh();
        }
        if screen == Screen::CrashViewer {
            self.model
                .crash_viewer
                .scan(&self.model.data_dir.join("instances"));
        }
        if screen == Screen::News {
            self.model
                .news
                .ensure_fetched(&launcher_core::platform::api_base_url());
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
            screens::home::HomeRequest::InstancePicked(id) => {
                self.model.config.selected_instance = if id.is_empty() { None } else { Some(id) };
                self.model.save_config();
            }
            screens::home::HomeRequest::EditInstance(id) => {
                self.enter(Screen::Library);
                self.model.library.begin_edit(&id);
            }
            screens::home::HomeRequest::OpenLibrary => {
                self.enter(Screen::Library);
            }
        }
    }

    fn handle_library_action(&mut self, action: Option<LibraryAction>) {
        match action {
            Some(LibraryAction::Play(instance)) => {
                self.select_instance(Some(&instance.id));
                self.model.home.last_error = None;
                self.model.library.mark_launched(&instance.id);
                self.model.active_screen = Screen::Home;
            }
            Some(LibraryAction::Create(name, version)) => {
                self.model.library.create_instance(name, version);
            }
            Some(LibraryAction::Saved(instance)) => {
                if self
                    .model
                    .home
                    .pending_instance
                    .as_ref()
                    .map(|p| p.id == instance.id)
                    == Some(true)
                {
                    self.select_instance(Some(&instance.id));
                }
            }
            Some(LibraryAction::OpenMods(instance)) => {
                self.select_instance(Some(&instance.id));
                self.enter(Screen::Mods);
            }
            None => {}
        }
    }

    fn select_instance(&mut self, id: Option<&str>) {
        self.model
            .home
            .select_instance(id, &self.model.library.instances);
        self.model.config.selected_instance = id.map(str::to_string);
        self.model.save_config();
    }

    fn handle_store_action(&mut self, action: Option<StoreAction>) {
        match action {
            Some(StoreAction::Equip(id)) => self.model.store.equip(&id),
            Some(StoreAction::Unequip(id)) => self.model.store.unequip(&id),
            Some(StoreAction::Buy(id)) => self.model.store.buy(&id),
            Some(StoreAction::Refresh) => self.model.store.refresh(),
            None => {}
        }
    }

    fn handle_account_action(&mut self, action: Option<AccountAction>) {
        match action {
            Some(AccountAction::SaveOffline(name)) => self.model.set_username(&name),
            Some(AccountAction::PlatformSignIn { email, password }) => {
                let client = self.model.platform.clone();
                let tx = self.model.account.begin_platform_job();
                std::thread::spawn(move || {
                    let Some(runtime) = worker_runtime(&tx) else {
                        return;
                    };
                    let result = runtime
                        .block_on(client.login(&email, &password))
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(PlatformEvent::Done(result));
                });
            }
            Some(AccountAction::PlatformRegister {
                email,
                username,
                password,
            }) => {
                let client = self.model.platform.clone();
                let tx = self.model.account.begin_platform_job();
                std::thread::spawn(move || {
                    let Some(runtime) = worker_runtime(&tx) else {
                        return;
                    };
                    let result = runtime
                        .block_on(client.register(&email, &username, &password))
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(PlatformEvent::Done(result));
                });
            }
            Some(AccountAction::PlatformSignOut) => {
                let client = self.model.platform.clone();
                let tx = self.model.account.begin_platform_job();
                std::thread::spawn(move || {
                    let Some(runtime) = worker_runtime(&tx) else {
                        return;
                    };
                    runtime.block_on(client.logout());
                    let _ = tx.send(PlatformEvent::SignedOut);
                });
            }
            None => {}
        }
    }

    fn record_instance_launch(&mut self) {
        if let Some(instance) = &self.model.home.pending_instance {
            let id = instance.id.clone();
            self.model.library.mark_launched(&id);
        }
    }
}

fn worker_runtime(tx: &std::sync::mpsc::Sender<PlatformEvent>) -> Option<tokio::runtime::Runtime> {
    match tokio::runtime::Runtime::new() {
        Ok(runtime) => Some(runtime),
        Err(e) => {
            let _ = tx.send(PlatformEvent::Done(Err(format!(
                "could not start async runtime: {e}"
            ))));
            None
        }
    }
}

fn rail_button(ui: &mut egui::Ui, icon: &str, tip: &str, selected: bool) -> egui::Response {
    let size = egui::vec2(46.0, 42.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if selected {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(8),
            theme::ACCENT.gamma_multiply(0.14),
        );
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left() + 1.0, rect.center().y - 11.0),
                egui::vec2(3.0, 22.0),
            ),
            egui::CornerRadius::same(2),
            theme::ACCENT,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(8), theme::BG_HOVER);
    }

    let color = if selected {
        theme::ACCENT
    } else if response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        fonts::icon_id(22.0),
        color,
    );

    response.on_hover_text(tip)
}
