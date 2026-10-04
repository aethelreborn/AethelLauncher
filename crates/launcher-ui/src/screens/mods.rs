use super::super::{fonts, theme};
use eframe::egui;
use launcher_core::install::downloader::{download_file, DownloadTask};
use launcher_core::modpack::{resolve_mod_file, search_mods, BrowseSource, GameContext, ModHit};
use launcher_core::perf::ManagedManifest;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct ModFile {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModsTab {
    #[default]
    Installed,
    Browse,
}

enum BrowseEvent {
    Results(Vec<ModHit>),
    SearchError(String),
    Installed { title: String, filename: String },
    InstallError(String),
}

#[derive(Debug, Default)]
pub struct ModsState {
    pub path: String,
    pub mods: Vec<ModFile>,
    pub error: Option<String>,
    loaded: bool,
    pub pending_delete: Option<String>,
    pub notice: Option<String>,
    pub manual_path: bool,

    pub tab: ModsTab,
    pub browse_source: BrowseSource,
    pub query: String,
    pub results: Vec<ModHit>,
    pub search_busy: bool,
    pub installing: Option<String>,
    pub browse_error: Option<String>,
    pub cf_key: String,
    pub persist_cf_key: Option<String>,
    rx: Option<Receiver<BrowseEvent>>,
    auto_search: Option<String>,
    pub pack_status: Option<String>,
    pack_key: Option<String>,
    last_drop: Option<Instant>,
}

impl Clone for ModsState {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            mods: self.mods.clone(),
            error: self.error.clone(),
            loaded: self.loaded,
            pending_delete: self.pending_delete.clone(),
            notice: self.notice.clone(),
            manual_path: self.manual_path,
            tab: self.tab,
            browse_source: self.browse_source,
            query: self.query.clone(),
            results: self.results.clone(),
            search_busy: self.search_busy,
            installing: self.installing.clone(),
            browse_error: self.browse_error.clone(),
            cf_key: self.cf_key.clone(),
            persist_cf_key: self.persist_cf_key.clone(),
            rx: None,
            auto_search: self.auto_search.clone(),
            pack_status: self.pack_status.clone(),
            pack_key: self.pack_key.clone(),
            last_drop: self.last_drop,
        }
    }
}

impl ModsState {
    pub fn point_at(&mut self, dir: &Path) {
        let display = dir.to_string_lossy().to_string();
        if self.path != display {
            self.path = display;
            self.loaded = false;
            self.pending_delete = None;
        }
        if !self.loaded {
            self.refresh();
        }
    }

    pub fn refresh(&mut self) {
        self.loaded = true;
        self.mods.clear();
        self.error = None;

        let dir = PathBuf::from(self.path.trim());
        if !dir.is_dir() {
            self.error = Some(format!("{} does not exist yet.", dir.display()));
            return;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                self.error = Some(format!("Could not read {}: {e}", dir.display()));
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("jar") {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            self.mods.push(ModFile { name, size });
        }
        self.mods.sort_by_key(|a| a.name.to_lowercase());
    }

    pub fn add_files(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("JAR files", &["jar"])
            .pick_files();
        if let Some(paths) = picked {
            self.import_paths(paths);
        }
    }

    pub fn import_paths(&mut self, paths: Vec<PathBuf>) {
        let now = Instant::now();
        if let Some(prev) = self.last_drop {
            if now.duration_since(prev) < Duration::from_millis(800) {
                return;
            }
        }
        self.last_drop = Some(now);

        let jars: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("jar"))
            .collect();
        if jars.is_empty() {
            self.error = Some("Only .jar files can be installed.".to_string());
            return;
        }
        match copy_into(&PathBuf::from(self.path.trim()), &jars) {
            Ok(added) => {
                self.refresh();
                self.notice = Some(format!("Added {added} file(s)."));
            }
            Err(e) => {
                self.error = Some(e);
                self.refresh();
            }
        }
    }

