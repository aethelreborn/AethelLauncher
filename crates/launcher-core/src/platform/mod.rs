//! Cosmetics platform client.
//!
//! Offline-first by design ([08 §9](../../../opencode-docs/08-ui-design.md)): the
//! catalogue is cached on disk and mirrors the backend seed, so the Cosmetics
//! screen renders instantly and keeps working when the backend is unreachable.
//! Only actions that genuinely need the server (buy / equip) fail, and they fail
//! with a clear message instead of a dead end.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default backend address; override with `AETHEL_API_URL`.
pub const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";

const CACHE_FILE: &str = "cosmetics-cache.json";

/// A catalogue entry. Mirrors the backend's `Cosmetic` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cosmetic {
    pub slug: String,
    pub name: String,
    /// `cape | wings | badge | hud | elytra | skin`
    pub kind: String,
    /// `common | rare | epic | legendary`
    pub rarity: String,
    pub asset_url: String,
    /// Fallback colour used when the asset image is unavailable.
    pub tint_hex: String,
    pub price_cents: i32,
    #[serde(default)]
    pub sort_order: i32,
}

impl Cosmetic {
    pub fn is_free(&self) -> bool {
        self.price_cents <= 0
    }

    /// Human price, e.g. `2.50` (in platform currency units).
    pub fn price_label(&self) -> String {
        if self.is_free() {
            "Free".to_string()
        } else {
            format!("{:.2}", self.price_cents as f64 / 100.0)
        }
    }

    /// Parse `tint_hex` (`#RRGGBB`) into an RGB triple.
    pub fn tint_rgb(&self) -> [u8; 3] {
        parse_hex(&self.tint_hex).unwrap_or([108, 92, 231])
    }
}

/// A player's ownership + equipped state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub balance_cents: i32,
    #[serde(default)]
    pub owned: Vec<String>,
    /// kind → slug
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

/// Result of a purchase attempt.
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

/// The starter catalogue, matching `supabase/migrations/0001_cosmetics.sql`.
/// Used before the first successful fetch and when the backend has never been
/// reachable, so the Cosmetics screen is never empty on a fresh install.
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

/// Client for the Aethel cosmetics backend.
#[derive(Clone)]
pub struct PlatformClient {
    base_url: String,
    http: reqwest::Client,
    cache_path: PathBuf,
    /// Last successful catalogue, shared so cloning the client is cheap.
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
        let base_url = std::env::var("AETHEL_API_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());

        let http = reqwest::Client::builder()
            .user_agent(concat!("AethelLauncher/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .unwrap_or_default();

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
            cache_path: data_dir.join("platform").join(CACHE_FILE),
            catalogue: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Catalogue from memory → disk → built-in seed, in that order. Never fails,
    /// so the UI can always render something.
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

    /// `GET /api/v1/health`
    pub async fn health(&self) -> Connection {
        let url = format!("{}/api/v1/health", self.base_url);
        match self.http.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    // A reachable server without a database cannot serve cosmetics.
                    Ok(body) if body["database"] == "ok" => Connection::Online,
                    Ok(_) => Connection::Offline,
                    Err(_) => Connection::Offline,
                }
            }
            _ => Connection::Offline,
        }
    }

    /// `GET /api/v1/cosmetics`, falling back to the cache on failure.
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

    /// `GET /api/v1/cosmetics/{username}/inventory`
    pub async fn fetch_inventory(&self, username: &str) -> anyhow::Result<Inventory> {
        let url = format!(
            "{}/api/v1/cosmetics/{}/inventory",
            self.base_url,
            urlencode(username)
        );
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("could not reach {url}"))?
            .error_for_status()
            .context("backend rejected the inventory request")?;

        let inventory: Inventory = response
            .json()
            .await
            .context("backend returned an unexpected inventory payload")?;

        self.write_cache(&self.cached_catalogue(), inventory.clone());
        Ok(inventory)
    }

    /// `POST /api/v1/cosmetics/{slug}/purchase`
    pub async fn purchase(
        &self,
        slug: &str,
        username: &str,
        request_id: &str,
    ) -> anyhow::Result<PurchaseOutcome> {
        let url = format!(
            "{}/api/v1/cosmetics/{}/purchase",
            self.base_url,
            urlencode(slug)
        );
        let response = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "username": username,
                "request_id": request_id,
            }))
            .send()
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

    /// `POST /api/v1/cosmetics/{slug}/equip`
    pub async fn equip(&self, slug: &str, username: &str) -> anyhow::Result<Inventory> {
        self.post_player_action("equip", slug, username).await
    }

    /// `POST /api/v1/cosmetics/{slug}/unequip`
    pub async fn unequip(&self, slug: &str, username: &str) -> anyhow::Result<Inventory> {
        self.post_player_action("unequip", slug, username).await
    }

    async fn post_player_action(
        &self,
        action: &str,
        slug: &str,
        username: &str,
    ) -> anyhow::Result<Inventory> {
        let url = format!(
            "{}/api/v1/cosmetics/{}/{}",
            self.base_url,
            urlencode(slug),
            action
        );
        let response = self
            .http
            .post(&url)
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await
            .with_context(|| format!("could not reach {url}"))?;

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

/// Minimal percent-encoding for the path segments we build.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalogue_matches_the_sql_seed() {
        let catalogue = builtin_catalogue();
        assert_eq!(catalogue.len(), 12);
        assert!(catalogue.iter().any(|c| c.slug == "cape-aethel"));
        assert!(catalogue.iter().any(|c| c.slug == "elytra-dragon"));
        // Every entry must have a usable tint for the image-less placeholder.
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
        // Falls back rather than panicking.
        let bad = Cosmetic {
            tint_hex: "garbage".to_string(),
            ..builtin_catalogue()[0].clone()
        };
        assert_eq!(bad.tint_rgb(), [108, 92, 231]);
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
        // No backend, no cache — the UI still gets a catalogue.
        let catalogue = client.cached_catalogue();
        assert_eq!(catalogue.len(), 12);
        assert!(client.cached_inventory().owned.is_empty());
    }

    #[test]
    fn url_encoding_escapes_path_segments() {
        assert_eq!(urlencode("cape-aethel"), "cape-aethel");
        assert_eq!(urlencode("a/b"), "a%2Fb");
        assert_eq!(urlencode("sp ace"), "sp%20ace");
    }
}
