//! Authentication module — offline flow and Microsoft placeholder.

pub mod microsoft;
pub mod offline;

pub use offline::*;

/// Authentication mode for an instance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Offline,
    Microsoft,
    Auto,
}

/// Resolved account after authentication.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Account {
    Offline {
        name: String,
        uuid: uuid::Uuid,
    },
    Microsoft {
        mc_access_token: String,
        username: String,
        uuid: String,
        #[serde(skip)]
        msa_refresh_token: Option<String>,
    },
}

impl Account {
    pub fn username(&self) -> &str {
        match self {
            Account::Offline { name, .. } => name,
            Account::Microsoft { username, .. } => username,
        }
    }

    /// Returns the UUID as a string.
    pub fn uuid_str(&self) -> String {
        match self {
            Account::Offline { uuid, .. } => uuid.to_string(),
            Account::Microsoft { uuid, .. } => uuid.clone(),
        }
    }
}
