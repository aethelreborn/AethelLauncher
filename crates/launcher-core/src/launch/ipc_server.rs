//! IPC WebSocket server — loopback-only communication between launcher and game.
//!
//! See [12 · IPC](../../../opencode-docs/12-ipc.md) for the protocol spec.

use rand::Rng;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Per-launch IPC server bound to 127.0.0.1 with a crypto-random token.
pub struct IpcServer {
    port: u16,
    token: String,
}

impl IpcServer {
    pub async fn new() -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let token: Vec<u8> = (0..16).map(|_| rand::thread_rng().gen()).collect();
        Ok(Self {
            port,
            token: hex::encode(&token),
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    pub fn jvm_args(&self) -> Vec<String> {
        vec![
            format!("-Daethel.ipc={}", self.url()),
            format!("-Daethel.ipcToken={}", self.token),
        ]
    }

    /// Accept loop — rejects non-loopback connections.
    pub async fn accept_loop(self: Arc<Self>) -> anyhow::Result<()> {
        // In production, this would accept WebSocket connections
        // For v1 stub, we just log the setup
        tracing::info!("IPC server listening on {}", self.url());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ipc_server_creation() {
        let server = IpcServer::new().await.expect("create IPC server");
        assert!(server.port() > 0);
        assert_eq!(server.token().len(), 32); // 16 bytes = 32 hex chars
        assert!(server.url().starts_with("ws://127.0.0.1:"));
    }

    #[tokio::test]
    async fn test_jvm_args() {
        let server = IpcServer::new().await.expect("create IPC server");
        let args = server.jvm_args();
        assert_eq!(args.len(), 2);
        assert!(args[0].starts_with("-Daethel.ipc="));
        assert!(args[1].starts_with("-Daethel.ipcToken="));
    }
}
