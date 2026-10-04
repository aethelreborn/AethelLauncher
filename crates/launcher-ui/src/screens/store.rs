use super::super::theme;
use eframe::egui;
use launcher_core::platform::{
    Cosmetic, EquippedState, Inventory, PlatformClient, PurchaseOutcome,
};

pub use launcher_core::platform::Connection;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

#[derive(Debug, Clone, PartialEq)]
pub enum StoreAction {
    Equip(String),
    Unequip(String),
    Buy(String),
    Refresh,
}

enum StoreEvent {
    Catalogue(Vec<Cosmetic>),
    Inventory(Box<Inventory>),
    Equipped(Box<EquippedState>),
    Connection(Connection),
    Error(String),
    Notice(String),
}

pub struct StoreState {
    pub cosmetics: Vec<Cosmetic>,
    pub inventory: Inventory,
    pub equipped: EquippedState,
    pub loading: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    connection: Connection,
    pub pending: Option<String>,
    pub kind_filter: Option<String>,
    /// Set when the local equipped set changed, so the launcher can push the new
    /// set to a running game session.
    pub cosmetics_changed: bool,
    client: PlatformClient,
    rx: Option<Receiver<StoreEvent>>,
}

impl std::fmt::Debug for StoreState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreState")
            .field("cosmetics", &self.cosmetics.len())
            .field("loading", &self.loading)
            .field("connection", &self.connection)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl StoreState {
    pub fn new(client: PlatformClient) -> Self {
        let cosmetics = client.cached_catalogue();
        let inventory = client.cached_inventory();
        let equipped = client.equipped_state();
        Self {
            cosmetics,
            inventory,
            equipped,
            loading: false,
            error: None,
            notice: None,
            connection: Connection::Unknown,
            pending: None,
            kind_filter: None,
            cosmetics_changed: false,
            client,
            rx: None,
        }
    }

    pub fn is_loaded(&self) -> bool {
        !self.cosmetics.is_empty()
    }

    pub fn connection(&self) -> Connection {
        self.connection
    }

    pub fn platform_username(&self) -> String {
        self.client.session_username()
    }

    pub fn is_signed_in(&self) -> bool {
        self.client.is_signed_in()
    }

    pub fn client(&self) -> &PlatformClient {
        &self.client
    }

    pub fn forget_inventory(&mut self) {
        self.inventory = Inventory::default();
        self.notice = None;
        self.error = None;
        self.client.clear_cached_inventory();
    }

    pub fn balance_label(&self) -> String {
        self.inventory.balance_label()
    }

    pub fn has_pending_animation(&self) -> bool {
        self.loading || self.pending.is_some() || self.rx.is_some()
    }

    fn kinds(&self) -> Vec<String> {
        self.cosmetics
            .iter()
            .map(|c| c.kind.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn refresh(&mut self) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.error = None;

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let client = self.client.clone();

        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(StoreEvent::Error(e.to_string()));
                    return;
                }
            };

            runtime.block_on(async {
                let connection = client.health().await;
                let _ = tx.send(StoreEvent::Connection(connection));

                match client.fetch_catalogue().await {
                    Ok(catalogue) => {
                        let _ = tx.send(StoreEvent::Catalogue(catalogue));
                    }
                    Err(e) => {
                        let _ = tx.send(StoreEvent::Error(format!(
                            "Could not refresh the catalogue: {e:#}"
                        )));
                    }
                }

                if client.is_signed_in() {
                    match client.fetch_inventory().await {
                        Ok(inventory) => {
                            let _ = tx.send(StoreEvent::Inventory(Box::new(inventory)));
                        }
                        Err(e) => {
                            let _ = tx.send(StoreEvent::Notice(format!(
                                "Using cached inventory ({e:#})"
                            )));
                        }
                    }
                }
            });
        });
    }

    pub fn equip(&mut self, slug: &str) {
        self.start_action(slug, Action::Equip);
    }

    pub fn unequip(&mut self, slug: &str) {
        self.start_action(slug, Action::Unequip);
    }

    pub fn buy(&mut self, slug: &str) {
        self.start_action(slug, Action::Buy);
    }

    fn start_action(&mut self, slug: &str, action: Action) {
        if self.pending.is_some() {
            return;
        }
        let signed_in = self.client.is_signed_in();
        if !signed_in && action == Action::Buy {
            self.error =
                Some("Sign in on the Account screen to buy and equip cosmetics.".to_string());
            return;
        }
        if !signed_in && action == Action::Equip && self.cosmetics.iter().all(|c| c.slug != slug) {
            self.error = Some("Unknown cosmetic.".to_string());
            return;
        }

        self.pending = Some(slug.to_string());
        self.error = None;
        self.notice = None;

        // The local equipped set is the source of truth for the in-game client,
        // so apply it (and hand it to the UI) before any network round-trip.
        let local = match action {
            Action::Equip => self.client.equip_local(slug),
            Action::Unequip => self.client.unequip_local(slug),
            Action::Buy => EquippedState::default(),
        };
        if action != Action::Buy {
            self.equipped = local.clone();
            self.cosmetics_changed = true;
        }

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let client = self.client.clone();
        let slug = slug.to_string();
        let display_name = self
            .cosmetics
            .iter()
            .find(|c| c.slug == slug)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| slug.clone());

        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Runtime::new() else {
                let _ = tx.send(StoreEvent::Error("could not start async runtime".into()));
                return;
            };
            runtime.block_on(async move {
                match action {
                    Action::Equip | Action::Unequip => {
                        let _ = tx.send(StoreEvent::Equipped(Box::new(local)));
                        let verb = match action {
                            Action::Unequip => "Unequipped",
                            _ => "Equipped",
                        };
                        if !signed_in {
                            let _ = tx.send(StoreEvent::Notice(format!(
                                "{verb} {display_name} on this machine"
                            )));
                            return;
                        }
                        let result = match action {
                            Action::Equip => client.equip(&slug).await,
                            _ => client.unequip(&slug).await,
                        };
                        match result {
                            Ok(inventory) => {
                                let _ = tx.send(StoreEvent::Inventory(Box::new(inventory)));
                                let _ =
                                    tx.send(StoreEvent::Notice(format!("{verb} {display_name}")));
                            }
                            Err(e) => {
                                let _ = tx.send(StoreEvent::Error(format!("{e:#}")));
                            }
                        }
                    }
                    Action::Buy => {
                        let request_id = uuid::Uuid::new_v4().to_string();
                        match client.purchase(&slug, &request_id).await {
                            Ok(PurchaseOutcome::InsufficientFunds { balance_cents }) => {
                                let _ = tx.send(StoreEvent::Error(format!(
                                    "Not enough credits (balance {})",
                                    balance_cents as f32 / 100.0
                                )));
                            }
                            Ok(outcome) => {
                                if let Ok(inventory) = client.fetch_inventory().await {
                                    let _ = tx.send(StoreEvent::Inventory(Box::new(inventory)));
                                }
                                let note = match outcome {
                                    PurchaseOutcome::AlreadyPurchased => {
                                        format!("Already owned {display_name}")
                                    }
                                    _ => format!("Purchased {display_name}"),
                                };
                                let _ = tx.send(StoreEvent::Notice(note));
                            }
                            Err(e) => {
                                let _ = tx.send(StoreEvent::Error(format!("{e:#}")));
                            }
                        }
                    }
                }
            });
        });
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        let mut events = Vec::new();
        let mut worker_done = false;
        loop {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    worker_done = true;
                    break;
                }
            }
        }
        if worker_done {
            self.rx = None;
            self.loading = false;
            self.pending = None;
        }

        for event in events {
            match event {
                StoreEvent::Catalogue(cosmetics) => {
                    self.cosmetics = cosmetics;
                    self.loading = false;
                    self.error = None;
                }
                StoreEvent::Inventory(inventory) => {
                    self.inventory = *inventory;
                    self.pending = None;
                }
                StoreEvent::Equipped(state) => {
                    self.equipped = *state;
                    self.cosmetics_changed = true;
                    self.pending = None;
                }
                StoreEvent::Connection(connection) => {
                    self.connection = connection;
                    if connection == Connection::Online {
                        self.loading = false;
                    }
                }
                StoreEvent::Error(message) => {
                    self.loading = false;
                    self.pending = None;
                    self.error = Some(message);
                    self.connection = Connection::Offline;
                }
                StoreEvent::Notice(message) => {
                    self.notice = Some(message);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Equip,
    Unequip,
    Buy,
}

fn rarity_color(rarity: &str) -> egui::Color32 {
    let [r, g, b] = launcher_core::platform::rarity_rgb(rarity);
    egui::Color32::from_rgb(r, g, b)
}

fn paint_cosmetic_art(ui: &mut egui::Ui, cosmetic: &Cosmetic, size: egui::Vec2) {
    let Some(texture) = crate::cosmetics_assets::texture(ui.ctx(), &cosmetic.asset_url) else {
        paint_placeholder(ui, cosmetic, size);
        return;
    };
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    let uv = if size.x > size.y {
        let half = 0.5 * size.y / size.x;
        egui::Rect::from_min_max(egui::pos2(0.0, 0.5 - half), egui::pos2(1.0, 0.5 + half))
    } else {
        let half = 0.5 * size.x / size.y;
        egui::Rect::from_min_max(egui::pos2(0.5 - half, 0.0), egui::pos2(0.5 + half, 1.0))
    };
    ui.painter()
        .image(texture.id(), rect, uv, egui::Color32::from_gray(255));
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(8),
        egui::Stroke::new(1.0_f32, theme::BORDER),
        egui::StrokeKind::Inside,
    );
}

fn paint_placeholder(ui: &mut egui::Ui, cosmetic: &Cosmetic, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let [r, g, b] = cosmetic.tint_rgb();
    let base = egui::Color32::from_rgb(r, g, b);

    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(8), base.gamma_multiply(0.18));
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(8),
        egui::Stroke::new(1.0_f32, base.gamma_multiply(0.55)),
        egui::StrokeKind::Inside,
    );

    let centre = rect.center();
    let glyph = match cosmetic.kind.as_str() {
        "cape" => crate::fonts::ICON_PALETTE,
        "elytra" => crate::fonts::ICON_AIRPLANE,
        "wings" => crate::fonts::ICON_FLIGHT,
        "hud" => crate::fonts::ICON_DASHBOARD,
        "badge" => "★",
        "skin" => crate::fonts::ICON_FACE,
        _ => crate::fonts::ICON_DIAMOND,
    };
    ui.painter().text(
        centre,
        egui::Align2::CENTER_CENTER,
        glyph,
        crate::fonts::icon_id(30.0),
        base,
    );
}

