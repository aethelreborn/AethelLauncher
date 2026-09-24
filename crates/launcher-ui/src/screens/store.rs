//! Cosmetics screen — catalogue grid with equip / buy actions.
//!
//! Backed by [`launcher_core::platform`], which is offline-first: the grid
//! renders from cache (or the built-in seed) even with no backend, and only the
//! mutating actions require the server ([08 §4 Store](../../../opencode-docs/08-ui-design.md)).

use super::super::theme;
use eframe::egui;
use launcher_core::platform::{Cosmetic, Inventory, PlatformClient};

/// Re-exported so the shell can render the connection badge without depending
/// on `launcher-core`'s module path directly.
pub use launcher_core::platform::Connection;
use std::sync::mpsc::{channel, Receiver};

/// What the screen wants the app to do after a frame.
#[derive(Debug, Clone, PartialEq)]
pub enum StoreAction {
    Equip(String),
    Unequip(String),
    Refresh,
}

enum StoreEvent {
    Catalogue(Vec<Cosmetic>),
    Inventory(Box<Inventory>),
    Connection(Connection),
    Error(String),
    Notice(String),
}

pub struct StoreState {
    pub cosmetics: Vec<Cosmetic>,
    pub inventory: Inventory,
    pub loading: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    connection: Connection,
    /// Slug of the item with an in-flight equip/purchase.
    pub pending: Option<String>,
    /// Active kind filter, `None` = all.
    pub kind_filter: Option<String>,
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
    pub fn new(data_dir: &std::path::Path) -> Self {
        let client = PlatformClient::new(data_dir);
        let cosmetics = client.cached_catalogue();
        let inventory = client.cached_inventory();
        Self {
            cosmetics,
            inventory,
            loading: false,
            error: None,
            notice: None,
            connection: Connection::Unknown,
            pending: None,
            kind_filter: None,
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

    pub fn balance_label(&self) -> String {
        self.inventory.balance_label()
    }

    /// True while something needs the frame loop to keep ticking.
    pub fn has_pending_animation(&self) -> bool {
        self.loading || self.pending.is_some()
    }

    /// Distinct kinds present in the catalogue, for the filter chips.
    fn kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self
            .cosmetics
            .iter()
            .map(|c| c.kind.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        kinds.sort();
        kinds
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
        let username = self.inventory.username.clone();
        let username = if username.is_empty() {
            String::new()
        } else {
            username
        };

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

                if !username.is_empty() {
                    match client.fetch_inventory(&username).await {
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

    /// Set the player identity and pull their inventory.
    pub fn set_player(&mut self, username: &str) {
        if self.inventory.username == username && !self.inventory.username.is_empty() {
            return;
        }
        self.inventory.username = username.to_string();

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let client = self.client.clone();
        let username = username.to_string();

        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Runtime::new() else {
                return;
            };
            runtime.block_on(async {
                match client.fetch_inventory(&username).await {
                    Ok(inventory) => {
                        let _ = tx.send(StoreEvent::Inventory(Box::new(inventory)));
                    }
                    Err(_) => {
                        // Offline is normal — the cached inventory still shows.
                        let _ = tx.send(StoreEvent::Connection(Connection::Offline));
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

    fn start_action(&mut self, slug: &str, action: Action) {
        if self.pending.is_some() {
            return;
        }
        let username = self.inventory.username.clone();
        if username.is_empty() {
            self.error = Some("Set a username on the Account screen first.".to_string());
            return;
        }

        self.pending = Some(slug.to_string());
        self.error = None;
        self.notice = None;

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
                let result = match action {
                    Action::Equip => client.equip(&slug, &username).await,
                    Action::Unequip => client.unequip(&slug, &username).await,
                };
                match result {
                    Ok(inventory) => {
                        let _ = tx.send(StoreEvent::Inventory(Box::new(inventory)));
                        let verb = match action {
                            Action::Equip => "Equipped",
                            Action::Unequip => "Unequipped",
                        };
                        let _ = tx.send(StoreEvent::Notice(format!("{verb} {display_name}")));
                    }
                    Err(e) => {
                        let _ = tx.send(StoreEvent::Error(format!("{e:#}")));
                    }
                }
            });
        });
    }

    /// Called once per frame by the shell.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
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

#[derive(Debug, Clone, Copy)]
enum Action {
    Equip,
    Unequip,
}

/// Colour for a rarity tier.
fn rarity_color(rarity: &str) -> egui::Color32 {
    match rarity {
        "legendary" => egui::Color32::from_rgb(241, 196, 15),
        "epic" => egui::Color32::from_rgb(155, 89, 182),
        "rare" => egui::Color32::from_rgb(0, 210, 255),
        _ => egui::Color32::from_rgb(149, 165, 166),
    }
}

/// A stand-in "asset" so the grid looks deliberate while images are still
/// served from the temp placeholder ([08 §4](../../../opencode-docs/08-ui-design.md)):
/// a tinted silhouette, never a broken-image icon.
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

    // Simple shape hinting at the cosmetic kind.
    let centre = rect.center();
    let glyph = match cosmetic.kind.as_str() {
        "cape" | "elytra" => "▐",
        "wings" => "⋀",
        "hud" => "▤",
        "badge" => "★",
        "skin" => "◐",
        _ => "◆",
    };
    ui.painter().text(
        centre,
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(30.0),
        base,
    );
}

pub fn store_panel(ui: &mut egui::Ui, state: &mut StoreState, player: &str) -> Option<StoreAction> {
    // Keep the identity in sync with whatever the Account screen selected.
    if state.inventory.username != player && !player.is_empty() {
        state.set_player(player);
    }

    let mut action = None;

    ui.horizontal(|ui| {
        theme::screen_title(ui, "Cosmetics", theme::ACCENT_2);
        ui.add_space(theme::SPACE_3);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::button(
                ui,
                "⟳ Refresh",
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

    theme::dim(ui, format!("Signed in as {player}"));
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

    // Kind filter chips.
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
    let equipped = state.inventory.is_equipped(&cosmetic.slug);
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

        paint_placeholder(ui, cosmetic, egui::vec2(width - 24.0, 84.0));
        ui.add_space(theme::SPACE_2);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&cosmetic.name)
                    .size(14.0)
                    .strong()
                    .color(theme::TEXT),
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

        let (label, kind) = if equipped {
            ("Unequip", theme::ButtonKind::Secondary)
        } else if owned {
            ("Equip", theme::ButtonKind::Success)
        } else if cosmetic.is_free() {
            ("Claim", theme::ButtonKind::Success)
        } else {
            ("Buy", theme::ButtonKind::Primary)
        };

        let affordable =
            cosmetic.is_free() || state.inventory.balance_cents >= cosmetic.price_cents;
        let enabled = owned || cosmetic.is_free() || affordable;

        let response =
            theme::button_sized(ui, label, kind, egui::vec2(width - 24.0, 30.0), enabled);

        if response.clicked() {
            if equipped {
                *action = Some(StoreAction::Unequip(cosmetic.slug.clone()));
            } else {
                *action = Some(StoreAction::Equip(cosmetic.slug.clone()));
            }
        }
        if !enabled {
            response.on_hover_text("Not enough credits");
        }
    });
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
