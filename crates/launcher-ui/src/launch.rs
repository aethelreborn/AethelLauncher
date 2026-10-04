use launcher_core::auth::Account;
use launcher_core::install::pipeline::{prepare, InstallOptions, Progress, ProgressSink};
use launcher_core::launch::process::run_game;
use launcher_core::launch::{IpcEvent, IpcHandle};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

const CONSOLE_CAP: usize = 4000;

#[derive(Debug)]
pub enum LaunchEvent {
    Stage(String),
    Files {
        completed: usize,
        total: usize,
        label: String,
    },
    Log(String),
    Perf(Box<launcher_core::perf::PerfReport>),
    Ready,
    Ipc {
        events: tokio::sync::broadcast::Receiver<IpcEvent>,
        handle: IpcHandle,
    },
    Exited(Option<i32>),
    Failed(String),
}

pub struct IpcConnection {
    pub events: tokio::sync::broadcast::Receiver<IpcEvent>,
    pub handle: IpcHandle,
}

#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub version_id: String,
    pub game_dir: PathBuf,
    pub shared_dir: PathBuf,
    pub ram_mb: u64,
    pub renderer: String,
    pub auth: Account,
    pub java_path: Option<PathBuf>,
    pub performance: launcher_core::perf::PerfConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LaunchStatus {
    #[default]
    Idle,
    Preparing,
    Running,
    Exited(Option<i32>),
    Failed,
}

impl LaunchStatus {
    pub fn label(&self) -> String {
        match self {
            LaunchStatus::Idle => "Idle".to_string(),
            LaunchStatus::Preparing => "Installing".to_string(),
            LaunchStatus::Running => "Running".to_string(),
            LaunchStatus::Exited(Some(0)) => "Exited cleanly".to_string(),
            LaunchStatus::Exited(Some(code)) => format!("Exited (code {code})"),
            LaunchStatus::Exited(None) => "Stopped".to_string(),
            LaunchStatus::Failed => "Failed".to_string(),
        }
    }
}

pub struct LaunchController {
    rx: Option<Receiver<LaunchEvent>>,
    stop: Arc<tokio::sync::Notify>,
    pub status: LaunchStatus,
    pub phase: String,
    pub progress_label: String,
    pub files_done: usize,
    pub files_total: usize,
    pub console: Vec<String>,
    pub error: Option<String>,
    pub started_at: Option<std::time::Instant>,
    pub last_perf: Option<launcher_core::perf::PerfReport>,
    crashed: bool,
    /// Set once a session connects, so the equipped cosmetics are pushed as soon
    /// as the in-game client is listening.
    needs_cosmetics_push: bool,
    pub ipc: Option<IpcConnection>,
    pub in_game: bool,
    pub game_fps: Option<f32>,
}

impl std::fmt::Debug for LaunchController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchController")
            .field("status", &self.status)
            .field("phase", &self.phase)
            .field("files_done", &self.files_done)
            .field("files_total", &self.files_total)
            .finish()
    }
}

impl Default for LaunchController {
    fn default() -> Self {
        Self::new()
    }
}

impl LaunchController {
    pub fn new() -> Self {
        Self {
            rx: None,
            stop: Arc::new(tokio::sync::Notify::new()),
            status: LaunchStatus::Idle,
            phase: String::new(),
            progress_label: String::new(),
            files_done: 0,
            files_total: 0,
            console: Vec::new(),
            error: None,
            started_at: None,
            last_perf: None,
            crashed: false,
            needs_cosmetics_push: false,
            ipc: None,
            in_game: false,
            game_fps: None,
        }
    }

    /// True once per session, right after the game connects its IPC socket.
    pub fn take_needs_cosmetics_push(&mut self) -> bool {
        std::mem::take(&mut self.needs_cosmetics_push)
    }

