use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";

pub fn api_base_url() -> String {
    let base = std::env::var("AETHEL_API_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API_URL.to_string());
    base.trim_end_matches('/').to_string()
}

const CACHE_FILE: &str = "cosmetics-cache.json";
const SESSION_FILE: &str = "session.json";
const EQUIPPED_FILE: &str = "equipped.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformSession {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: String,
    pub username: String,
    pub role: String,
}

impl PlatformSession {
    pub fn access_expired(&self) -> bool {
        self.access_expired_within(chrono::Duration::zero())
    }

    pub fn access_expired_within(&self, leeway: chrono::Duration) -> bool {
        match chrono::DateTime::parse_from_rfc3339(&self.expires_at) {
            Ok(expires) => chrono::Utc::now() + leeway >= expires.with_timezone(&chrono::Utc),
            Err(_) => true,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AuthTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    username: String,
    role: String,
}

impl AuthTokenResponse {
    fn into_session(self) -> PlatformSession {
        let expires_in = self.expires_in;
        PlatformSession {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: (chrono::Utc::now() + chrono::Duration::seconds(expires_in)).to_rfc3339(),
            username: self.username,
            role: self.role,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cosmetic {
    pub slug: String,
    pub name: String,
    pub kind: String,
    pub rarity: String,
    pub asset_url: String,
    pub tint_hex: String,
    pub price_cents: i32,
    #[serde(default)]
    pub sort_order: i32,
}

impl Cosmetic {
    pub fn is_free(&self) -> bool {
        self.price_cents <= 0
    }

    pub fn price_label(&self) -> String {
        if self.is_free() {
            "Free".to_string()
        } else {
            format!("{:.2}", self.price_cents as f64 / 100.0)
        }
    }

    pub fn tint_rgb(&self) -> [u8; 3] {
        parse_hex(&self.tint_hex).unwrap_or([108, 92, 231])
    }

    pub fn rarity_hex(&self) -> &'static str {
        rarity_hex(&self.rarity)
    }
}

pub const RARITY_COMMON: &str = "#95A5A6";
pub const RARITY_RARE: &str = "#00D2FF";
pub const RARITY_EPIC: &str = "#9B59B6";
pub const RARITY_LEGENDARY: &str = "#F1C40F";

pub fn rarity_hex(rarity: &str) -> &'static str {
    match rarity {
        "legendary" => RARITY_LEGENDARY,
        "epic" => RARITY_EPIC,
        "rare" => RARITY_RARE,
        _ => RARITY_COMMON,
    }
}

pub fn rarity_rgb(rarity: &str) -> [u8; 3] {
    parse_hex(rarity_hex(rarity)).unwrap_or([0x95, 0xA5, 0xA6])
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub balance_cents: i32,
    #[serde(default)]
    pub owned: Vec<String>,
    #[serde(default)]
    pub equipped: BTreeMap<String, String>,
}

impl Inventory {
    pub fn owns(&self, slug: &str) -> bool {
        self.owned.iter().any(|s| s == slug)
    }

    pub fn is_equipped(&self, slug: &str) -> bool {
        self.equipped.values().any(|s| s == slug)
    }

    pub fn balance_label(&self) -> String {
        format!("{:.2}", self.balance_cents as f64 / 100.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Unknown,
    Online,
    Offline,
}

/// One worn piece. `slot` mirrors the vanilla slot it renders in, so two capes
/// can never be equipped at once and wings/elytra share one slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquippedItem {
    pub slug: String,
    pub kind: String,
    pub name: String,
    pub asset_url: String,
    pub slot: String,
}

/// The offline-first equipped set, mirrored to the in-game client over IPC and
/// readable from `platform/equipped.json` by `aethel-cosmetics` directly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquippedState {
    #[serde(default)]
    pub items: Vec<EquippedItem>,
    #[serde(default)]
    pub updated_at: String,
}

impl EquippedState {
    pub fn slot(&self, slot: &str) -> Option<&EquippedItem> {
        self.items.iter().find(|i| i.slot == slot)
    }

    pub fn slugs(&self) -> Vec<String> {
        self.items.iter().map(|i| i.slug.clone()).collect()
    }

    pub fn contains(&self, slug: &str) -> bool {
        self.items.iter().any(|i| i.slug == slug)
    }

    pub fn documents(&self) -> Vec<serde_json::Value> {
        self.items
            .iter()
            .map(|i| {
                serde_json::json!({
                    "slug": i.slug,
                    "kind": i.kind,
                    "name": i.name,
                    "assetUrl": i.asset_url,
                    "slot": i.slot,
                })
            })
            .collect()
    }
}

/// The vanilla slot a catalogue kind renders in.
pub fn slot_for_kind(kind: &str) -> String {
    match kind {
        "cape" => "cape".to_string(),
        "elytra" | "wings" => "wings".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PurchaseOutcome {
    Purchased { balance_cents: i32 },
    AlreadyPurchased,
    InsufficientFunds { balance_cents: i32 },
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
}

pub fn builtin_catalogue() -> Vec<Cosmetic> {
    let entry =
        |slug: &str, name: &str, kind: &str, rarity: &str, tint: &str, price: i32, sort: i32| {
            Cosmetic {
                slug: slug.to_string(),
                name: name.to_string(),
                kind: kind.to_string(),
                rarity: rarity.to_string(),
                asset_url: format!(
                    "{kind}s/{}.png",
                    slug.split('-').next_back().unwrap_or(slug)
                ),
                tint_hex: tint.to_string(),
                price_cents: price,
                sort_order: sort,
            }
        };

    vec![
        entry(
            "cape-aethel",
            "Aethel Cape",
            "cape",
            "common",
            "#6C5CE7",
            0,
            10,
        ),
        entry(
            "cape-aurora",
            "Aurora Cape",
            "cape",
            "rare",
            "#00D2FF",
            250,
            20,
        ),
        entry(
            "cape-ember",
            "Ember Cape",
            "cape",
            "epic",
            "#E74C3C",
            500,
            30,
        ),
        entry(
            "wings-void",
            "Void Wings",
            "wings",
            "legendary",
            "#8E44AD",
            900,
            40,
        ),
        entry(
            "wings-prism",
            "Prism Wings",
            "wings",
            "epic",
            "#00D2FF",
            700,
            50,
        ),
        entry(
            "badge-founder",
            "Founder Badge",
            "badge",
            "legendary",
            "#F1C40F",
            0,
            60,
        ),
        entry(
            "badge-early",
            "Early Adopter",
            "badge",
            "rare",
            "#2ECC71",
            0,
            70,
        ),
        entry(
            "badge-bugfinder",
            "Bug Finder",
            "badge",
            "epic",
            "#E67E22",
            0,
            80,
        ),
        entry(
            "hud-minimal",
            "Minimal HUD",
            "hud",
            "common",
            "#95A5A6",
            0,
            90,
        ),
        entry("hud-neon", "Neon HUD", "hud", "rare", "#00D2FF", 300, 100),
        entry(
            "elytra-dragon",
            "Dragon Elytra",
            "elytra",
            "legendary",
            "#111111",
            1200,
            110,
        ),
        entry(
            "skin-shadow",
            "Shadow Skin Accent",
            "skin",
            "rare",
            "#2C3E50",
            200,
            120,
        ),
    ]
}

#[derive(Debug, Serialize, Deserialize)]
struct DiskCache {
    cosmetics: Vec<Cosmetic>,
    #[serde(default)]
    inventory: Inventory,
    fetched_at: String,
}

#[derive(Clone)]
pub struct PlatformClient {
    base_url: String,
    http: reqwest::Client,
    cache_path: PathBuf,
    session_path: PathBuf,
    equipped_path: PathBuf,
    session: Arc<parking_lot::RwLock<Option<PlatformSession>>>,
    catalogue: Arc<parking_lot::Mutex<Option<Vec<Cosmetic>>>>,
}

impl std::fmt::Debug for PlatformClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl PlatformClient {
    pub fn new(data_dir: &Path) -> Self {
        let base_url = api_base_url();

        let http = reqwest::Client::builder()
            .user_agent(concat!("AethelLauncher/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .unwrap_or_default();

        let platform_dir = data_dir.join("platform");
        let session_path = platform_dir.join(SESSION_FILE);
        let session = read_session(&session_path);

        Self {
            base_url,
            http,
            cache_path: platform_dir.join(CACHE_FILE),
            session_path,
            equipped_path: platform_dir.join(EQUIPPED_FILE),
            session: Arc::new(parking_lot::RwLock::new(session)),
            catalogue: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    pub fn equipped_path(&self) -> &Path {
        &self.equipped_path
    }

    pub fn equipped_state(&self) -> EquippedState {
        std::fs::read_to_string(&self.equipped_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn equipped_documents(&self) -> Vec<serde_json::Value> {
        self.equipped_state().documents()
    }

    fn write_equipped(&self, state: &EquippedState) -> EquippedState {
        let mut state = state.clone();
        state.updated_at = chrono::Utc::now().to_rfc3339();
        if let Some(parent) = self.equipped_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec_pretty(&state) {
            Ok(raw) => {
                if let Err(e) = std::fs::write(&self.equipped_path, raw) {
                    tracing::debug!("could not persist equipped cosmetics: {e}");
                }
            }
            Err(e) => tracing::debug!("could not serialise equipped cosmetics: {e}"),
        }
        state
    }

    fn item_from_catalogue(&self, slug: &str) -> Option<EquippedItem> {
        self.cached_catalogue()
            .into_iter()
            .find(|c| c.slug == slug)
            .map(|c| EquippedItem {
                slug: c.slug.clone(),
                kind: c.kind.clone(),
                name: c.name.clone(),
                asset_url: c.asset_url.clone(),
                slot: slot_for_kind(&c.kind),
            })
    }

    /// Equips locally (one piece per slot) without a backend round-trip, so the
    /// in-game client can render cosmetics offline.
    pub fn equip_local(&self, slug: &str) -> EquippedState {
        let Some(item) = self.item_from_catalogue(slug) else {
            return self.equipped_state();
        };
        let mut state = self.equipped_state();
        state.items.retain(|i| i.slot != item.slot);
        state.items.push(item);
        state.items.sort_by(|a, b| a.slot.cmp(&b.slot));
        self.write_equipped(&state)
    }

    pub fn unequip_local(&self, slug: &str) -> EquippedState {
        let mut state = self.equipped_state();
        state.items.retain(|i| i.slug != slug);
        self.write_equipped(&state)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn session(&self) -> Option<PlatformSession> {
        self.session.read().clone()
    }

    pub fn is_signed_in(&self) -> bool {
        self.session.read().is_some()
    }

    pub fn session_username(&self) -> String {
        self.session
            .read()
            .as_ref()
            .map(|s| s.username.clone())
            .unwrap_or_default()
    }

    pub async fn login(&self, email: &str, password: &str) -> anyhow::Result<PlatformSession> {
        let session = self
            .auth_call(
                "login",
                &serde_json::json!({ "email": email, "password": password }),
            )
            .await?;
        self.store_session(session.clone());
        Ok(session)
    }

    pub async fn register(
        &self,
        email: &str,
        username: &str,
        password: &str,
    ) -> anyhow::Result<PlatformSession> {
        let session = self
            .auth_call(
                "register",
                &serde_json::json!({
                    "email": email,
                    "username": username,
                    "password": password,
                }),
            )
            .await?;
        self.store_session(session.clone());
        Ok(session)
    }

    pub async fn logout(&self) {
        let session = self.session.read().clone();
        if let Some(session) = session {
            let url = format!("{}/api/v1/auth/logout", self.base_url);
            let _ = self
                .http
                .post(&url)
                .json(&serde_json::json!({ "refresh_token": session.refresh_token }))
                .send()
                .await;
        }
        self.clear_session();
    }

    pub fn clear_session(&self) {
        *self.session.write() = None;
        if self.session_path.exists() {
            let _ = std::fs::remove_file(&self.session_path);
        }
    }

    pub fn clear_cached_inventory(&self) {
        self.write_cache(&self.cached_catalogue(), Inventory::default());
    }

    async fn auth_call(
        &self,
        action: &str,
        body: &serde_json::Value,
    ) -> anyhow::Result<PlatformSession> {
        let url = format!("{}/api/v1/auth/{action}", self.base_url);
        let response = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("could not reach {url}"))?;

        let status = response.status();
        let payload: serde_json::Value = response
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({}));
        if !status.is_success() {
            let message = payload["message"]
                .as_str()
                .unwrap_or(match status.as_u16() {
                    401 => "invalid email or password",
                    409 => "that email or username is already registered",
                    503 => "sign-in needs the backend database — is it running?",
                    _ => "sign-in failed",
                })
                .to_string();
            anyhow::bail!(message);
        }

        let tokens: AuthTokenResponse = serde_json::from_value(payload)
            .context("backend returned an unexpected sign-in payload")?;
        Ok(tokens.into_session())
    }

    async fn refresh_session(&self) -> anyhow::Result<()> {
        let Some(current) = self.session.read().clone() else {
            anyhow::bail!("not signed in");
        };
        let url = format!("{}/api/v1/auth/refresh", self.base_url);
        let response = self
            .http
            .post(&url)
            .json(&serde_json::json!({ "refresh_token": current.refresh_token }))
            .send()
            .await
            .with_context(|| format!("could not reach {url}"))?;

        if !response.status().is_success() {
            self.clear_session();
            anyhow::bail!("session expired — please sign in again");
        }

        let payload: serde_json::Value = response.json().await?;
        let tokens: AuthTokenResponse =
            serde_json::from_value(payload).context("unexpected refresh payload")?;
        let mut session = tokens.into_session();
        if session.refresh_token.is_empty() {
            session.refresh_token = current.refresh_token;
        }
        self.store_session(session);
        Ok(())
    }

    async fn ensure_fresh_access(&self) -> anyhow::Result<String> {
        let session = self
            .session
            .read()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("sign in on the Account screen first"))?;
        if session.access_expired_within(chrono::Duration::seconds(30)) {
            self.refresh_session().await?;
            return Ok(self
                .session
                .read()
                .clone()
                .map(|s| s.access_token)
                .unwrap_or_default());
        }
        Ok(session.access_token)
    }

    async fn authed_get(&self, url: &str) -> anyhow::Result<reqwest::Response> {
        let token = self.ensure_fresh_access().await?;
        let response = self.http.get(url).bearer_auth(&token).send().await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.refresh_session().await?;
            let token = self
                .session
                .read()
                .clone()
                .map(|s| s.access_token)
                .unwrap_or_default();
            return Ok(self.http.get(url).bearer_auth(&token).send().await?);
        }
        Ok(response)
    }

    async fn authed_post(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> anyhow::Result<reqwest::Response> {
        let token = self.ensure_fresh_access().await?;
        let response = self
            .http
            .post(url)
            .bearer_auth(&token)
            .json(body)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.refresh_session().await?;
            let token = self
                .session
                .read()
                .clone()
                .map(|s| s.access_token)
                .unwrap_or_default();
            return Ok(self
                .http
                .post(url)
                .bearer_auth(&token)
                .json(body)
                .send()
                .await?);
        }
        Ok(response)
    }

    fn store_session(&self, session: PlatformSession) {
        *self.session.write() = Some(session.clone());
        if let Some(parent) = self.session_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec_pretty(&session) {
            Ok(raw) => {
                if let Err(e) = std::fs::write(&self.session_path, raw) {
                    tracing::warn!("could not persist platform session: {e}");
                    return;
                }
                restrict_permissions(&self.session_path);
            }
            Err(e) => tracing::warn!("could not serialise platform session: {e}"),
        }
    }

    pub fn cached_catalogue(&self) -> Vec<Cosmetic> {
        if let Some(cached) = self.catalogue.lock().clone() {
            return cached;
        }
        if let Some(disk) = self.read_cache() {
            if !disk.cosmetics.is_empty() {
                *self.catalogue.lock() = Some(disk.cosmetics.clone());
                return disk.cosmetics;
            }
        }
        builtin_catalogue()
    }

    pub fn cached_inventory(&self) -> Inventory {
        self.read_cache().map(|c| c.inventory).unwrap_or_default()
    }

    pub async fn health(&self) -> Connection {
        let url = format!("{}/api/v1/health", self.base_url);
        match self.http.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(body) if body["database"] == "ok" => Connection::Online,
                    Ok(_) => Connection::Offline,
                    Err(_) => Connection::Offline,
                }
            }
            _ => Connection::Offline,
        }
    }

    pub async fn fetch_catalogue(&self) -> anyhow::Result<Vec<Cosmetic>> {
        let url = format!("{}/api/v1/cosmetics", self.base_url);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("could not reach {url}"))?
            .error_for_status()
            .context("backend rejected the catalogue request")?;

        let cosmetics: Vec<Cosmetic> = response
            .json()
            .await
            .context("backend returned an unexpected catalogue payload")?;

        if cosmetics.is_empty() {
            anyhow::bail!("backend returned an empty catalogue");
        }

        *self.catalogue.lock() = Some(cosmetics.clone());
        self.write_cache(&cosmetics, self.cached_inventory());
        Ok(cosmetics)
    }

    pub async fn fetch_inventory(&self) -> anyhow::Result<Inventory> {
        let url = format!("{}/api/v1/cosmetics/mine", self.base_url);
        let response = self
            .authed_get(&url)
            .await
            .with_context(|| format!("could not reach {url}"))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("session expired — sign in again on the Account screen");
        }
        let status = response.status();
        let inventory: Inventory = response
            .json()
            .await
            .context("backend returned an unexpected inventory payload")?;
        if !status.is_success() {
            anyhow::bail!("backend rejected the inventory request ({status})");
        }

        self.write_cache(&self.cached_catalogue(), inventory.clone());
        Ok(inventory)
    }

    pub async fn purchase(&self, slug: &str, request_id: &str) -> anyhow::Result<PurchaseOutcome> {
        let url = format!(
            "{}/api/v1/cosmetics/{}/purchase",
            self.base_url,
            urlencode(slug)
        );
        let response = self
            .authed_post(&url, &serde_json::json!({ "request_id": request_id }))
            .await
            .with_context(|| format!("could not reach {url}"))?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({}));

        if status == reqwest::StatusCode::PAYMENT_REQUIRED {
            return Ok(PurchaseOutcome::InsufficientFunds {
                balance_cents: body["balance_cents"].as_i64().unwrap_or(0) as i32,
            });
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("session expired — sign in again on the Account screen");
        }
        if !status.is_success() {
            let message = body["message"]
                .as_str()
                .unwrap_or("purchase failed")
                .to_string();
            anyhow::bail!("{message}");
        }

        let result = body["status"].as_str().unwrap_or("purchased");
        Ok(match result {
            "already_purchased" => PurchaseOutcome::AlreadyPurchased,
            _ => PurchaseOutcome::Purchased {
                balance_cents: body["balance_cents"].as_i64().unwrap_or(0) as i32,
            },
        })
    }

    pub async fn equip(&self, slug: &str) -> anyhow::Result<Inventory> {
        self.post_player_action("equip", slug).await
    }

    pub async fn unequip(&self, slug: &str) -> anyhow::Result<Inventory> {
        self.post_player_action("unequip", slug).await
    }

    async fn post_player_action(&self, action: &str, slug: &str) -> anyhow::Result<Inventory> {
        let url = format!(
            "{}/api/v1/cosmetics/{}/{}",
            self.base_url,
            urlencode(slug),
            action
        );
        let response = self
            .authed_post(&url, &serde_json::json!({}))
            .await
            .with_context(|| format!("could not reach {url}"))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("session expired — sign in again on the Account screen");
        }
        if !response.status().is_success() {
            let body: serde_json::Value = response
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({}));
            let message = body["message"]
                .as_str()
                .unwrap_or("the backend rejected the request")
                .to_string();
            anyhow::bail!("{message}");
        }

        let inventory: Inventory = response.json().await?;
        self.write_cache(&self.cached_catalogue(), inventory.clone());
        Ok(inventory)
    }

