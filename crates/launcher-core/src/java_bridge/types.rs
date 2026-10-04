
#[derive(Debug, Clone)]
pub struct MCLauncherSession {
    pub access_token: String,
    pub username: String,
    pub uuid: String,
    pub user_type: String,
    pub client_token: String,
}

impl MCLauncherSession {
    pub fn is_legacy(&self) -> bool {
        self.user_type == "legacy" || self.user_type.is_empty()
    }

    pub fn is_microsoft(&self) -> bool {
        self.user_type == "mst" || self.user_type == "msa"
    }
}

impl From<MCLauncherSession> for super::super::auth::Account {
    fn from(session: MCLauncherSession) -> Self {
        if session.is_microsoft() {
            Self::Microsoft {
                mc_access_token: session.access_token,
                username: session.username,
                uuid: session.uuid,
                msa_refresh_token: None,
            }
        } else {
            use super::super::auth::{offline_uuid, Account};
            let uuid = offline_uuid(&session.username);
            Self::Offline {
                name: session.username,
                uuid,
            }
        }
    }
}