    /// Sends the equipped cosmetics to the in-game client. Returns true when a
    /// frame went out, so the caller can log or clear its dirty flag.
    pub fn push_cosmetics(&mut self, items: Vec<serde_json::Value>) -> bool {
        let Some(conn) = self.ipc.as_ref() else {
            return false;
        };
        let sent = conn.handle.push_cosmetics(items);
        if sent {
            self.push_console("[ipc] cosmetics pushed to game".to_string());
        }
        sent
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, LaunchStatus::Preparing | LaunchStatus::Running)
    }

    pub fn progress_fraction(&self) -> Option<f32> {
        if self.files_total == 0 {
            return None;
        }
        Some((self.files_done as f32 / self.files_total as f32).clamp(0.0, 1.0))
    }

    pub fn start(&mut self, request: LaunchRequest) {
        if self.is_active() {
            return;
        }

        self.console.clear();
        self.console
            .push(format!("== Aethel launch: {} ==", request.version_id));
        self.error = None;
        self.phase = "Starting".to_string();
        self.progress_label.clear();
        self.files_done = 0;
        self.files_total = 0;
        self.status = LaunchStatus::Preparing;
        self.started_at = Some(std::time::Instant::now());
        self.ipc = None;
        self.in_game = false;
        self.game_fps = None;

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let stop = Arc::new(tokio::sync::Notify::new());
        self.stop = stop.clone();

        std::thread::spawn(move || run_launch(request, tx, stop));
    }

    pub fn stop(&self) {
        self.stop.notify_one();
    }

    pub fn take_crash(&mut self) -> bool {
        std::mem::take(&mut self.crashed)
    }

    pub fn push_note(&mut self, line: impl Into<String>) {
        self.push_console(line.into());
    }

    pub fn poll(&mut self) {
        if let Some(rx) = &self.rx {
            let mut events = Vec::new();
            while let Ok(ev) = rx.try_recv() {
                events.push(ev);
            }
            for ev in events {
                self.handle(ev);
            }
        }

        let mut ipc_events = Vec::new();
        if let Some(conn) = &mut self.ipc {
            loop {
                match conn.events.try_recv() {
                    Ok(ev) => ipc_events.push(ev),
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                        ipc_events.push(IpcEvent::Closed);
                        break;
                    }
                }
            }
        }
        for ev in ipc_events {
            self.handle_ipc(ev);
        }
    }

    fn handle(&mut self, event: LaunchEvent) {
        match event {
            LaunchEvent::Stage(stage) => {
                self.phase = stage.clone();
                self.push_console(format!("[stage] {stage}"));
            }
            LaunchEvent::Files {
                completed,
                total,
                label,
            } => {
                self.progress_label = label;
                self.files_done = completed;
                self.files_total = total;
            }
            LaunchEvent::Log(line) => self.push_console(line),
            LaunchEvent::Perf(report) => {
                self.last_perf = Some(*report);
            }
            LaunchEvent::Ready => {
                self.status = LaunchStatus::Running;
                self.phase = "Running".to_string();
                self.files_done = 0;
                self.files_total = 0;
                self.progress_label.clear();
            }
            LaunchEvent::Ipc { events, handle } => {
                self.ipc = Some(IpcConnection { events, handle });
            }
            LaunchEvent::Exited(code) => {
                self.status = LaunchStatus::Exited(code);
                self.phase = self.status.label();
                self.in_game = false;
                self.game_fps = None;
                if let Some(code) = code {
                    if code != 0 {
                        self.crashed = true;
                        self.error = Some(format!(
                            "Minecraft exited with code {code}. See the log below."
                        ));
                    }
                }
            }
            LaunchEvent::Failed(message) => {
                self.status = LaunchStatus::Failed;
                self.phase = "Failed".to_string();
                self.push_console(format!("[error] {message}"));
                self.error = Some(message);
            }
        }
    }

    fn handle_ipc(&mut self, event: IpcEvent) {
        match event {
            IpcEvent::Connected { session_id } => {
                self.push_console(format!("[ipc] session {session_id}"));
                self.needs_cosmetics_push = true;
            }
            IpcEvent::Launched {
                renderer,
                width,
                height,
            } => {
                self.in_game = true;
                self.phase = "In game".to_string();
                self.push_console(format!(
                    "[ipc] game launched ({renderer}, {width}x{height})"
                ));
            }
            IpcEvent::Fps { fps, .. } => self.game_fps = Some(fps),
            IpcEvent::Crash { summary, .. } => {
                self.push_console(format!("[ipc] game crash: {summary}"));
            }
            IpcEvent::Closed => {
                if self.in_game {
                    self.push_console("[ipc] game session closed".to_string());
                }
                self.in_game = false;
                self.game_fps = None;
            }
            _ => {}
        }
    }

    fn push_console(&mut self, line: String) {
        if self.console.len() >= CONSOLE_CAP {
            self.console.drain(0..CONSOLE_CAP / 4);
        }
        self.console.push(line);
    }
}

