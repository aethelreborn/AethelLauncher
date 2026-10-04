use super::super::theme;
use eframe::egui;
use serde::Deserialize;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

#[derive(Debug, Clone, Deserialize)]
pub struct NewsItem {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub url: Option<String>,
}

enum NewsEvent {
    Items(Vec<NewsItem>),
    Unavailable(String),
}

#[derive(Debug, Default)]
pub struct NewsState {
    items: Vec<NewsItem>,
    loading: bool,
    fetched: bool,
    rx: Option<Receiver<NewsEvent>>,
    notice: Option<String>,
}

impl NewsState {
    pub fn ensure_fetched(&mut self, base_url: &str) {
        if self.loading || self.fetched {
            return;
        }
        self.loading = true;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let url = format!("{}/api/v1/news", base_url.trim_end_matches('/'));

        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Runtime::new() else {
                return;
            };
            runtime.block_on(async {
                let result = reqwest::get(&url).await;
                let event = match result {
                    Ok(response) if response.status().is_success() => {
                        match response.json::<Vec<NewsItem>>().await {
                            Ok(items) => NewsEvent::Items(items),
                            Err(e) => NewsEvent::Unavailable(format!("bad news payload: {e}")),
                        }
                    }
                    Ok(response) => {
                        NewsEvent::Unavailable(format!("news route {}", response.status()))
                    }
                    Err(e) => NewsEvent::Unavailable(format!("backend unreachable: {e}")),
                };
                let _ = tx.send(event);
            });
        });
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        let mut done = false;
        let mut events = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }
        if done {
            self.rx = None;
            self.loading = false;
            self.fetched = true;
        }
        for event in events {
            match event {
                NewsEvent::Items(items) => {
                    self.items = items;
                    self.notice = None;
                }
                NewsEvent::Unavailable(note) => {
                    self.notice = Some(note);
                }
            }
        }
    }
}

pub fn news_panel(ui: &mut egui::Ui, state: &mut NewsState) {
    state.poll();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("News")
                .size(24.0)
                .strong()
                .color(theme::TEXT),
        );
        if state.loading {
            ui.label(
                egui::RichText::new("Loading…")
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
        }
    });
    ui.separator();

    if state.items.is_empty() {
        theme::empty_state(
            ui,
            "No news yet",
            "Announcements from the Aethel team land here.",
        );
        if let Some(notice) = &state.notice {
            ui.add_space(theme::SPACE_2);
            ui.label(
                egui::RichText::new(notice)
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
        }
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("news_feed")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for item in &state.items {
                egui::Frame::new()
                    .fill(theme::BG_RAISE)
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width() - 28.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&item.title)
                                    .size(16.0)
                                    .strong()
                                    .family(crate::fonts::strong_family())
                                    .color(theme::TEXT),
                            );
                            if !item.date.is_empty() {
                                ui.label(
                                    egui::RichText::new(&item.date)
                                        .size(11.0)
                                        .color(theme::TEXT_DIM),
                                );
                            }
                        });
                        if !item.body.is_empty() {
                            ui.add_space(theme::SPACE_2);
                            ui.label(
                                egui::RichText::new(&item.body)
                                    .size(13.0)
                                    .color(theme::TEXT_DIM),
                            );
                        }
                        if let Some(url) = &item.url {
                            ui.add_space(theme::SPACE_2);
                            if ui.link(url).clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            }
                        }
                    });
                ui.add_space(theme::SPACE_3);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn news_item_tolerates_missing_fields() {
        let item: NewsItem = serde_json::from_str(r#"{"title":"Alpha"}"#).unwrap();
        assert_eq!(item.title, "Alpha");
        assert_eq!(item.date, "");
        assert_eq!(item.body, "");
        assert_eq!(item.url, None);
    }

    #[test]
    fn news_feed_deserializes_full_payload() {
        let payload = r#"[{"title":"Launch","date":"2026-09-30","body":"We shipped.","url":"https://example.com"}]"#;
        let items: Vec<NewsItem> = serde_json::from_str(payload).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn ensure_fetched_is_single_flight() {
        let mut state = NewsState::default();
        state.ensure_fetched("http://127.0.0.1:9");
        assert!(state.loading);
        let rx_ptr = state.rx.as_ref().map(|_| ());
        state.ensure_fetched("http://127.0.0.1:9");
        assert_eq!(rx_ptr, state.rx.as_ref().map(|_| ()));

        for _ in 0..500 {
            state.poll();
            if !state.loading {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(state.fetched, "worker finished");
        assert!(
            state.notice.is_some(),
            "unreachable backend surfaces a notice"
        );
    }
}