pub fn store_panel(ui: &mut egui::Ui, state: &mut StoreState) -> Option<StoreAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        theme::screen_title(ui, "Cosmetics", theme::TEXT);
        ui.add_space(theme::SPACE_3);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::button(
                ui,
                "↻ Refresh",
                theme::ButtonKind::Secondary,
                !state.loading,
            )
            .clicked()
            {
                action = Some(StoreAction::Refresh);
            }

            ui.add_space(theme::SPACE_2);
            theme::badge(
                ui,
                &format!("{} credits", state.balance_label()),
                theme::SUCCESS,
            );

            ui.add_space(theme::SPACE_2);
            match state.connection {
                Connection::Online => theme::badge(ui, "Online", theme::SUCCESS),
                Connection::Offline => theme::badge(ui, "Offline", theme::WARN),
                Connection::Unknown => theme::badge(ui, "Connecting…", theme::TEXT_DIM),
            }
        });
    });

    let signed_in = state.platform_username();
    if signed_in.is_empty() {
        theme::dim(ui, "Not signed in — cosmetics sync is off");
        ui.add_space(theme::SPACE_2);
        theme::dim(
            ui,
            "Sign in on the Account screen to buy, equip and sync your cosmetics.",
        );
    } else {
        theme::dim(ui, format!("Signed in as {signed_in}"));
    }
    ui.add_space(theme::SPACE_3);

    if let Some(error) = &state.error {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("⚠").color(theme::DANGER));
            ui.label(egui::RichText::new(error).color(theme::DANGER).size(12.0));
            if theme::button(ui, "Retry", theme::ButtonKind::Secondary, true).clicked() {
                action = Some(StoreAction::Refresh);
            }
        });
        ui.add_space(theme::SPACE_2);
    }
    if let Some(notice) = &state.notice {
        ui.label(egui::RichText::new(notice).size(12.0).color(theme::SUCCESS));
        ui.add_space(theme::SPACE_2);
    }

    let kinds = state.kinds();
    ui.horizontal_wrapped(|ui| {
        if theme::pill(ui, "All", state.kind_filter.is_none()).clicked() {
            state.kind_filter = None;
        }
        for kind in kinds {
            let label = capitalize(&kind);
            let selected = state.kind_filter.as_deref() == Some(kind.as_str());
            if theme::pill(ui, &label, selected).clicked() {
                state.kind_filter = if selected { None } else { Some(kind.clone()) };
            }
        }
    });

    ui.add_space(theme::SPACE_3);

    if state.cosmetics.is_empty() {
        theme::empty_state(
            ui,
            "No cosmetics available",
            "Press Refresh once the backend is reachable.",
        );
        return action;
    }

    let visible: Vec<Cosmetic> = state
        .cosmetics
        .iter()
        .filter(|c| {
            state
                .kind_filter
                .as_ref()
                .map(|k| &c.kind == k)
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("store_grid")
        .show(ui, |ui| {
            let card_width = 208.0;
            let spacing = theme::SPACE_2;
            let available = ui.available_width();
            let columns = ((available + spacing) / (card_width + spacing))
                .floor()
                .max(1.0) as usize;

            for row in visible.chunks(columns) {
                ui.horizontal(|ui| {
                    for cosmetic in row {
                        cosmetic_card(ui, state, cosmetic, card_width, &mut action);
                    }
                });
                ui.add_space(spacing);
            }
        });

    action
}

fn cosmetic_card(
    ui: &mut egui::Ui,
    state: &StoreState,
    cosmetic: &Cosmetic,
    width: f32,
    action: &mut Option<StoreAction>,
) {
    let owned = state.inventory.owns(&cosmetic.slug);
    let equipped =
        state.equipped.contains(&cosmetic.slug) || state.inventory.is_equipped(&cosmetic.slug);
    let busy = state.pending.as_deref() == Some(cosmetic.slug.as_str());
    let rarity = rarity_color(&cosmetic.rarity);

    let frame = egui::Frame::new()
        .fill(theme::BG_RAISE)
        .stroke(egui::Stroke::new(
            if equipped { 2.0_f32 } else { 1.0_f32 },
            if equipped {
                theme::ACCENT
            } else {
                theme::BORDER
            },
        ))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(10, 10));

    frame.show(ui, |ui| {
        ui.set_width(width - 20.0);
        ui.vertical(|ui| {
            paint_cosmetic_art(ui, cosmetic, egui::vec2(width - 24.0, 84.0));
            ui.add_space(theme::SPACE_2);

            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&cosmetic.name)
                            .size(14.0)
                            .strong()
                            .family(crate::fonts::strong_family())
                            .color(theme::TEXT),
                    )
                    .truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if equipped {
                        ui.label(egui::RichText::new("✓").size(13.0).color(theme::SUCCESS));
                    }
                });
            });

            ui.horizontal(|ui| {
                theme::badge(ui, &capitalize(&cosmetic.rarity), rarity);
                ui.label(
                    egui::RichText::new(cosmetic.price_label())
                        .size(12.0)
                        .color(if cosmetic.is_free() {
                            theme::SUCCESS
                        } else {
                            theme::TEXT_DIM
                        }),
                );
            });

            ui.add_space(theme::SPACE_2);

            if busy {
                ui.add(
                    egui::ProgressBar::new(0.0)
                        .animate(true)
                        .desired_width(ui.available_width()),
                );
                return;
            }

            // Free pieces can be worn locally without an account, so the in-game client has
            // something to render offline.
            let locally_wearable = !state.is_signed_in() && cosmetic.is_free();
            let (label, kind) = if equipped {
                ("Unequip", theme::ButtonKind::Secondary)
            } else if owned || locally_wearable {
                ("Equip", theme::ButtonKind::Success)
            } else if cosmetic.is_free() {
                ("Claim", theme::ButtonKind::Success)
            } else {
                ("Buy", theme::ButtonKind::Primary)
            };

            let affordable =
                cosmetic.is_free() || state.inventory.balance_cents >= cosmetic.price_cents;
            let enabled = owned || locally_wearable || cosmetic.is_free() || affordable;

            let response =
                theme::button_sized(ui, label, kind, egui::vec2(width - 24.0, 30.0), enabled);

            if response.clicked() {
                if equipped {
                    *action = Some(StoreAction::Unequip(cosmetic.slug.clone()));
                } else if owned || locally_wearable {
                    *action = Some(StoreAction::Equip(cosmetic.slug.clone()));
                } else {
                    *action = Some(StoreAction::Buy(cosmetic.slug.clone()));
                }
            }
            if !enabled {
                response.on_hover_text("Not enough credits");
            } else if locally_wearable {
                response.on_hover_text("Equipped on this machine only");
            }
        });
    });
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