    fn read_cache(&self) -> Option<DiskCache> {
        let raw = std::fs::read_to_string(&self.cache_path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn write_cache(&self, cosmetics: &[Cosmetic], inventory: Inventory) {
        let cache = DiskCache {
            cosmetics: cosmetics.to_vec(),
            inventory,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Some(parent) = self.cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec_pretty(&cache) {
            Ok(raw) => {
                if let Err(e) = std::fs::write(&self.cache_path, raw) {
                    tracing::debug!("could not cache cosmetics: {e}");
                }
            }
            Err(e) => tracing::debug!("could not serialise cosmetics cache: {e}"),
        }
    }
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn read_session(path: &Path) -> Option<PlatformSession> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalogue_matches_the_sql_seed() {
        let catalogue = builtin_catalogue();
        assert_eq!(catalogue.len(), 12);
        assert!(catalogue.iter().any(|c| c.slug == "cape-aethel"));
        assert!(catalogue.iter().any(|c| c.slug == "elytra-dragon"));
        for c in &catalogue {
            assert!(parse_hex(&c.tint_hex).is_some(), "bad tint: {}", c.slug);
            assert!(!c.name.is_empty());
        }
    }

    #[test]
    fn prices_render_as_currency_and_free() {
        let free = Cosmetic {
            price_cents: 0,
            ..builtin_catalogue()[0].clone()
        };
        assert_eq!(free.price_label(), "Free");
        assert!(free.is_free());

        let paid = Cosmetic {
            price_cents: 250,
            ..builtin_catalogue()[1].clone()
        };
        assert_eq!(paid.price_label(), "2.50");
        assert!(!paid.is_free());
    }

    #[test]
    fn tint_parsing_handles_hashes_and_garbage() {
        assert_eq!(parse_hex("#00D2FF"), Some([0, 210, 255]));
        assert_eq!(parse_hex("6C5CE7"), Some([108, 92, 231]));
        assert_eq!(parse_hex("nope"), None);
        let bad = Cosmetic {
            tint_hex: "garbage".to_string(),
            ..builtin_catalogue()[0].clone()
        };
        assert_eq!(bad.tint_rgb(), [108, 92, 231]);
    }

    #[test]
    fn rarity_palette_is_canonical_and_falls_back() {
        assert_eq!(rarity_hex("legendary"), RARITY_LEGENDARY);
        assert_eq!(rarity_hex("epic"), RARITY_EPIC);
        assert_eq!(rarity_hex("rare"), RARITY_RARE);
        assert_eq!(rarity_hex("common"), RARITY_COMMON);
        assert_eq!(rarity_hex("mythic"), RARITY_COMMON);

        assert_eq!(rarity_rgb("legendary"), [241, 196, 15]);
        assert_eq!(rarity_rgb("common"), [149, 165, 166]);
        assert_eq!(rarity_rgb("mythic"), [149, 165, 166]);

        for c in builtin_catalogue() {
            assert!(
                parse_hex(c.rarity_hex()).is_some(),
                "bad rarity: {}",
                c.slug
            );
        }
    }

    #[test]
    fn inventory_ownership_helpers() {
        let mut inv = Inventory {
            balance_cents: 500,
            owned: vec!["cape-aethel".to_string()],
            ..Default::default()
        };
        inv.equipped
            .insert("cape".to_string(), "cape-aethel".to_string());

        assert!(inv.owns("cape-aethel"));
        assert!(!inv.owns("wings-void"));
        assert!(inv.is_equipped("cape-aethel"));
        assert!(!inv.is_equipped("wings-void"));
        assert_eq!(inv.balance_label(), "5.00");
    }

    #[test]
    fn fresh_install_falls_back_to_the_seed_catalogue() {
        let dir = tempfile::tempdir().unwrap();
        let client = PlatformClient::new(dir.path());
        let catalogue = client.cached_catalogue();
        assert_eq!(catalogue.len(), 12);
        assert!(client.cached_inventory().owned.is_empty());
        assert!(!client.is_signed_in());
        assert_eq!(client.session_username(), "");
    }

    #[test]
    fn url_encoding_escapes_path_segments() {
        assert_eq!(urlencode("cape-aethel"), "cape-aethel");
        assert_eq!(urlencode("a/b"), "a%2Fb");
        assert_eq!(urlencode("sp ace"), "sp%20ace");
    }

    fn session_expiring_in(seconds: i64) -> PlatformSession {
        PlatformSession {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339(),
            username: "Steve".into(),
            role: "user".into(),
        }
    }

    #[test]
    fn session_expiry_detects_fresh_and_stale_tokens() {
        assert!(!session_expiring_in(3600).access_expired());
        assert!(session_expiring_in(-10).access_expired());
        assert!(session_expiring_in(10).access_expired_within(chrono::Duration::seconds(30)));
        assert!(!session_expiring_in(120).access_expired_within(chrono::Duration::seconds(30)));
        let broken = PlatformSession {
            expires_at: "not-a-date".into(),
            ..session_expiring_in(3600)
        };
        assert!(broken.access_expired());
    }

    #[test]
    fn local_equip_replaces_the_slot_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let client = PlatformClient::new(dir.path());

        let state = client.equip_local("cape-aethel");
        assert!(state.contains("cape-aethel"));
        assert_eq!(
            state.slot("cape").map(|i| i.slug.as_str()),
            Some("cape-aethel")
        );

        let state = client.equip_local("cape-ember");
        assert!(state.contains("cape-ember"));
        assert!(!state.contains("cape-aethel"), "one cape per slot");
        assert_eq!(state.items.len(), 1);

        let state = client.equip_local("wings-void");
        assert_eq!(state.items.len(), 2, "wings do not touch the cape slot");
        assert_eq!(
            state.slot("wings").map(|i| i.slug.as_str()),
            Some("wings-void")
        );

        let reloaded = PlatformClient::new(dir.path()).equipped_state();
        assert_eq!(reloaded.slugs(), state.slugs());
        assert!(reloaded.updated_at.contains('T'));

        let state = client.unequip_local("cape-ember");
        assert!(!state.contains("cape-ember"));
        assert!(state.contains("wings-void"), "other slots survive");
        client.unequip_local("wings-void");
        assert!(PlatformClient::new(dir.path())
            .equipped_state()
            .items
            .is_empty());
    }

    #[test]
    fn equipped_documents_carry_what_the_mod_needs() {
        let dir = tempfile::tempdir().unwrap();
        let client = PlatformClient::new(dir.path());
        client.equip_local("elytra-dragon");
        let docs = client.equipped_documents();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0]["slug"], "elytra-dragon");
        assert_eq!(docs[0]["slot"], "wings");
        assert_eq!(docs[0]["kind"], "elytra");
        assert_eq!(docs[0]["assetUrl"], "elytras/dragon.png");
    }