    pub fn delete_mod(&mut self, name: &str) -> Result<(), String> {
        let dir = PathBuf::from(self.path.trim());
        let Some(base) = Path::new(name).file_name().and_then(|n| n.to_str()) else {
            return Err("Invalid file name.".to_string());
        };
        let path = dir.join(base);
        if !path.is_file() {
            return Err(format!("{base} not found."));
        }
        std::fs::remove_file(&path).map_err(|e| format!("Could not delete {base}: {e}"))?;
        self.pending_delete = None;
        self.refresh();
        self.notice = Some(format!("Removed {base}."));
        Ok(())
    }

    pub fn set_cf_key(&mut self, key: String) {
        if self.cf_key != key {
            self.cf_key = key.clone();
            self.persist_cf_key = Some(key);
            self.browse_error = None;
        }
    }

    pub fn start_search(&mut self, mc_version: Option<&str>, loader: Option<&str>) {
        if self.browse_source == BrowseSource::CurseForge && self.cf_key.trim().is_empty() {
            self.browse_error = Some(
                "CurseForge needs a free API key — paste it above (console.curseforge.com)."
                    .to_string(),
            );
            return;
        }
        self.search_busy = true;
        self.browse_error = None;
        let ctx = GameContext::from_parts(mc_version, loader);
        let query = self.query.trim().to_string();
        let source = self.browse_source;
        let key = self.cf_key.trim().to_string();
        tracing::debug!("browse: search queued q={query:?} src={source:?}");
        self.spawn_worker(move |client, tx, rt| {
            let key = (!key.is_empty()).then_some(key.as_str());
            tracing::debug!("browse: worker runtime ready, requesting…");
            let out =
                rt.block_on(async move { search_mods(&client, source, &query, &ctx, key).await });
            match out {
                Ok(hits) => {
                    tracing::debug!("browse: worker got {} hits", hits.len());
                    let _ = tx.send(BrowseEvent::Results(hits));
                }
                Err(e) => {
                    tracing::warn!("browse: worker error: {e}");
                    let _ = tx.send(BrowseEvent::SearchError(e.to_string()));
                }
            }
        });
    }

    pub fn start_install(
        &mut self,
        hit: &ModHit,
        mc_version: Option<&str>,
        loader: Option<&str>,
        mods_dir: &Path,
    ) {
        if self.installing.is_some() {
            return;
        }
        if hit.source == BrowseSource::CurseForge && self.cf_key.trim().is_empty() {
            self.browse_error = Some(
                "CurseForge needs a free API key — paste it above (console.curseforge.com)."
                    .to_string(),
            );
            return;
        }
        self.installing = Some(hit.project_id.clone());
        self.browse_error = None;
        let ctx = GameContext::from_parts(mc_version, loader);
        let key = self.cf_key.trim().to_string();
        let hit = hit.clone();
        let dir = mods_dir.to_path_buf();
        self.spawn_worker(move |client, tx, rt| {
            let key = (!key.is_empty()).then_some(key.as_str());
            let out: anyhow::Result<String> = rt.block_on(async {
                let (filename, url) =
                    resolve_mod_file(&client, hit.source, &hit.project_id, &ctx, key).await?;
                std::fs::create_dir_all(&dir)?;
                let dest = available_destination(&dir, OsStr::new(&filename));
                download_file(&client, &DownloadTask::new(url, &dest)).await?;
                Ok(dest
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or(filename))
            });
            match out {
                Ok(filename) => {
                    let _ = tx.send(BrowseEvent::Installed {
                        title: hit.title,
                        filename,
                    });
                }
                Err(e) => {
                    let _ = tx.send(BrowseEvent::InstallError(e.to_string()));
                }
            }
        });
    }

