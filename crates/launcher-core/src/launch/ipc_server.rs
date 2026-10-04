use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, watch, Mutex};
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::Message;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FRAME_BYTES: usize = 64 * 1024;
const RATE_PER_SEC: f64 = 100.0;
const RATE_BURST: f64 = 20.0;
const CLOSE_HANDSHAKE_TIMEOUT: u16 = 4000;
const CLOSE_INVALID_TOKEN: u16 = 4003;
const CLOSE_TOO_MANY: u16 = 4004;
const CLOSE_NOT_EXTENSIBLE: u16 = 1003;
const CLOSE_INVALID_PAYLOAD: u16 = 1007;

type WsStream = futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>;
type WsSink =
    futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, Message>;

struct RateGate {
    tokens: f64,
    last_refill: Instant,
    last_seen: HashMap<&'static str, Instant>,
}

impl RateGate {
    fn new() -> Self {
        Self {
            tokens: RATE_BURST,
            last_refill: Instant::now(),
            last_seen: HashMap::new(),
        }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * RATE_PER_SEC).min(RATE_BURST);
        self.last_refill = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }

    fn allow_type(&mut self, kind: &'static str, min_interval: Duration) -> bool {
        let now = Instant::now();
        if let Some(prev) = self.last_seen.get(kind) {
            if now.duration_since(*prev) < min_interval {
                return false;
            }
        }
        self.last_seen.insert(kind, now);
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum IpcMessage {
    #[serde(rename = "aethel_hello")]
    AethelHello {
        v: u8,
        token: String,
        launcher: String,
        version: String,
        game_pid: u32,
        mc_version: String,
    },
    Welcome {
        v: u8,
        session_id: String,
        slot: String,
        capabilities: Vec<String>,
    },
    Launched {
        v: u8,
        renderer: String,
        width: u32,
        height: u32,
    },
    Ping {
        v: u8,
        seq: u64,
    },
    Pong {
        v: u8,
        seq: u64,
    },
    Fps {
        v: u8,
        fps: f32,
        frame_time_ms: f32,
    },
    Playtime {
        v: u8,
        secs: u64,
    },
    World {
        v: u8,
        dimension: String,
        server: Option<String>,
        players: Option<u32>,
    },
    Toggle {
        v: u8,
        module_id: String,
        enabled: bool,
    },
    HudLayout {
        v: u8,
        layout: Value,
    },
    SetTheme {
        v: u8,
        theme: Value,
    },
    Telemetry {
        v: u8,
        events: Vec<Value>,
    },
    #[serde(rename = "crash.handshake")]
    CrashHandshake {
        v: u8,
        report_id: String,
    },
    Crash {
        v: u8,
        summary: String,
        stack: String,
        report_id: Option<String>,
    },
    Cosmetics {
        v: u8,
        items: Vec<Value>,
    },
    SetToggles {
        v: u8,
        toggles: Vec<ToggleState>,
    },
    QuickJoin {
        v: u8,
        address: String,
        port: Option<u16>,
    },
    Focus {
        v: u8,
        focused: bool,
    },
    Bye {
        v: u8,
        code: i32,
        reason: Option<String>,
    },
    Exit {
        v: u8,
        code: i32,
        reason: String,
        #[serde(rename = "graceMs", skip_serializing_if = "Option::is_none")]
        grace_ms: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToggleState {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpcEvent {
    Connected {
        session_id: String,
    },
    Launched {
        renderer: String,
        width: u32,
        height: u32,
    },
    Fps {
        fps: f32,
        frame_time_ms: f32,
    },
    Playtime {
        secs: u64,
    },
    World {
        dimension: String,
        server: Option<String>,
        players: Option<u32>,
    },
    Toggle {
        module_id: String,
        enabled: bool,
    },
    HudLayout {
        layout: Value,
    },
    Telemetry {
        events: Vec<Value>,
    },
    CrashHandshake {
        report_id: String,
    },
    Crash {
        summary: String,
        stack: String,
        report_id: Option<String>,
    },
    Cosmetics {
        items: Vec<Value>,
    },
    Closed,
}

#[derive(Clone, Debug)]
pub struct IpcHandle {
    tx: mpsc::Sender<String>,
}

impl IpcHandle {
    pub fn send(&self, msg: &IpcMessage) -> bool {
        match serde_json::to_string(msg) {
            Ok(frame) => self.tx.try_send(frame).is_ok(),
            Err(_) => false,
        }
    }

    pub fn set_theme(&self, theme: Value) -> bool {
        self.send(&IpcMessage::SetTheme { v: 1, theme })
    }

    pub fn set_toggles(&self, toggles: Vec<ToggleState>) -> bool {
        self.send(&IpcMessage::SetToggles { v: 1, toggles })
    }

    pub fn push_hud_layout(&self, layout: Value) -> bool {
        self.send(&IpcMessage::HudLayout { v: 1, layout })
    }

    pub fn push_cosmetics(&self, items: Vec<Value>) -> bool {
        self.send(&IpcMessage::Cosmetics { v: 1, items })
    }

    pub fn quick_join(&self, address: impl Into<String>, port: Option<u16>) -> bool {
        self.send(&IpcMessage::QuickJoin {
            v: 1,
            address: address.into(),
            port,
        })
    }

    pub fn focus(&self, focused: bool) -> bool {
        self.send(&IpcMessage::Focus { v: 1, focused })
    }

    pub fn exit(&self, reason: &str, grace_ms: u64) -> bool {
        self.send(&IpcMessage::Exit {
            v: 1,
            code: 0,
            reason: reason.to_string(),
            grace_ms: Some(grace_ms),
        })
    }
}

pub struct IpcServer {
    port: u16,
    token: String,
    listener: Arc<TcpListener>,
    session: Arc<Mutex<Option<String>>>,
    alive_tx: watch::Sender<bool>,
    events: broadcast::Sender<IpcEvent>,
    outbound_tx: mpsc::Sender<String>,
    outbound_rx: Arc<Mutex<Option<mpsc::Receiver<String>>>>,
    handshake_timeout: Duration,
}

impl std::fmt::Debug for IpcServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcServer")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl IpcServer {
    pub async fn new() -> anyhow::Result<Self> {
        Self::with_config(HANDSHAKE_TIMEOUT).await
    }

    async fn with_config(handshake_timeout: Duration) -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let token: Vec<u8> = (0..16).map(|_| rand::thread_rng().gen()).collect();
        let (alive_tx, _) = watch::channel(false);
        let (events, _) = broadcast::channel(64);
        let (outbound_tx, outbound_rx) = mpsc::channel(64);
        Ok(Self {
            port,
            token: hex::encode(token),
            listener: Arc::new(listener),
            session: Arc::new(Mutex::new(None)),
            alive_tx,
            events,
            outbound_tx,
            outbound_rx: Arc::new(Mutex::new(Some(outbound_rx))),
            handshake_timeout,
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

    pub fn subscribe(&self) -> broadcast::Receiver<IpcEvent> {
        self.events.subscribe()
    }

    pub fn handle(&self) -> IpcHandle {
        IpcHandle {
            tx: self.outbound_tx.clone(),
        }
    }

    pub fn send(&self, msg: &IpcMessage) -> bool {
        match serde_json::to_string(msg) {
            Ok(frame) => self.outbound_tx.try_send(frame).is_ok(),
            Err(_) => false,
        }
    }

    pub async fn has_session(&self) -> bool {
        self.session.lock().await.is_some()
    }

    pub async fn wait_bye(&self) {
        let mut rx = self.alive_tx.subscribe();
        loop {
            if !*rx.borrow_and_update() {
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    }

    pub async fn accept_loop(self: Arc<Self>) -> anyhow::Result<()> {
        tracing::info!("IPC server listening on {}", self.url());
        loop {
            let (stream, peer) = self.listener.accept().await?;
            if !peer.ip().is_loopback() {
                tracing::warn!("IPC: rejected non-loopback peer {peer}");
                continue;
            }
            let server = self.clone();
            tokio::spawn(async move {
                if let Err(e) = server.handle_connection(stream, peer).await {
                    tracing::debug!("IPC connection from {peer} ended: {e:#}");
                }
            });
        }
    }

    async fn handle_connection(
        self: Arc<Self>,
        stream: TcpStream,
        peer: std::net::SocketAddr,
    ) -> anyhow::Result<()> {
        let mut config = WebSocketConfig::default();
        config.max_message_size = Some(MAX_FRAME_BYTES);
        config.max_frame_size = Some(MAX_FRAME_BYTES);
        let ws = tokio_tungstenite::accept_async_with_config(stream, Some(config)).await?;
        let (mut sink, mut incoming) = ws.split();

        let claimed = {
            let mut slot = self.session.lock().await;
            if slot.is_none() {
                *slot = Some(String::new());
                true
            } else {
                false
            }
        };
        if !claimed {
            tracing::warn!("IPC: rejected second connection from {peer}");
            let close = Message::Close(Some(CloseFrame {
                code: CLOSE_TOO_MANY.into(),
                reason: "too many connections".into(),
            }));
            let _ = sink.send(close).await;
            return Ok(());
        }

        let result = self.session_worker(&mut sink, &mut incoming, peer).await;

        let was_alive;
        {
            let mut slot = self.session.lock().await;
            was_alive = self.alive_tx.send_replace(false);
            if was_alive {
                let _ = self.events.send(IpcEvent::Closed);
            }
            *slot = None;
        }
        result
    }

    async fn session_worker(
        &self,
        sink: &mut WsSink,
        incoming: &mut WsStream,
        peer: std::net::SocketAddr,
    ) -> anyhow::Result<()> {
        let first = match tokio::time::timeout(self.handshake_timeout, incoming.next()).await {
            Err(_) => {
                tracing::warn!("IPC: handshake timeout from {peer}");
                close_frame(sink, CLOSE_HANDSHAKE_TIMEOUT, "handshake timeout").await;
                return Ok(());
            }
            Ok(None) => anyhow::bail!("closed before hello"),
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(Some(Ok(msg))) => msg,
        };
        let hello: Value = match first {
            Message::Text(text) => serde_json::from_str(&text)?,
            other => anyhow::bail!("expected hello text frame, got {other:?}"),
        };
        let token_ok = hello["type"] == "aethel_hello" && token_eq(&self.token, &hello);
        if !token_ok {
            tracing::warn!("IPC: rejected handshake from {peer}");
            close_frame(sink, CLOSE_INVALID_TOKEN, "invalid token").await;
            return Ok(());
        }

        let session_id = {
            let bytes: Vec<u8> = (0..8).map(|_| rand::thread_rng().gen()).collect();
            hex::encode(bytes)
        };
        *self.session.lock().await = Some(session_id.clone());
        self.alive_tx.send_replace(true);

        let welcome = IpcMessage::Welcome {
            v: 1,
            session_id: session_id.clone(),
            slot: "primary".to_string(),
            capabilities: vec![],
        };
        sink.send(Message::Text(serde_json::to_string(&welcome)?.into()))
            .await?;
        tracing::info!("IPC: session {session_id} established with {peer}");
        let _ = self.events.send(IpcEvent::Connected {
            session_id: session_id.clone(),
        });

        let mut outbound = self
            .outbound_rx
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow::anyhow!("outbound queue already taken"))?;

        let mut gate = RateGate::new();
        let mut launched_seen = false;
        let outcome: anyhow::Result<()> = loop {
            tokio::select! {
                incoming_msg = incoming.next() => {
                    match incoming_msg {
                        None => break Ok(()),
                        Some(Err(e)) => break Err(e.into()),
                        Some(Ok(Message::Close(_))) => break Ok(()),
                        Some(Ok(Message::Ping(payload))) => {
                            sink.send(Message::Pong(payload)).await?;
                        }
                        Some(Ok(Message::Pong(_))) => {}
                        Some(Ok(Message::Binary(_))) => {
                            close_frame(sink, CLOSE_NOT_EXTENSIBLE, "binary frames not allowed").await;
                            break Ok(());
                        }
                        Some(Ok(Message::Text(text))) => {
                            match self.dispatch_text(&text, &mut gate, &mut launched_seen).await? {
                                TextAction::Continue => {}
                                TextAction::Reply(frame) => {
                                    sink.send(Message::Text(frame.into())).await?;
                                }
                                TextAction::Close { code, reason } => {
                                    close_frame(sink, code, reason).await;
                                    break Ok(());
                                }
                                TextAction::Bye => break Ok(()),
                            }
                        }
                        Some(Ok(_)) => {}
                    }
                }
                out = outbound.recv() => {
                    match out {
                        None => break Ok(()),
                        Some(frame) => sink.send(Message::Text(frame.into())).await?,
                    }
                }
            }
        };

        *self.outbound_rx.lock().await = Some(outbound);
        outcome
    }

    async fn dispatch_text(
        &self,
        text: &str,
        gate: &mut RateGate,
        launched_seen: &mut bool,
    ) -> anyhow::Result<TextAction> {
        let value: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => {
                return Ok(TextAction::Close {
                    code: CLOSE_INVALID_PAYLOAD,
                    reason: "invalid json",
                })
            }
        };
        let Some(version) = value.get("v").and_then(Value::as_u64) else {
            return Ok(TextAction::Close {
                code: CLOSE_INVALID_PAYLOAD,
                reason: "missing v",
            });
        };
        if version > 1 {
            tracing::debug!("IPC: ignoring frame with unsupported v={version}");
            return Ok(TextAction::Continue);
        }
        let msg: IpcMessage = match serde_json::from_value(value) {
            Ok(m) => m,
            Err(_) => {
                tracing::debug!("IPC: ignoring unknown or malformed frame");
                return Ok(TextAction::Continue);
            }
        };
        if !gate.allow() {
            tracing::debug!("IPC: rate limit drop");
            return Ok(TextAction::Continue);
        }

        match msg {
            IpcMessage::Launched {
                renderer,
                width,
                height,
                ..
            } => {
                if !*launched_seen {
                    *launched_seen = true;
                    let _ = self.events.send(IpcEvent::Launched {
                        renderer,
                        width,
                        height,
                    });
                }
                Ok(TextAction::Continue)
            }
            IpcMessage::Ping { seq, .. } => {
                if gate.allow_type("ping", Duration::from_millis(900)) {
                    let pong = serde_json::to_string(&IpcMessage::Pong { v: 1, seq })?;
                    Ok(TextAction::Reply(pong))
                } else {
                    Ok(TextAction::Continue)
                }
            }
            IpcMessage::Fps {
                fps, frame_time_ms, ..
            } => {
                if gate.allow_type("fps", Duration::from_millis(900)) {
                    let _ = self.events.send(IpcEvent::Fps { fps, frame_time_ms });
                }
                Ok(TextAction::Continue)
            }
            IpcMessage::Playtime { secs, .. } => {
                if gate.allow_type("playtime", Duration::from_secs(55)) {
                    let _ = self.events.send(IpcEvent::Playtime { secs });
                }
                Ok(TextAction::Continue)
            }
            IpcMessage::World {
                dimension,
                server,
                players,
                ..
            } => {
                let _ = self.events.send(IpcEvent::World {
                    dimension,
                    server,
                    players,
                });
                Ok(TextAction::Continue)
            }
            IpcMessage::Toggle {
                module_id, enabled, ..
            } => {
                let _ = self.events.send(IpcEvent::Toggle { module_id, enabled });
                Ok(TextAction::Continue)
            }
            IpcMessage::HudLayout { layout, .. } => {
                let _ = self.events.send(IpcEvent::HudLayout { layout });
                Ok(TextAction::Continue)
            }
            IpcMessage::Telemetry { events, .. } => {
                let _ = self.events.send(IpcEvent::Telemetry { events });
                Ok(TextAction::Continue)
            }
            IpcMessage::CrashHandshake { report_id, .. } => {
                let _ = self.events.send(IpcEvent::CrashHandshake { report_id });
                Ok(TextAction::Continue)
            }
            IpcMessage::Crash {
                summary,
                stack,
                report_id,
                ..
            } => {
                let _ = self.events.send(IpcEvent::Crash {
                    summary,
                    stack,
                    report_id,
                });
                Ok(TextAction::Continue)
            }
            IpcMessage::Cosmetics { items, .. } => {
                let _ = self.events.send(IpcEvent::Cosmetics { items });
                Ok(TextAction::Continue)
            }
            IpcMessage::Bye { .. } => {
                tracing::info!("IPC: game sent bye");
                Ok(TextAction::Bye)
            }
            _ => {
                tracing::debug!("IPC: ignoring unexpected inbound type");
                Ok(TextAction::Continue)
            }
        }
    }
}

enum TextAction {
    Continue,
    Reply(String),
    Close { code: u16, reason: &'static str },
    Bye,
}

async fn close_frame(sink: &mut WsSink, code: u16, reason: &str) {
    let close = Message::Close(Some(CloseFrame {
        code: code.into(),
        reason: reason.into(),
    }));
    let _ = sink.send(close).await;
}

fn token_eq(expected: &str, hello: &Value) -> bool {
    let given = hello["token"].as_str().unwrap_or_default();
    let expected = expected.as_bytes();
    let given = given.as_bytes();
    let mut diff = (expected.len() ^ given.len()) as u8;
    for i in 0..expected.len().max(given.len()) {
        let a = expected.get(i).copied().unwrap_or(0);
        let b = given.get(i).copied().unwrap_or(0);
        diff |= a ^ b;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hello(token: &str) -> String {
        json!({"v":1,"type":"aethel_hello","token":token,"launcher":"aethel",
               "version":"1.0.0","gamePid":42,"mcVersion":"1.21.11"})
        .to_string()
    }

    async fn boot() -> (Arc<IpcServer>, u16) {
        let server = Arc::new(IpcServer::new().await.expect("create IPC server"));
        let port = server.port();
        let accept = server.clone();
        tokio::spawn(async move {
            let _ = accept.accept_loop().await;
        });
        (server, port)
    }

    type Client = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn connect(port: u16) -> Client {
        let url = format!("ws://127.0.0.1:{port}");
        tokio_tungstenite::connect_async(&url)
            .await
            .expect("connect")
            .0
    }

    async fn send_text(ws: &mut Client, text: &str) {
        use futures_util::SinkExt;
        ws.send(Message::Text(text.to_string().into()))
            .await
            .expect("send");
    }

    async fn next_frame(ws: &mut Client) -> Option<Message> {
        use futures_util::StreamExt;
        ws.next().await.map(|r| r.expect("frame"))
    }

    #[tokio::test]
    async fn test_ipc_server_creation() {
        let server = IpcServer::new().await.expect("create IPC server");
        assert!(server.port() > 0);
        assert_eq!(server.token().len(), 32);
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

    #[tokio::test]
    async fn handshake_accepts_good_token() {
        let (server, port) = boot().await;
        let good_token = server.token().to_string();
        let mut events = server.subscribe();

        let mut ws = connect(port).await;
        send_text(&mut ws, &hello(&good_token)).await;

        let reply = next_frame(&mut ws).await.expect("welcome frame");
        let welcome: Value = serde_json::from_str(reply.to_text().expect("text")).expect("json");
        assert_eq!(welcome["type"], "welcome");
        assert_eq!(welcome["slot"], "primary");
        assert!(server.has_session().await);

        let mut connected = false;
        for _ in 0..10 {
            if matches!(events.try_recv(), Ok(IpcEvent::Connected { .. })) {
                connected = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(connected, "connected event never arrived");
    }

    #[tokio::test]
    async fn handshake_rejects_bad_token_with_4003() {
        let (server, port) = boot().await;
        let mut ws = connect(port).await;
        send_text(&mut ws, &hello("00")).await;

        let reply = next_frame(&mut ws).await.expect("close frame");
        match reply {
            Message::Close(frame) => {
                let code: u16 = frame.expect("close code").code.into();
                assert_eq!(code, CLOSE_INVALID_TOKEN);
            }
            other => panic!("expected close, got {other:?}"),
        }
        assert!(!server.has_session().await);
    }

    #[tokio::test]
    async fn second_connection_is_rejected_with_4004() {
        let (server, port) = boot().await;
        let good_token = server.token().to_string();

        let mut first = connect(port).await;
        send_text(&mut first, &hello(&good_token)).await;
        assert!(next_frame(&mut first).await.is_some());

        let mut second = connect(port).await;
        let reply = next_frame(&mut second).await.expect("close frame");
        match reply {
            Message::Close(frame) => {
                let code: u16 = frame.expect("close code").code.into();
                assert_eq!(code, CLOSE_TOO_MANY);
            }
            other => panic!("expected close, got {other:?}"),
        }
        assert!(server.has_session().await);
    }

    #[tokio::test]
    async fn frame_without_v_closes_1007() {
        let (server, port) = boot().await;
        let token = server.token().to_string();
        let mut ws = connect(port).await;
        send_text(&mut ws, &hello(&token)).await;
        assert!(next_frame(&mut ws).await.is_some());

        send_text(&mut ws, r#"{"type":"fps","fps":60}"#).await;
        let reply = next_frame(&mut ws).await.expect("close frame");
        match reply {
            Message::Close(frame) => {
                let code: u16 = frame.expect("close code").code.into();
                assert_eq!(code, CLOSE_INVALID_PAYLOAD);
            }
            other => panic!("expected close, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unsupported_v_and_unknown_type_are_ignored() {
        let (server, port) = boot().await;
        let token = server.token().to_string();
        let mut ws = connect(port).await;
        send_text(&mut ws, &hello(&token)).await;
        assert!(next_frame(&mut ws).await.is_some());
        let mut events = server.subscribe();

        send_text(&mut ws, r#"{"v":2,"type":"fps","fps":999}"#).await;
        send_text(&mut ws, r#"{"v":1,"type":"flux_capacitor"}"#).await;
        send_text(
            &mut ws,
            r#"{"v":1,"type":"launched","renderer":"vulkan","width":1920,"height":1080}"#,
        )
        .await;

        let mut saw_launched = false;
        for _ in 0..8 {
            match events.try_recv() {
                Ok(IpcEvent::Launched {
                    renderer, width, ..
                }) => {
                    assert_eq!(renderer, "vulkan");
                    assert_eq!(width, 1920);
                    saw_launched = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        assert!(saw_launched, "launched event never arrived");
    }

    #[tokio::test]
    async fn binary_frames_close_1003() {
        let (server, port) = boot().await;
        let token = server.token().to_string();
        let mut ws = connect(port).await;
        send_text(&mut ws, &hello(&token)).await;
        assert!(next_frame(&mut ws).await.is_some());

        use futures_util::SinkExt;
        ws.send(Message::Binary(vec![1u8, 2, 3].into()))
            .await
            .expect("send binary");
        let reply = next_frame(&mut ws).await.expect("close frame");
        match reply {
            Message::Close(frame) => {
                let code: u16 = frame.expect("close code").code.into();
                assert_eq!(code, CLOSE_NOT_EXTENSIBLE);
            }
            other => panic!("expected close, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ping_gets_pong_and_bye_ends_session() {
        let (server, port) = boot().await;
        let token = server.token().to_string();
        let mut ws = connect(port).await;
        send_text(&mut ws, &hello(&token)).await;
        assert!(next_frame(&mut ws).await.is_some());
        let mut events = server.subscribe();

        send_text(&mut ws, r#"{"v":1,"type":"ping","seq":7}"#).await;
        let pong = next_frame(&mut ws).await.expect("pong");
        let pong: Value = serde_json::from_str(pong.to_text().expect("text")).expect("json");
        assert_eq!(pong["type"], "pong");
        assert_eq!(pong["seq"], 7);

        send_text(
            &mut ws,
            r#"{"v":1,"type":"bye","code":0,"reason":"game_shutdown"}"#,
        )
        .await;
        let mut released = false;
        for _ in 0..50 {
            if !server.has_session().await {
                released = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(released, "bye must release the session");

        let mut saw_closed = false;
        for _ in 0..10 {
            if matches!(events.try_recv(), Ok(IpcEvent::Closed)) {
                saw_closed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(saw_closed, "closed event never arrived");
    }

    #[tokio::test]
    async fn outbound_frames_queue_then_deliver_after_handshake() {
        let server = Arc::new(IpcServer::new().await.expect("create IPC server"));
        let port = server.port();
        let good_token = server.token().to_string();
        let accept = server.clone();
        tokio::spawn(async move {
            let _ = accept.accept_loop().await;
        });

        assert!(server.handle().set_theme(json!({"schema":1})));

        let mut ws = connect(port).await;
        send_text(&mut ws, &hello(&good_token)).await;
        let welcome = next_frame(&mut ws).await.expect("welcome");
        assert!(welcome.to_text().expect("text").contains("welcome"));

        let frame = next_frame(&mut ws).await.expect("queued setTheme");
        let frame: Value = serde_json::from_str(frame.to_text().expect("text")).expect("json");
        assert_eq!(frame["type"], "setTheme");
        assert_eq!(frame["theme"]["schema"], 1);
    }

    #[tokio::test]
    async fn wait_bye_returns_immediately_without_session() {
        let server = IpcServer::new().await.expect("create IPC server");
        let started = Instant::now();
        tokio::time::timeout(Duration::from_millis(200), server.wait_bye())
            .await
            .expect("wait_bye must not block when idle");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn wait_bye_resolves_when_session_ends() {
        let (server, port) = boot().await;
        let token = server.token().to_string();
        let mut ws = connect(port).await;
        send_text(&mut ws, &hello(&token)).await;
        assert!(next_frame(&mut ws).await.is_some());
        assert!(server.has_session().await);

        let waiter = {
            let server = server.clone();
            tokio::spawn(async move {
                server.wait_bye().await;
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiter.is_finished(), "must wait while session is live");

        send_text(&mut ws, r#"{"v":1,"type":"bye","code":0}"#).await;
        tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("wait_bye resolves after bye")
            .expect("join");
    }

    #[tokio::test]
    async fn handshake_times_out_with_4000() {
        let server = Arc::new(
            IpcServer::with_config(Duration::from_millis(150))
                .await
                .expect("create IPC server"),
        );
        let port = server.port();
        let accept = server.clone();
        tokio::spawn(async move {
            let _ = accept.accept_loop().await;
        });

        let mut ws = connect(port).await;
        let reply = tokio::time::timeout(Duration::from_secs(3), next_frame(&mut ws))
            .await
            .expect("timeout")
            .expect("close frame");
        match reply {
            Message::Close(frame) => {
                let code: u16 = frame.expect("close code").code.into();
                assert_eq!(code, CLOSE_HANDSHAKE_TIMEOUT);
            }
            other => panic!("expected close, got {other:?}"),
        }
        assert!(!server.has_session().await);
    }

    #[test]
    fn token_eq_is_exact() {
        let hello_good = json!({"type":"aethel_hello","token":"abc123"});
        let hello_bad = json!({"type":"aethel_hello","token":"abc124"});
        let hello_missing = json!({"type":"aethel_hello"});
        assert!(token_eq("abc123", &hello_good));
        assert!(!token_eq("abc123", &hello_bad));
        assert!(!token_eq("abc123", &hello_missing));
        assert!(!token_eq(
            "abc123",
            &json!({"type":"aethel_hello","token":"abc1234"})
        ));
    }

    #[test]
    fn wire_enum_roundtrips_v1_frames() {
        let launched =
            r#"{"v":1,"type":"launched","renderer":"vulkan","width":1920,"height":1080}"#;
        let msg: IpcMessage = serde_json::from_str(launched).expect("parse launched");
        match msg {
            IpcMessage::Launched { width, .. } => assert_eq!(width, 1920),
            other => panic!("wrong variant: {other:?}"),
        }

        let crash = r#"{"v":1,"type":"crash.handshake","reportId":"r-9"}"#;
        let msg: IpcMessage = serde_json::from_str(crash).expect("parse crash.handshake");
        assert!(matches!(msg, IpcMessage::CrashHandshake { .. }));

        let exit = IpcMessage::Exit {
            v: 1,
            code: 0,
            reason: "launcher_stop".into(),
            grace_ms: Some(5000),
        };
        let text = serde_json::to_string(&exit).expect("encode exit");
        assert!(text.contains(r#""type":"exit""#));
        assert!(text.contains(r#""graceMs":5000"#));

        let hello_msg = IpcMessage::AethelHello {
            v: 1,
            token: "t".into(),
            launcher: "aethel".into(),
            version: "1.0.0".into(),
            game_pid: 9,
            mc_version: "1.21.11".into(),
        };
        let text = serde_json::to_string(&hello_msg).expect("encode hello");
        assert!(text.contains(r#""gamePid":9"#));
        assert!(text.contains(r#""mcVersion":"1.21.11""#));
    }

    #[test]
    fn rate_gate_allows_burst_then_throttles() {
        let mut gate = RateGate::new();
        let mut allowed = 0;
        for _ in 0..40 {
            if gate.allow() {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 20, "burst capacity is 20");

        let mut gate = RateGate::new();
        assert!(gate.allow_type("fps", Duration::from_millis(900)));
        assert!(!gate.allow_type("fps", Duration::from_millis(900)));
        assert!(gate.allow_type("playtime", Duration::from_secs(55)));
    }

    #[tokio::test]
    #[ignore = "requires a JDK; run `make ipc-e2e`"]
    async fn java_smoke_client_completes_full_exchange() {
        let classes = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../gamesupport/ipc-client/out");
        assert!(
            classes.join("dev/aethel/ipc/SmokeTest.class").exists(),
            "run `make ipc-client` first"
        );

        let server = Arc::new(IpcServer::new().await.expect("ipc server"));
        let port = server.port();
        let accept = server.clone();
        tokio::spawn(async move {
            let _ = accept.accept_loop().await;
        });
        assert!(server.handle().set_theme(json!({"schema": 1})));
        let mut events = server.subscribe();

        let output = tokio::time::timeout(
            Duration::from_secs(60),
            tokio::process::Command::new("java")
                .arg("-cp")
                .arg(&classes)
                .arg("dev.aethel.ipc.SmokeTest")
                .arg(format!("ws://127.0.0.1:{port}"))
                .arg(server.token())
                .output(),
        )
        .await
        .expect("java run timed out")
        .expect("spawn java");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "smoke failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(stdout.contains("smoke ok"), "stdout:\n{stdout}");

        let mut saw_connected = false;
        let mut saw_launched = false;
        for _ in 0..50 {
            match events.try_recv() {
                Ok(IpcEvent::Connected { .. }) => saw_connected = true,
                Ok(IpcEvent::Launched { renderer, .. }) => {
                    assert_eq!(renderer, "vulkan");
                    saw_launched = true;
                }
                Ok(_) => {}
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
            if saw_connected && saw_launched {
                break;
            }
        }
        assert!(saw_connected, "connected event missing");
        assert!(saw_launched, "launched event missing");
    }
}