    #[test]
    fn slots_follow_the_catalogue_kind() {
        assert_eq!(slot_for_kind("cape"), "cape");
        assert_eq!(slot_for_kind("elytra"), "wings");
        assert_eq!(slot_for_kind("wings"), "wings");
        assert_eq!(slot_for_kind("skin"), "skin");
        assert_eq!(slot_for_kind("hud"), "hud");
    }

    #[test]
    fn local_equip_ignores_unknown_slugs() {
        let dir = tempfile::tempdir().unwrap();
        let client = PlatformClient::new(dir.path());
        let state = client.equip_local("cape-does-not-exist");
        assert!(state.items.is_empty());
    }

    #[test]
    fn session_persists_to_disk_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let client = PlatformClient::new(dir.path());
        let session = session_expiring_in(3600);
        client.store_session(session.clone());

        let raw = std::fs::read_to_string(dir.path().join("platform").join(SESSION_FILE)).unwrap();
        assert!(raw.contains("access_token"));

        let reloaded = PlatformClient::new(dir.path());
        assert_eq!(reloaded.session(), Some(session));
        assert!(reloaded.is_signed_in());
        assert_eq!(reloaded.session_username(), "Steve");

        reloaded.clear_session();
        assert!(reloaded.session().is_none());
        assert!(
            !dir.path().join("platform").join(SESSION_FILE).exists(),
            "session file removed on sign-out"
        );
    }

    #[cfg(unix)]
    #[test]
    fn session_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let client = PlatformClient::new(dir.path());
        client.store_session(session_expiring_in(3600));
        let mode = std::fs::metadata(dir.path().join("platform").join(SESSION_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