    pub fn is_busy(&self) -> bool {
        self.search_busy || self.installing.is_some()
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        let mut events = Vec::new();
        let mut closed = false;
        loop {
            match rx.try_recv() {
                Ok(ev) => events.push(ev),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }
        if closed {
            self.rx = None;
            if events.is_empty() {
                self.search_busy = false;
                self.installing = None;
                self.browse_error = Some("The browse worker stopped unexpectedly.".to_string());
            }
        }
        for ev in events {
            match ev {
                BrowseEvent::Results(hits) => {
                    self.results = hits;
                    self.search_busy = false;
                }
                BrowseEvent::SearchError(e) => {
                    self.search_busy = false;
                    self.installing = None;
                    self.browse_error = Some(e);
                }
                BrowseEvent::Installed { title, filename } => {
                    self.installing = None;
                    self.refresh();
                    self.notice = Some(format!("Installed {title} as {filename}."));
                }
                BrowseEvent::InstallError(e) => {
                    self.installing = None;
                    self.browse_error = Some(e);
                }
            }
        }
    }

    fn spawn_worker<F>(&mut self, work: F)
    where
        F: FnOnce(reqwest::Client, Sender<BrowseEvent>, tokio::runtime::Runtime) + Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let client = match reqwest::Client::builder()
                .user_agent(concat!("Aethel-Launcher/", env!("CARGO_PKG_VERSION")))
                .build()
            {
                Ok(client) => client,
                Err(e) => {
                    let _ = tx.send(BrowseEvent::SearchError(format!(
                        "could not start HTTP client: {e}"
                    )));
                    return;
                }
            };
            match tokio::runtime::Runtime::new() {
                Ok(rt) => work(client, tx, rt),
                Err(e) => {
                    let _ = tx.send(BrowseEvent::SearchError(format!(
                        "could not start async runtime: {e}"
                    )));
                }
            }
        });
    }
}

fn read_pack_status(mods_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(ManagedManifest::path(mods_dir)).ok()?;
    let manifest: ManagedManifest = serde_json::from_str(&raw).ok()?;
    if manifest.installed.is_empty() {
        return None;
    }
    let shown: Vec<String> = manifest
        .installed
        .iter()
        .take(3)
        .map(|m| format!("{} {}", m.name, m.version))
        .collect();
    let more = if manifest.installed.len() > 3 {
        format!(" +{} more", manifest.installed.len() - 3)
    } else {
        String::new()
    };
    let renderer = if manifest.renderer.is_empty() {
        "unknown renderer".to_string()
    } else {
        manifest.renderer.clone()
    };
    Some(format!(
        "Performance pack: {}{} · {} {} · {renderer}",
        shown.join(", "),
        more,
        manifest.mc_version,
        manifest.loader
    ))
}

pub fn copy_into(dir: &Path, sources: &[PathBuf]) -> Result<usize, String> {
    if dir.as_os_str().is_empty() {
        return Err("No folder selected.".to_string());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;

    let mut added = 0;
    for src in sources {
        let Some(name) = src.file_name() else {
            continue;
        };
        let dest = available_destination(dir, name);
        std::fs::copy(src, &dest).map_err(|e| format!("Could not copy {}: {e}", src.display()))?;
        added += 1;
    }
    Ok(added)
}

fn available_destination(dir: &Path, name: &OsStr) -> PathBuf {
    let direct = dir.join(name);
    if direct.exists() {
        unique_destination(dir, name)
    } else {
        direct
    }
}

fn unique_destination(dir: &Path, name: &OsStr) -> PathBuf {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string_lossy().to_string());
    let ext = path.extension().and_then(|e| e.to_str());
    for i in 1..1000 {
        let candidate = match ext {
            Some(ext) => dir.join(format!("{stem} ({i}).{ext}")),
            None => dir.join(format!("{stem} ({i})")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(name)
}

fn human_count(n: u64) -> String {
    const K: u64 = 1_000;
    const M: u64 = K * 1_000;
    if n >= M {
        format!("{:.1}M", n as f64 / M as f64)
    } else if n >= K {
        format!("{:.1}K", n as f64 / K as f64)
    } else {
        n.to_string()
    }
}

pub fn mods_panel(
    ui: &mut egui::Ui,
    state: &mut ModsState,
    default_mods_dir: &Path,
    game_dir: &Path,
    mc_version: Option<&str>,
    loader: Option<&str>,
) {
    if !state.manual_path {
        state.point_at(default_mods_dir);
    }

    let pack_key = game_dir.to_string_lossy().to_string();
    if state.pack_key.as_deref() != Some(pack_key.as_str()) {
        state.pack_key = Some(pack_key);
        state.pack_status = read_pack_status(&game_dir.join("mods"));
    }

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Mods")
                .size(24.0)
                .color(theme::ACCENT_CYAN),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!("{} jar(s)", state.mods.len()))
                .size(12.0)
                .color(theme::TEXT_SECONDARY),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::icon_only_button(
                ui,
                fonts::ICON_REFRESH,
                "Rescan the folder",
                theme::ButtonKind::Secondary,
                egui::vec2(30.0, 28.0),
                true,
            )
            .clicked()
            {
                state.refresh();
            }
            ui.add_space(8.0);
            if theme::icon_button(
                ui,
                fonts::ICON_UPLOAD,
                "Add mod",
                theme::ButtonKind::Primary,
                true,
            )
            .clicked()
            {
                state.add_files();
            }
        });
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let installed_label = format!("Installed ({})", state.mods.len());
        if theme::pill(ui, &installed_label, state.tab == ModsTab::Installed).clicked() {
            state.tab = ModsTab::Installed;
            state.browse_error = None;
        }
        if theme::pill(ui, "Browse", state.tab == ModsTab::Browse).clicked() {
            state.tab = ModsTab::Browse;
            state.browse_error = None;
        }
    });
    ui.add_space(8.0);

    if let Some(notice) = &state.notice {
        ui.label(egui::RichText::new(format!("✓ {notice}")).color(theme::SUCCESS));
        ui.add_space(4.0);
    }
    if let Some(err) = &state.error {
        ui.label(egui::RichText::new(format!("⚠ {err}")).color(theme::ORANGE));
        ui.add_space(4.0);
    }

    match state.tab {
        ModsTab::Installed => installed_tab(ui, state, game_dir),
        ModsTab::Browse => browse_tab(ui, state, mc_version, loader, default_mods_dir),
    }
}

