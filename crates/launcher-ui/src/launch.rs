//! Background launch controller.
//!
//! Owns a worker thread with its own Tokio runtime so the egui frame loop is
//! never blocked by downloads or by the running game. Events flow back to the
//! UI over a plain `std::sync::mpsc` channel that [`LaunchController::poll`]
//! drains once per frame.

use launcher_core::auth::Account;
use launcher_core::install::pipeline::{prepare, InstallOptions, Progress, ProgressSink};
use launcher_core::launch::process::run_game;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// How many console lines to retain.
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
    /// What the performance step did (Fabric + mods + options).
    Perf(Box<launcher_core::perf::PerfReport>),
    /// Downloads finished and the JVM is about to spawn.
    Ready,
    Exited(Option<i32>),
    Failed(String),
}

/// Everything one launch needs.
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub version_id: String,
    /// Per-instance game directory (JVM working directory).
    pub game_dir: PathBuf,
    /// Shared root holding libraries/, assets/, versions/, runtimes/.
    pub shared_dir: PathBuf,
    pub ram_mb: u64,
    pub renderer: String,
    pub auth: Account,
    pub java_path: Option<PathBuf>,
    /// Fabric + performance mods + graphics preset.
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
    /// Current coarse phase, e.g. `"Downloading libraries"`.
    pub phase: String,
    /// Label of the in-flight download batch.
    pub progress_label: String,
    pub files_done: usize,
    pub files_total: usize,
    pub console: Vec<String>,
    pub error: Option<String>,
    pub started_at: Option<std::time::Instant>,
    /// Performance report from the most recent launch.
    pub last_perf: Option<launcher_core::perf::PerfReport>,
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
        }
    }

    /// Is a launch currently preparing or running?
    pub fn is_active(&self) -> bool {
        matches!(self.status, LaunchStatus::Preparing | LaunchStatus::Running)
    }

    /// Fraction complete for the current download batch, if any.
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

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let stop = Arc::new(tokio::sync::Notify::new());
        self.stop = stop.clone();

        std::thread::spawn(move || run_launch(request, tx, stop));
    }

    /// Ask the worker to terminate a running game (or abandon a launch in the
    /// JVM phase). Safe to call when nothing is running.
    pub fn stop(&self) {
        self.stop.notify_one();
    }

    /// Append a UI-local note to the console (e.g. "Stop requested").
    pub fn push_note(&mut self, line: impl Into<String>) {
        self.push_console(line.into());
    }

    /// Drain pending events. Call once per UI frame.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        for ev in events {
            self.handle(ev);
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
            LaunchEvent::Exited(code) => {
                self.status = LaunchStatus::Exited(code);
                self.phase = self.status.label();
                if let Some(code) = code {
                    if code != 0 {
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

    fn push_console(&mut self, line: String) {
        if self.console.len() >= CONSOLE_CAP {
            self.console.drain(0..CONSOLE_CAP / 4);
        }
        self.console.push(line);
    }
}

/// Entry point for the worker thread.
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

        let prepared = match prepare(&options, &sink).await {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(LaunchEvent::Failed(format!("{e:#}")));
                return;
            }
        };

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

        // Bridge the game's stdout/stderr into UI events while it runs.
        let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let forward_tx = tx.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(line) = log_rx.recv().await {
                if forward_tx.send(LaunchEvent::Log(line)).is_err() {
                    break;
                }
            }
        });

        let outcome = run_game(
            &prepared.java,
            &prepared.args,
            &prepared.main_class,
            &prepared.cwd,
            None,
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
}
