
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModpackError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("failed to parse JSON response: {0}")]
    Json(#[from] serde_json::Error),

    #[error("modpack not found: {0}")]
    NotFound(String),

    #[error("invalid response from provider: {0}")]
    InvalidResponse(String),

    #[error("download failed at {url}: {source}")]
    DownloadFailed { url: String, source: anyhow::Error },

    #[error("file not found locally: {0}")]
    FileNotFound(std::path::PathBuf),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("checksum mismatch for {path}: expected {expected}, got {actual}")]
    ChecksumMismatch { path: String, expected: String, actual: String },
}

impl From<anyhow::Error> for ModpackError {
    fn from(err: anyhow::Error) -> Self {
        ModpackError::InvalidResponse(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ModpackError>;