fn installed_tab(ui: &mut egui::Ui, state: &mut ModsState, game_dir: &Path) {
    theme::glass_card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Folder").color(theme::TEXT_SECONDARY));
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.path)
                    .desired_width(420.0)
                    .hint_text("path to a mods folder"),
            );
            if response.changed() {
                state.loaded = false;
                state.manual_path = true;
                state.pending_delete = None;
            }
            if ui.small_button("Go").clicked() {
                state.refresh();
            }
            if state.manual_path
                && theme::icon_button(
                    ui,
                    fonts::ICON_FOLDER_OPEN,
                    "Follow instance",
                    theme::ButtonKind::Ghost,
                    true,
                )
                .clicked()
            {
                state.manual_path = false;
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("Game directory: {}", game_dir.display()))
                .size(11.0)
                .color(theme::BORDER),
        );
        ui.label(
            egui::RichText::new("Drag & drop .jar files anywhere in this list, or press Add mod.")
                .size(11.0)
                .color(theme::BORDER),
        );
        if let Some(status) = &state.pack_status {
            ui.label(egui::RichText::new(status).size(11.0).color(theme::SUCCESS));
        }
    });

    ui.add_space(12.0);

    let dropped: Vec<PathBuf> = ui.ctx().input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|f| f.path.clone())
            .collect()
    });
    if !dropped.is_empty() {
        state.import_paths(dropped);
    }

    if state.mods.is_empty() {
        theme::empty_state(
            ui,
            "No mods installed.",
            "Drop .jar files here, press Add mod, or open the Browse tab to search online.",
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("mods_list")
        .show(ui, |ui| {
            let rows: Vec<(String, u64)> = state
                .mods
                .iter()
                .map(|m| (m.name.clone(), m.size))
                .collect();
            for (name, size) in rows {
                let armed = state.pending_delete.as_deref() == Some(name.as_str());
                theme::glass_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&name).size(14.0).color(if armed {
                            theme::DANGER
                        } else {
                            theme::TEXT_PRIMARY
                        }));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let tip = if armed {
                                "Click again to delete"
                            } else {
                                "Remove from this instance"
                            };
                            if theme::icon_only_button(
                                ui,
                                fonts::ICON_DELETE,
                                tip,
                                if armed {
                                    theme::ButtonKind::Danger
                                } else {
                                    theme::ButtonKind::Secondary
                                },
                                egui::vec2(30.0, 26.0),
                                true,
                            )
                            .clicked()
                            {
                                if armed {
                                    if let Err(e) = state.delete_mod(&name) {
                                        state.error = Some(e);
                                    }
                                } else {
                                    state.pending_delete = Some(name.clone());
                                }
                            }
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(human_size(size))
                                    .size(11.0)
                                    .color(theme::TEXT_SECONDARY),
                            );
                        });
                    });
                });
                ui.add_space(4.0);
            }
        });
}