fn run_launch(request: LaunchRequest, tx: Sender<LaunchEvent>, stop: Arc<tokio::sync::Notify>) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let _ = tx.send(LaunchEvent::Failed(format!(
                "failed to start async runtime: {e}"
            )));
            return;
        }
    };

    runtime.block_on(async move {
        let sink_tx = tx.clone();
        let sink: ProgressSink = Arc::new(move |p: Progress| {
            let event = match p {
                Progress::Stage(stage) => LaunchEvent::Stage(stage),
                Progress::Files {
                    completed,
                    total,
                    label,
                } => LaunchEvent::Files {
                    completed,
                    total,
                    label,
                },
                Progress::Log(line) => LaunchEvent::Log(line),
            };
            let _ = sink_tx.send(event);
        });

        let options = InstallOptions {
            version_id: request.version_id.clone(),
            game_dir: request.game_dir.clone(),
            shared_dir: request.shared_dir.clone(),
            ram_mb: request.ram_mb,
            renderer: request.renderer.clone(),
            auth: request.auth.clone(),
            java_path: request.java_path.clone(),
            performance: request.performance,
        };

        let mut prepared = match prepare(&options, &sink).await {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(LaunchEvent::Failed(format!("{e:#}")));
                return;
            }
        };

        // The in-game client reads the equipped cosmetics from the shared data dir when it boots.
        prepared
            .args
            .jvm_args
            .push(format!("-Daethel.home={}", request.shared_dir.display()));

        let _ = tx.send(LaunchEvent::Log(format!(
            "Java: {}",
            prepared.java.display()
        )));
        let _ = tx.send(LaunchEvent::Log(format!(
            "Main class: {}",
            prepared.main_class
        )));
        let _ = tx.send(LaunchEvent::Log(prepared.perf.summary()));
        for failure in &prepared.perf.failures {
            let _ = tx.send(LaunchEvent::Log(format!("⚠ {failure}")));
        }
        let _ = tx.send(LaunchEvent::Perf(Box::new(prepared.perf.clone())));
        let _ = tx.send(LaunchEvent::Ready);

        let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let forward_tx = tx.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(line) = log_rx.recv().await {
                if forward_tx.send(LaunchEvent::Log(line)).is_err() {
                    break;
                }
            }
        });

        let ipc = match launcher_core::launch::IpcServer::new().await {
            Ok(server) => {
                let server = Arc::new(server);
                let _ = tx.send(LaunchEvent::Ipc {
                    events: server.subscribe(),
                    handle: server.handle(),
                });
                let accept = server.clone();
                tokio::spawn(async move {
                    let _ = accept.accept_loop().await;
                });
                Some(server)
            }
            Err(e) => {
                let _ = tx.send(LaunchEvent::Log(format!("⚠ IPC unavailable: {e:#}")));
                None
            }
        };

        let outcome = run_game(
            &prepared.java,
            &prepared.args,
            &prepared.main_class,
            &prepared.cwd,
            ipc.as_deref(),
            log_tx,
            stop,
        )
        .await;

        let _ = forwarder.await;

        match outcome {
            Ok(code) => {
                let _ = tx.send(LaunchEvent::Exited(code));
            }
            Err(e) => {
                let _ = tx.send(LaunchEvent::Failed(format!("{e:#}")));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_fraction_is_none_until_files_report() {
        let mut c = LaunchController::new();
        assert!(c.progress_fraction().is_none());
        c.handle(LaunchEvent::Files {
            completed: 5,
            total: 10,
            label: "Assets".into(),
        });
        assert_eq!(c.progress_fraction(), Some(0.5));
    }

    #[test]
    fn events_drive_status_transitions() {
        let mut c = LaunchController::new();
        c.handle(LaunchEvent::Stage("Downloading client".into()));
        assert_eq!(c.phase, "Downloading client");

        c.handle(LaunchEvent::Ready);
        assert_eq!(c.status, LaunchStatus::Running);
        assert!(c.is_active());

        c.handle(LaunchEvent::Exited(Some(0)));
        assert!(!c.is_active());
        assert!(c.error.is_none());

        c.handle(LaunchEvent::Exited(Some(1)));
        assert!(c.error.is_some());
    }

    #[test]
    fn failure_records_error_and_logs() {
        let mut c = LaunchController::new();
        c.handle(LaunchEvent::Failed("boom".into()));
        assert_eq!(c.status, LaunchStatus::Failed);
        assert_eq!(c.error.as_deref(), Some("boom"));
        assert!(c.console.iter().any(|l| l.contains("boom")));
    }

    #[test]
    fn console_is_capped() {
        let mut c = LaunchController::new();
        for i in 0..(CONSOLE_CAP + 500) {
            c.push_console(format!("line {i}"));
        }
        assert!(c.console.len() <= CONSOLE_CAP);
    }

    #[test]
    fn ipc_launched_flips_to_in_game_state() {
        let mut c = LaunchController::new();
        c.handle(LaunchEvent::Ready);
        assert!(!c.in_game);

        c.handle_ipc(IpcEvent::Launched {
            renderer: "vulkan".into(),
            width: 1920,
            height: 1080,
        });
        assert!(c.in_game);
        assert_eq!(c.phase, "In game");
        assert!(c.console.iter().any(|l| l.contains("1920x1080")));

        c.handle_ipc(IpcEvent::Fps {
            fps: 144.0,
            frame_time_ms: 6.9,
        });
        assert_eq!(c.game_fps, Some(144.0));

        c.handle_ipc(IpcEvent::Closed);
        assert!(!c.in_game);
        assert!(c.game_fps.is_none());
    }

    #[test]
    fn game_exit_clears_in_game_state() {
        let mut c = LaunchController::new();
        c.handle(LaunchEvent::Ready);
        c.handle_ipc(IpcEvent::Launched {
            renderer: "opengl".into(),
            width: 800,
            height: 600,
        });
        c.game_fps = Some(60.0);

        c.handle(LaunchEvent::Exited(Some(0)));
        assert!(!c.in_game);
        assert!(c.game_fps.is_none());
    }
}
