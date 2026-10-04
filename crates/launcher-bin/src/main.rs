use std::fs::OpenOptions;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const DEFAULT_LOG_FILTER: &str =
    "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,egui_wgpu=warn,tokio_tungstenite=warn";

fn main() -> anyhow::Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();

    tracing::info!("Aethel Launcher starting...");

    let home = launcher_core::CoreHandle::home_dir();
    let _ = std::fs::create_dir_all(&home);
    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(home.join("launcher.lock"))?;
    if let Err(e) = lock_file.try_lock() {
        tracing::warn!("another instance holds the lock ({e}) — exiting");
        eprintln!("Aethel Launcher is already running.");
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder {
            title: Some("Aethel Launcher".to_string()),
            inner_size: Some(egui::vec2(1200.0, 800.0)),
            min_inner_size: Some(egui::vec2(960.0, 600.0)),
            icon: Some(launcher_ui::branding::window_icon()),
            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "Aethel Launcher",
        options,
        Box::new(|cc| Ok(Box::new(launcher_ui::AethelApp::new(cc)))),
    )?;

    tracing::info!("Aethel Launcher exited.");
    drop(lock_file);
    Ok(())
}