fn browse_tab(
    ui: &mut egui::Ui,
    state: &mut ModsState,
    mc_version: Option<&str>,
    loader: Option<&str>,
    mods_dir: &Path,
) {
    ui.horizontal(|ui| {
        if theme::pill(
            ui,
            "Modrinth",
            state.browse_source == BrowseSource::Modrinth,
        )
        .clicked()
        {
            state.browse_source = BrowseSource::Modrinth;
            state.results.clear();
            state.browse_error = None;
            state.search_busy = false;
        }
        if theme::pill(
            ui,
            "CurseForge",
            state.browse_source == BrowseSource::CurseForge,
        )
        .clicked()
        {
            state.browse_source = BrowseSource::CurseForge;
            state.results.clear();
            state.browse_error = None;
            state.search_busy = false;
        }
        ui.add_space(12.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .desired_width(280.0)
                .hint_text("Search mods…"),
        );
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if theme::icon_button(
            ui,
            fonts::ICON_SEARCH,
            "Search",
            theme::ButtonKind::Primary,
            !state.search_busy,
        )
        .clicked()
            || enter_pressed
        {
            state.start_search(mc_version, loader);
        }
    });

    ui.add_space(4.0);
    let context_label = match (mc_version, loader) {
        (Some(mc), Some(l)) => format!("For {mc} · {l}"),
        (Some(mc), None) => format!("For {mc}"),
        _ => "No instance selected — unfiltered results".to_string(),
    };
    ui.label(
        egui::RichText::new(context_label)
            .size(11.0)
            .color(theme::BORDER),
    );

    if state.browse_source == BrowseSource::CurseForge {
        ui.add_space(6.0);
        theme::glass_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("API key").color(theme::TEXT_SECONDARY));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.cf_key)
                        .desired_width(340.0)
                        .hint_text("free key from console.curseforge.com"),
                );
                if response.changed() {
                    let key = state.cf_key.clone();
                    state.set_cf_key(key);
                }
            });
            ui.label(
                egui::RichText::new(
                    "CurseForge requires a free API key; Modrinth works without one.",
                )
                .size(11.0)
                .color(theme::BORDER),
            );
        });
        ui.add_space(6.0);
    }

    if let Some(err) = &state.browse_error {
        ui.label(egui::RichText::new(format!("⚠ {err}")).color(theme::ORANGE));
        ui.add_space(6.0);
    }

    let auto_key = format!(
        "{:?}:{}:{}:{}",
        state.browse_source,
        mc_version.unwrap_or(""),
        loader.unwrap_or(""),
        !state.cf_key.trim().is_empty()
    );
    if state.auto_search.as_deref() != Some(auto_key.as_str()) {
        let first = state.results.is_empty();
        state.auto_search = Some(auto_key);
        if first && !state.search_busy {
            state.start_search(mc_version, loader);
        }
    }

    if state.search_busy {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!(
                "Searching {}…",
                match state.browse_source {
                    BrowseSource::Modrinth => "Modrinth",
                    BrowseSource::CurseForge => "CurseForge",
                }
            ))
            .size(14.0)
            .color(theme::TEXT_SECONDARY),
        );
        return;
    }

    if state.results.is_empty() {
        theme::empty_state(
            ui,
            "No results.",
            "Try a shorter query — or another source pill above.",
        );
        return;
    }

    let results: Vec<(ModHit, bool)> = state
        .results
        .iter()
        .map(|h| {
            (
                h.clone(),
                state.installing.as_deref() == Some(h.project_id.as_str()),
            )
        })
        .collect();
    let any_installing = state.installing.is_some();

    egui::ScrollArea::vertical()
        .id_salt("browse_results")
        .show(ui, |ui| {
            for (hit, busy) in results {
                theme::glass_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&hit.title)
                                        .size(15.0)
                                        .color(theme::TEXT_PRIMARY),
                                );
                                if !hit.author.is_empty() {
                                    ui.label(
                                        egui::RichText::new(format!("by {}", hit.author))
                                            .size(12.0)
                                            .color(theme::TEXT_SECONDARY),
                                    );
                                }
                                ui.add_space(6.0);
                                theme::badge(ui, &human_count(hit.downloads), theme::SUCCESS);
                            });
                            ui.label(
                                egui::RichText::new(&hit.description)
                                    .size(12.0)
                                    .color(theme::TEXT_DIM),
                            );
                            let source_name = match hit.source {
                                BrowseSource::Modrinth => "Modrinth",
                                BrowseSource::CurseForge => "CurseForge",
                            };
                            ui.label(
                                egui::RichText::new(format!("{source_name} · {}", hit.slug))
                                    .size(11.0)
                                    .color(theme::BORDER),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if busy {
                                ui.label(
                                    egui::RichText::new("Installing…")
                                        .size(13.0)
                                        .color(theme::ACCENT_2),
                                );
                            } else if theme::icon_button(
                                ui,
                                fonts::ICON_DOWNLOAD,
                                "Install",
                                theme::ButtonKind::Primary,
                                !any_installing,
                            )
                            .clicked()
                            {
                                state.start_install(&hit, mc_version, loader, mods_dir);
                            }
                        });
                    });
                });
                ui.add_space(4.0);
            }
        });
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_lists_only_jars() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sodium.jar"), b"jar").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"text").unwrap();
        std::fs::create_dir(dir.path().join("sub.jar")).unwrap();

        let mut state = ModsState {
            path: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        state.refresh();

        assert_eq!(state.mods.len(), 1);
        assert_eq!(state.mods[0].name, "sodium.jar");
        assert!(state.error.is_none());
    }

    #[test]
    fn missing_folder_reports_an_error() {
        let mut state = ModsState {
            path: "/definitely/not/a/dir".to_string(),
            ..Default::default()
        };
        state.refresh();
        assert!(state.mods.is_empty());
        assert!(state.error.is_some());
    }

    #[test]
    fn human_size_formats_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn human_count_formats_units() {
        assert_eq!(human_count(999), "999");
        assert_eq!(human_count(12_300), "12.3K");
        assert_eq!(human_count(5_200_000), "5.2M");
    }

    #[test]
    fn import_copies_and_keeps_names_unique() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("sodium.jar"), b"one").unwrap();

        let added = copy_into(dest.path(), &[src.path().join("sodium.jar")]).unwrap();
        assert_eq!(added, 1);
        assert!(dest.path().join("sodium.jar").is_file());

        let added = copy_into(dest.path(), &[src.path().join("sodium.jar")]).unwrap();
        assert_eq!(added, 1);
        assert!(dest.path().join("sodium (1).jar").is_file());
        assert_eq!(
            std::fs::read(dest.path().join("sodium.jar")).unwrap(),
            b"one"
        );
    }

    #[test]
    fn delete_only_removes_plain_files_inside_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.jar"), b"jar").unwrap();

        let mut state = ModsState {
            path: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        state.refresh();

        assert!(state.delete_mod("../evil.jar").is_err());
        assert!(dir.path().join("bad.jar").is_file());

        state.delete_mod("bad.jar").unwrap();
        assert!(!dir.path().join("bad.jar").exists());
        assert!(state.mods.is_empty());
        assert!(state.notice.is_some());
    }

    #[test]
    fn import_paths_rejects_non_jars() {
        let dir = tempfile::tempdir().unwrap();
        let txt = tempfile::tempdir().unwrap();
        std::fs::write(txt.path().join("readme.txt"), b"hi").unwrap();

        let mut state = ModsState {
            path: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        state.import_paths(vec![txt.path().join("readme.txt")]);
        assert!(state.mods.is_empty());
        assert!(state.error.as_deref().is_some_and(|e| e.contains(".jar")));
    }

    #[test]
    fn curseforge_search_without_key_fails_fast() {
        let mut state = ModsState {
            browse_source: BrowseSource::CurseForge,
            ..Default::default()
        };
        state.start_search(Some("1.21.11"), Some("fabric"));
        assert!(!state.search_busy, "must not spawn a doomed request");
        assert!(state
            .browse_error
            .as_deref()
            .is_some_and(|e| e.contains("API key")));
        assert!(state.rx.is_none());
    }

    #[test]
    fn poll_applies_results_and_install_completion() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut state = ModsState {
            rx: Some(rx),
            search_busy: true,
            installing: Some("p1".to_string()),
            ..Default::default()
        };

        tx.send(BrowseEvent::Results(vec![ModHit {
            source: BrowseSource::Modrinth,
            project_id: "p1".to_string(),
            slug: "sodium".to_string(),
            title: "Sodium".to_string(),
            author: "jellysquid".to_string(),
            description: "rendering".to_string(),
            downloads: 100,
            icon_url: None,
        }]))
        .unwrap();
        state.poll();
        assert!(!state.search_busy);
        assert_eq!(state.results.len(), 1);
        assert_eq!(state.results[0].title, "Sodium");

        tx.send(BrowseEvent::Installed {
            title: "Sodium".to_string(),
            filename: "sodium.jar".to_string(),
        })
        .unwrap();
        state.poll();
        assert!(state.installing.is_none());
        assert!(state
            .notice
            .as_deref()
            .is_some_and(|n| n.contains("sodium.jar")));
    }

    #[test]
    fn set_cf_key_flags_persistence_once() {
        let mut state = ModsState::default();
        state.set_cf_key("abc".to_string());
        assert_eq!(state.persist_cf_key.as_deref(), Some("abc"));
        state.persist_cf_key = None;
        state.set_cf_key("abc".to_string());
        assert!(state.persist_cf_key.is_none(), "same key must not re-save");
    }

    #[test]
    fn available_destination_keeps_the_bare_name_when_free() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            available_destination(dir.path(), OsStr::new("sodium.jar")),
            dir.path().join("sodium.jar")
        );
        std::fs::write(dir.path().join("sodium.jar"), b"x").unwrap();
        assert_eq!(
            available_destination(dir.path(), OsStr::new("sodium.jar")),
            dir.path().join("sodium (1).jar")
        );
    }

    #[test]
    fn worker_death_never_leaves_the_ui_spinning() {
        let (tx, rx) = std::sync::mpsc::channel::<BrowseEvent>();
        let mut state = ModsState {
            rx: Some(rx),
            search_busy: true,
            installing: Some("p9".to_string()),
            ..Default::default()
        };
        drop(tx);
        state.poll();
        assert!(!state.search_busy);
        assert!(state.installing.is_none());
        assert!(state.browse_error.is_some());
        assert!(state.rx.is_none());
        assert!(!state.is_busy(), "idle repaint policy must resume");
    }

    #[test]
    fn pack_status_reads_the_managed_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let manifest = launcher_core::perf::ManagedManifest {
            mc_version: "1.21.11".to_string(),
            loader: "fabric".to_string(),
            renderer: "vulkan".to_string(),
            installed: vec![launcher_core::perf::InstalledMod {
                slug: "vulkanmod".to_string(),
                name: "VulkanMod".to_string(),
                version: "0.6.8".to_string(),
                filename: "vulkanmod.jar".to_string(),
            }],
            skipped: vec![],
        };
        std::fs::write(
            launcher_core::perf::ManagedManifest::path(&mods),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();

        let status = read_pack_status(&mods).expect("manifest should be read");
        assert!(status.contains("VulkanMod 0.6.8"));
        assert!(status.contains("1.21.11 fabric"));
        assert!(status.contains("vulkan"));

        assert!(read_pack_status(&dir.path().join("nope")).is_none());
    }
}
