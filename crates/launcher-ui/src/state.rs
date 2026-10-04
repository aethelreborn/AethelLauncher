use crate::config::LauncherConfig;
use crate::launch::LaunchController;
use crate::screens::{
    account, crash_viewer, downloads, home, library, mods, news, settings, splash, store,
};
use launcher_core::auth::{offline_uuid, validate_username, Account};
use launcher_core::platform::PlatformClient;
use launcher_core::{CoreHandle, VersionCache};
use std::path::PathBuf;

pub struct AppModel {
    pub active_screen: Screen,
    pub splash: splash::SplashState,
    pub home: home::AppState,
    pub library: library::LibraryState,
    pub downloads: downloads::DownloadsState,
    pub mods: mods::ModsState,
    pub account: account::AccountState,
    pub settings: settings::SettingsState,
    pub news: news::NewsState,
    pub crash_viewer: crash_viewer::CrashViewerState,
    pub store: store::StoreState,
    pub platform: PlatformClient,
    pub launch: LaunchController,
    pub auth: Account,
    pub config: LauncherConfig,
    pub data_dir: PathBuf,
}

impl std::fmt::Debug for AppModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppModel")
            .field("active_screen", &self.active_screen)
            .field("auth", &self.auth)
            .field("launch", &self.launch)
            .field("store", &self.store)
            .field("data_dir", &self.data_dir)
            .finish_non_exhaustive()
    }
}

impl Default for AppModel {
    fn default() -> Self {
        Self::new()
    }
}

impl AppModel {
    pub fn new() -> Self {
        let core = CoreHandle::new();
        let data_dir = core.base_dir.clone();

        let mut config = LauncherConfig::load(&data_dir);
        if config.username.is_empty() {
            config.username = default_username();
        }
        let auth = offline_account_or_fallback(&config.username);

        let db_path = data_dir.join("instances.db");
        let instances_dir = data_dir.join("instances");
        let mut library = library::LibraryState::new(db_path, instances_dir.clone());
        library.load_from_disk();

        let mut home = home::AppState::new(VersionCache::new(core.cache_dir.clone()));
        home.env = home::LaunchEnv {
            shared_dir: data_dir.clone(),
            instances_dir,
            java_path: config.java_path.clone().map(PathBuf::from),
        };
        home.renderer_mode = settings::RendererMode::parse(&config.renderer);
        if let Some(ram) = config.ram_mb {
            home.ram_mb = ram;
        }
        if let Some(id) = &config.selected_instance {
            home.select_instance(Some(id), &library.instances);
        }

        let platform = PlatformClient::new(&data_dir);
        let mut account_state = account::AccountState::from_username(&config.username);
        if let Some(session) = platform.session() {
            account_state.set_session(&session);
        }

        let mut mods = mods::ModsState::default();
        mods.cf_key = config.curseforge_api_key.clone();

        Self {
            active_screen: Screen::default(),
            splash: Default::default(),
            home,
            library,
            downloads: Default::default(),
            mods,
            account: account_state,
            settings: settings::SettingsState::from_config(&config, &data_dir),
            news: Default::default(),
            crash_viewer: Default::default(),
            store: store::StoreState::new(platform.clone()),
            platform,
            launch: LaunchController::new(),
            auth,
            config,
            data_dir,
        }
    }

    pub fn save_config(&mut self) {
        if let Err(e) = self.config.save(&self.data_dir) {
            tracing::warn!("failed to save launcher config: {e}");
        }
    }

    pub fn set_username(&mut self, username: &str) {
        match offline_account_or_fallback_result(username) {
            Ok(account) => {
                self.auth = account;
                self.config.username = username.to_string();
                self.account.saved_username = username.to_string();
                self.account.error = None;
                self.save_config();
            }
            Err(e) => {
                self.account.error = Some(e);
            }
        }
    }

    pub fn apply_settings(&mut self) {
        self.settings.apply_to(&mut self.config);
        self.home.renderer_mode = self.settings.renderer_mode;
        self.home.ram_mb = self.settings.max_ram_mb;
        self.home.env.java_path = if self.settings.java_path.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.settings.java_path.trim()))
        };
        self.settings.dirty = false;
        self.settings.notice = Some("Saved".to_string());
        self.save_config();
    }
}

fn default_username() -> String {
    for key in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(name) = std::env::var(key) {
            let trimmed = name.trim();
            if validate_username(trimmed).is_ok() {
                return trimmed.to_string();
            }
        }
    }
    "Player".to_string()
}

fn offline_account_or_fallback_result(username: &str) -> Result<Account, String> {
    validate_username(username)?;
    Ok(Account::Offline {
        name: username.to_string(),
        uuid: offline_uuid(username),
    })
}

fn offline_account_or_fallback(username: &str) -> Account {
    offline_account_or_fallback_result(username).unwrap_or_else(|_| Account::Offline {
        name: "Player".to_string(),
        uuid: offline_uuid("Player"),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Home,
    Library,
    Store,
    Downloads,
    Mods,
    Account,
    Settings,
    News,
    CrashViewer,
    Splash,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_username_is_a_valid_minecraft_name() {
        let name = default_username();
        assert!(
            validate_username(&name).is_ok(),
            "default username {name:?} is not a legal Minecraft name"
        );
    }

    #[test]
    fn invalid_username_never_makes_an_account() {
        assert!(offline_account_or_fallback_result("ab").is_err());
        let account = offline_account_or_fallback("xx");
        assert_eq!(account.username(), "Player");
    }
}
